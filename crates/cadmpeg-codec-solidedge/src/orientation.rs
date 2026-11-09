// SPDX-License-Identifier: Apache-2.0
//! Direction of the hosted loop rings and edges.
//!
//! The shared Parasolid decoder reads a hosted coedge ring in the order of its
//! link cells and takes the vertex-use cell of a coedge as the vertex where the
//! coedge starts. On Solid Edge streams this gives every loop the clockwise
//! direction about the face normal that its face record states, and it gives
//! every arc edge without a stored parameter range its longer way round
//! (SE-20). Both are one reversal of the traversal: the ring is the reverse of
//! the loop and each vertex-use cell names the vertex where its coedge ends.
//! This pass applies that reversal to the decoded model.
//!
//! An edge that has no parameter range runs along its curve from the start
//! vertex to the end vertex, so its ends swap and the senses of its coedges
//! stay. An edge that keeps a range has its ends set by the curve, so they stay
//! and the senses of its coedges flip. A closed edge runs the whole curve from
//! its one vertex; its coedges flip.

use std::collections::HashSet;

use cadmpeg_core::decode::DecodeContext;
use cadmpeg_core::CodecError;
use cadmpeg_ir::document::CadIr;
use cadmpeg_ir::topology::{LoopBoundary, Sense};

/// What the reversal changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Reversal {
    /// Edges whose start and end swapped.
    pub(crate) edges: usize,
    /// Loop rings that were reversed.
    pub(crate) loops: usize,
}

/// Reverse the traversal of every loop of the hosted model.
///
/// # Errors
///
/// Refuses when the decode budget or the allocator does not admit a rebuilt
/// ring.
pub(crate) fn reverse_hosted_traversal(
    ctx: &DecodeContext<'_>,
    ir: &mut CadIr,
) -> Result<Reversal, CodecError> {
    let mut outcome = Reversal::default();
    let mut swapped = HashSet::new();
    for edge in &mut ir.model.edges {
        ctx.charge_work(1, "reverse Solid Edge edges")?;
        if edge.param_range().is_some() || edge.start == edge.end {
            continue;
        }
        std::mem::swap(&mut edge.start, &mut edge.end);
        ctx.insert_hash_set(
            &mut swapped,
            edge.id.clone(),
            "track reversed Solid Edge edges",
        )?;
        outcome.edges += 1;
    }
    for coedge in &mut ir.model.coedges {
        ctx.charge_work(1, "reverse Solid Edge coedges")?;
        if !swapped.contains(&coedge.edge) {
            coedge.sense = match coedge.sense {
                Sense::Forward => Sense::Reversed,
                Sense::Reversed => Sense::Forward,
            };
        }
    }
    for lp in &mut ir.model.loops {
        ctx.charge_work(1, "reverse Solid Edge loops")?;
        let LoopBoundary::Ring(ring) = &lp.boundary else {
            continue;
        };
        let mut coedges = Vec::new();
        ctx.reserve_vec(
            &mut coedges,
            ring.coedges().len(),
            "reverse Solid Edge loop ring",
        )?;
        coedges.extend(ring.coedges().iter().rev().cloned());
        let vertex_uses = ring.vertex_uses().to_vec();
        lp.replace_ring(ctx, coedges, vertex_uses)
            .map_err(|error| CodecError::malformed(error.to_string()))?;
        outcome.loops += 1;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_decode_context;
    use cadmpeg_ir::ids::VertexId;
    use cadmpeg_ir::topology::EdgeCarrier;
    use std::collections::HashMap;

    /// The vertices where each coedge of the loops starts and ends.
    fn traversal(ir: &CadIr) -> Vec<(String, Vec<(VertexId, VertexId)>)> {
        let edges: HashMap<_, _> = ir.model.edges.iter().map(|e| (&e.id, e)).collect();
        let coedges: HashMap<_, _> = ir.model.coedges.iter().map(|c| (&c.id, c)).collect();
        ir.model
            .loops
            .iter()
            .map(|lp| {
                let ends = lp
                    .coedges()
                    .iter()
                    .map(|id| {
                        let coedge = coedges[id];
                        let edge = edges[&coedge.edge];
                        match coedge.sense {
                            Sense::Forward => (edge.start.clone(), edge.end.clone()),
                            Sense::Reversed => (edge.end.clone(), edge.start.clone()),
                        }
                    })
                    .collect();
                (lp.id.as_str().to_owned(), ends)
            })
            .collect()
    }

    /// A cube whose edges with an even index have lost their parameter range.
    fn cube_with_unranged_edges() -> CadIr {
        let mut ir = cadmpeg_ir::examples::unit_cube().expect("the example cube builds");
        for (index, edge) in ir.model.edges.iter_mut().enumerate() {
            if index % 2 == 0 {
                let curve = edge.curve().cloned();
                edge.carrier = EdgeCarrier::unbounded(curve);
            }
        }
        ir
    }

    #[test]
    fn reversal_runs_every_loop_the_other_way_and_keeps_it_closed() {
        let mut ir = cube_with_unranged_edges();
        let before = traversal(&ir);
        with_decode_context(&[], |ctx| {
            let outcome = reverse_hosted_traversal(ctx, &mut ir).expect("reversal is admitted");
            assert_eq!(outcome.edges, 6);
            assert_eq!(outcome.loops, 6);
        });
        let after = traversal(&ir);
        for ((id, was), (_, now)) in before.iter().zip(&after) {
            assert_eq!(was.len(), now.len(), "{id}");
            // The new ring is the old one backwards, and each coedge runs the other way.
            for (position, (start, end)) in now.iter().enumerate() {
                let (old_start, old_end) = &was[was.len() - 1 - position];
                assert_eq!((start, end), (old_end, old_start), "{id} coedge {position}");
            }
            // Each coedge ends where the next one starts.
            for position in 0..now.len() {
                assert_eq!(now[position].1, now[(position + 1) % now.len()].0, "{id}");
            }
        }
    }

    #[test]
    fn an_edge_with_a_range_keeps_its_ends_and_an_unranged_edge_swaps_them() {
        let mut ir = cube_with_unranged_edges();
        let ends: Vec<_> = ir
            .model
            .edges
            .iter()
            .map(|e| (e.start.clone(), e.end.clone(), e.param_range().is_some()))
            .collect();
        with_decode_context(&[], |ctx| {
            reverse_hosted_traversal(ctx, &mut ir).expect("reversal is admitted");
        });
        for (edge, (start, end, ranged)) in ir.model.edges.iter().zip(ends) {
            if ranged {
                assert_eq!((&edge.start, &edge.end), (&start, &end));
            } else {
                assert_eq!((&edge.start, &edge.end), (&end, &start));
            }
        }
    }
}
