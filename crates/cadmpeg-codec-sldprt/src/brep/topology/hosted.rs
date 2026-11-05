// SPDX-License-Identifier: Apache-2.0
//! Vertex readings that only a hosting codec admits.
//!
//! The main scan reads a vertex-use whose tolerance field is the null real or a
//! plausible tolerance. Solid Edge streams add two more shapes that the main
//! scan leaves out, because no `SolidWorks` fixture has verified them:
//!
//! - a vertex-use whose tolerance field holds neither value, immediately
//!   followed by the world point that its point cell names;
//! - a vertex index in a coedge that names a world point directly, with no
//!   vertex-use record between them.
//!
//! Both readings run after the scan, for the vertex indices that coedges name and
//! the tables do not hold.

use std::collections::HashSet;

use cadmpeg_core::decode::{DecodeContext, View};
use cadmpeg_core::CodecError;

use super::{attr_at, body_start, refs_be, refs_tripled, set_tolerance_at, Tables, VertexUse};

/// What the recovery added.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(in crate::brep) struct RecoveredVertices {
    /// Vertex-uses read in front of their world point.
    pub(in crate::brep) before_point: usize,
    /// Vertex indices that name a world point directly.
    pub(in crate::brep) point_as_vertex: usize,
}

/// Bytes from the attr of a vertex-use to its tolerance field, by cell layout.
const PLAIN_FIELD_AFTER_BODY_START: usize = 16;
const TRIPLED_FIELD_AFTER_BODY_START: usize = 21;
/// Width of the tolerance field.
const FIELD_WIDTH: usize = 8;

/// Add the vertex-uses that the main scan did not read.
///
/// `bodies` are the stream bodies the tables came from.
pub(in crate::brep) fn recover_vertex_uses(
    ctx: &DecodeContext<'_>,
    bodies: &[&[u8]],
    tables: &mut Tables,
) -> Result<RecoveredVertices, CodecError> {
    let mut recovered = RecoveredVertices::default();
    let mut missing: HashSet<u16> = HashSet::new();
    for coedge in tables.coedges.values() {
        ctx.charge_work(1, "collect missing Parasolid vertex indices")?;
        let vertex = coedge.refs[4];
        if vertex > 1 && !tables.vertex_uses.contains_key(&vertex) {
            ctx.insert_hash_set(
                &mut missing,
                vertex,
                "collect missing Parasolid vertex indices",
            )?;
        }
    }
    if missing.is_empty() {
        return Ok(recovered);
    }
    for buf in bodies {
        ctx.charge_work(
            cadmpeg_core::decode::u64_from_index(buf.len() / 64 + 1),
            "scan Parasolid vertex layouts",
        )?;
        let Some(last) = buf.len().checked_sub(2) else {
            continue;
        };
        for off in 0..last {
            if buf[off] != 0 || buf[off + 1] != 0x12 {
                continue;
            }
            let Some(record) = vertex_before_point(buf, off, tables) else {
                continue;
            };
            if missing.contains(&record.attr) && !tables.vertex_uses.contains_key(&record.attr) {
                tables.insert_vertex_use(ctx, record)?;
                recovered.before_point += 1;
            }
        }
    }
    let mut named: Vec<u16> = missing.into_iter().collect();
    ctx.stable_sort_by(
        &mut named,
        Ord::cmp,
        |_| 0,
        "order Parasolid vertex indices",
    )?;
    for vertex in named {
        if tables.vertex_uses.contains_key(&vertex) {
            continue;
        }
        let Some(point) = tables.points.get(&vertex) else {
            continue;
        };
        let record = VertexUse {
            attr: vertex,
            sequence: 0,
            refs: [0, 0, 0, 0, vertex],
            offset: point.offset,
            tolerance_m: None,
        };
        tables.insert_vertex_use(ctx, record)?;
        recovered.point_as_vertex += 1;
    }
    Ok(recovered)
}

/// A vertex-use in either cell layout whose tolerance field has any value and
/// that the world point named by its point cell follows directly.
fn vertex_before_point(buf: &[u8], off: usize, tables: &Tables) -> Option<VertexUse> {
    let p = body_start(buf, off, 0x12)?;
    let attr = attr_at(buf, p)?;
    let sequence = View::u32_be_at(buf, p + 2)?;
    let (refs, field_at) = if let Some(refs) = refs_tripled::<5>(buf, p + 6) {
        (refs, p + TRIPLED_FIELD_AFTER_BODY_START)
    } else {
        (refs_be::<5>(buf, p + 6)?, p + PLAIN_FIELD_AFTER_BODY_START)
    };
    let point = refs[4];
    if !tables.points.contains_key(&point) {
        return None;
    }
    let next = field_at + FIELD_WIDTH;
    let next_body = body_start(buf, next, 0x1d)?;
    (attr_at(buf, next_body) == Some(point)).then_some(())?;
    Some(VertexUse {
        attr,
        sequence,
        refs,
        offset: off,
        tolerance_m: set_tolerance_at(buf, field_at)
            .then(|| View::f64_be_at(buf, field_at))
            .flatten(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vertex_in_front_of_its_point_is_read_whatever_its_tolerance_field_holds() {
        let mut bytes = vec![0_u8, 0x12];
        bytes.extend(50_u16.to_be_bytes());
        bytes.extend(7_u32.to_be_bytes());
        for reference in [0_u16, 0, 0, 0, 60] {
            bytes.extend(reference.to_be_bytes());
        }
        bytes.extend(0.0_f64.to_be_bytes());
        let point_at = bytes.len();
        bytes.extend([0_u8, 0x1d]);
        bytes.extend(60_u16.to_be_bytes());
        bytes.resize(bytes.len() + 40, 0);
        let mut tables = Tables::default();
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(
            &bytes,
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("root");
        let point = super::super::parse_point(&ctx, &bytes, point_at, false)
            .unwrap()
            .expect("point");
        tables.insert_point(&ctx, point).unwrap();
        let vertex = vertex_before_point(&bytes, 0, &tables).expect("vertex");
        assert_eq!(
            (vertex.attr, vertex.refs[4], vertex.tolerance_m),
            (50, 60, None)
        );
        let mut other = bytes.clone();
        other[point_at + 3] = 61;
        assert!(vertex_before_point(&other, 0, &tables).is_none());
    }

    #[test]
    fn a_vertex_index_that_names_a_point_gets_a_vertex_use_on_that_point() {
        use super::super::Coedge;
        use cadmpeg_ir::topology::Sense;

        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("root");
        let mut bytes = vec![0_u8, 0x1d];
        bytes.extend(60_u16.to_be_bytes());
        bytes.extend(0_u32.to_be_bytes());
        bytes.extend([0_u8; 8]);
        for value in [0.01_f64, 0.02, 0.03] {
            bytes.extend(value.to_be_bytes());
        }
        let mut tables = Tables::default();
        let point = super::super::parse_point(&ctx, &bytes, 0, false)
            .unwrap()
            .expect("point");
        tables.insert_point(&ctx, point).unwrap();
        tables
            .insert_coedge(
                &ctx,
                Coedge {
                    attr: 5,
                    refs: [0, 0, 0, 0, 60, 0, 0, 0, 0],
                    sense: Sense::Forward,
                    offset: 0,
                },
            )
            .unwrap();
        let recovered = recover_vertex_uses(&ctx, &[], &mut tables).unwrap();
        assert_eq!(recovered.point_as_vertex, 1);
        assert_eq!(tables.vertex_uses[&60].refs[4], 60);
    }
}
