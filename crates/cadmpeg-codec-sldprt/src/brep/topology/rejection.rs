// SPDX-License-Identifier: Apache-2.0
//! Why the scan left out a record that a reference names, and the second pass
//! that admits such a record.
//!
//! The scan keeps a record only when it passes strict checks on length, the
//! tolerance field, and the sense marker. A hosted stream can hold records that
//! fail those checks and that other records name. The second pass reads such a
//! record under relaxed rules, but only at an index that a record in the tables
//! names and the tables do not hold, so the references vouch for it.

use std::collections::{HashMap, HashSet};

use cadmpeg_core::decode::{u64_from_index, DecodeContext, View};
use cadmpeg_core::CodecError;
use cadmpeg_ir::topology::Sense;

use super::{
    attr_at, body_start, parse_bridge, parse_coedge_candidates, parse_edge_use_candidates,
    parse_loop, parse_sense, parse_vertex_use, refs_be, refs_tripled, Bridge, Coedge,
    EdgeReferences, EdgeUse, Loop, Tables, VertexUse, MAGIC, MAX_VERTEX_TOLERANCE_M,
    MIN_VERTEX_TOLERANCE_M,
};

/// Extra reference cells that the relaxed readings allow before a marker.
const EXTRA_CELLS: usize = 3;
/// The most rounds of the second pass; an admitted record can name more records.
const ROUNDS: usize = 8;
/// The node types the second pass admits.
pub(in crate::brep) const ADMITTED_TYPES: [u8; 5] = [0x0e, 0x0f, 0x10, 0x11, 0x12];
/// The node types that open a record, for the adjacency check.
const RECORD_TYPES: [u8; 6] = [0x0e, 0x0f, 0x10, 0x11, 0x12, 0x1d];

/// What the second pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Admitted {
    /// Records admitted, by position in [`ADMITTED_TYPES`].
    pub(crate) by_type: [usize; 5],
    /// Referenced indices with several different relaxed readings.
    pub(crate) ambiguous: usize,
    /// Rounds that admitted at least one record.
    pub(crate) rounds: usize,
}

/// A field of eight bytes, classified by what it holds.
fn field_class(buf: &[u8], at: usize) -> &'static str {
    if buf.get(at..at + MAGIC.len()) == Some(MAGIC.as_slice()) {
        return "null";
    }
    match View::f64_be_at(buf, at) {
        None => "short",
        Some(0.0) => "zero",
        Some(value) if !value.is_finite() => "nonfinite",
        Some(value) if (MIN_VERTEX_TOLERANCE_M..=MAX_VERTEX_TOLERANCE_M).contains(&value.abs()) => {
            "tol"
        }
        Some(_) => "other",
    }
}

fn byte_or_null(buf: &[u8], at: usize) -> String {
    buf.get(at)
        .map_or_else(|| "null".to_owned(), ToString::to_string)
}

/// Offset from `from` of the first sense marker in the next `span` bytes.
fn marker_within(buf: &[u8], from: usize, span: usize) -> String {
    (0..span)
        .find(|step| {
            buf.get(from + step)
                .copied()
                .and_then(parse_sense)
                .is_some()
        })
        .map_or_else(|| "null".to_owned(), |step| step.to_string())
}

/// The checks that the strict reading of the record at `off` fails, as the body
/// of a JSON object. Every field is a number, a flag, or a fixed word.
pub(in crate::brep) fn rejection_json(buf: &[u8], off: usize, code: u8, tables: &Tables) -> String {
    let Some(p) = body_start(buf, off, code) else {
        return "{\"header\":false}".to_owned();
    };
    let Some(remaining) = buf.len().checked_sub(p) else {
        return "{\"header\":false}".to_owned();
    };
    match code {
        0x11 => {
            let candidates = parse_coedge_candidates(buf, off);
            format!(
                "{{\"remaining\":{remaining},\"strict\":{},\"m20\":{},\"m29\":{},\
                 \"marker_from_20\":{},\"marker_from_29\":{}}}",
                candidates.len(),
                byte_or_null(buf, p + 20),
                byte_or_null(buf, p + 29),
                marker_within(buf, p + 20, 16),
                marker_within(buf, p + 29, 16),
            )
        }
        0x0f => {
            let parsed = parse_loop(buf, off);
            let owner = parsed.as_ref().map(|record| record.refs[2]);
            let first = parsed.as_ref().map(|record| record.refs[1]);
            format!(
                "{{\"remaining\":{remaining},\"strict\":{},\"owner_in_bridges\":{},\
                 \"first_in_coedges\":{},\"first_names_this_loop\":{}}}",
                usize::from(parsed.is_some()),
                owner.is_some_and(|owner| tables.bridges.contains_key(&owner)),
                first.is_some_and(|first| tables.coedges.contains_key(&first)),
                first
                    .zip(attr_at(buf, p))
                    .is_some_and(|(first, attr)| tables
                        .coedges
                        .get(&first)
                        .is_some_and(|coedge| coedge.refs[1] == attr)),
            )
        }
        0x0e => format!(
            "{{\"remaining\":{remaining},\"strict\":{},\"field8\":\"{}\",\"field9\":\"{}\",\
             \"tripled\":{},\"marker_from_26\":{},\"marker_from_31\":{}}}",
            usize::from(parse_bridge(buf, off).is_some()),
            field_class(buf, p + 8),
            field_class(buf, p + 9),
            (0..5).all(|index| buf.get(p + 18 + index * 3) == Some(&1)),
            marker_within(buf, p + 26, 16),
            marker_within(buf, p + 31, 16),
        ),
        0x10 => format!(
            "{{\"remaining\":{remaining},\"strict\":{},\"field8\":\"{}\"}}",
            parse_edge_use_candidates(buf, off).len(),
            field_class(buf, p + 8),
        ),
        0x12 => format!(
            "{{\"remaining\":{remaining},\"strict\":{},\"field16\":\"{}\",\"field21\":\"{}\",\
             \"plain_refs\":{},\"tripled_refs\":{}}}",
            usize::from(parse_vertex_use(buf, off).is_some()),
            field_class(buf, p + 16),
            field_class(buf, p + 21),
            refs_be::<5>(buf, p + 6).is_some(),
            refs_tripled::<5>(buf, p + 6).is_some(),
        ),
        _ => format!("{{\"remaining\":{remaining}}}"),
    }
}

/// Whether a record header follows at `at`.
fn header_follows(buf: &[u8], at: usize) -> bool {
    buf.get(at) == Some(&0)
        && buf
            .get(at + 1)
            .is_some_and(|code| RECORD_TYPES.contains(code))
}

/// The marker of a record whose reference array has extra cells: the first
/// sense marker at `first + step * extra`, with a record header right after it
/// when `extra` is not zero.
fn marker_with_extra(buf: &[u8], first: usize, step: usize) -> Option<Sense> {
    (0..=EXTRA_CELLS).find_map(|extra| {
        let at = first + step * extra;
        let sense = buf.get(at).copied().and_then(parse_sense)?;
        (extra == 0 || header_follows(buf, at + 1)).then_some(sense)
    })
}

fn relaxed_coedge(buf: &[u8], off: usize) -> Option<Coedge> {
    let p = body_start(buf, off, 0x11)?;
    let attr = attr_at(buf, p)?;
    let (refs, sense) = if let Some(refs) = refs_be::<9>(buf, p + 2) {
        (refs, marker_with_extra(buf, p + 20, 2)?)
    } else {
        (
            refs_tripled::<9>(buf, p + 2)?,
            marker_with_extra(buf, p + 29, 3)?,
        )
    };
    Some(Coedge {
        attr,
        refs,
        sense,
        offset: off,
    })
}

fn relaxed_bridge(buf: &[u8], off: usize) -> Option<Bridge> {
    let p = body_start(buf, off, 0x0e)?;
    let attr = attr_at(buf, p)?;
    let sequence = View::u32_be_at(buf, p + 2)?;
    let owner = View::u16_be_at(buf, p + 6)?;
    let tripled = (0..5).all(|index| buf.get(p + 18 + index * 3) == Some(&1));
    let (refs, sense) = if tripled {
        (
            refs_tripled::<5>(buf, p + 16)?,
            marker_with_extra(buf, p + 31, 3)?,
        )
    } else {
        (
            refs_be::<5>(buf, p + 16)?,
            marker_with_extra(buf, p + 26, 2)?,
        )
    };
    Some(Bridge {
        attr,
        refs,
        sequence,
        sense,
        owner: (owner > 1).then_some(owner),
        offset: off,
    })
}

fn relaxed_vertex(buf: &[u8], off: usize, tables: &Tables) -> Option<VertexUse> {
    let p = body_start(buf, off, 0x12)?;
    let attr = attr_at(buf, p)?;
    let sequence = View::u32_be_at(buf, p + 2)?;
    let refs = refs_tripled::<5>(buf, p + 6).or_else(|| refs_be::<5>(buf, p + 6))?;
    tables.points.contains_key(&refs[4]).then_some(())?;
    Some(VertexUse {
        attr,
        refs,
        sequence,
        offset: off,
        tolerance_m: None,
    })
}

fn relaxed_edge(buf: &[u8], off: usize) -> Option<EdgeUse> {
    let p = body_start(buf, off, 0x10)?;
    let attr = attr_at(buf, p)?;
    let sequence = View::u32_be_at(buf, p + 2)?;
    Some(EdgeUse {
        attr,
        references: EdgeReferences::Bare(refs_be::<6>(buf, p + 16)?),
        sequence,
        offset: off,
    })
}

/// A record read under the relaxed rules.
enum Relaxed {
    Bridge(Bridge),
    Loop(Loop),
    Edge(EdgeUse),
    Coedge(Coedge),
    Vertex(VertexUse),
}

impl Relaxed {
    fn read(buf: &[u8], off: usize, code: u8, tables: &Tables) -> Option<Self> {
        match code {
            0x0e => relaxed_bridge(buf, off).map(Self::Bridge),
            0x0f => parse_loop(buf, off).map(Self::Loop),
            0x10 => relaxed_edge(buf, off).map(Self::Edge),
            0x11 => relaxed_coedge(buf, off).map(Self::Coedge),
            0x12 => relaxed_vertex(buf, off, tables).map(Self::Vertex),
            _ => None,
        }
    }

    /// Whether two readings state the same references, whatever their offset.
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bridge(a), Self::Bridge(b)) => a.refs == b.refs && a.sense == b.sense,
            (Self::Loop(a), Self::Loop(b)) => a.refs == b.refs,
            (Self::Edge(a), Self::Edge(b)) => a.references == b.references,
            (Self::Coedge(a), Self::Coedge(b)) => a.refs == b.refs && a.sense == b.sense,
            (Self::Vertex(a), Self::Vertex(b)) => a.refs == b.refs,
            _ => false,
        }
    }
}

/// The `(type, index)` pairs that records in the tables name and that the
/// tables do not hold.
fn dangling(ctx: &DecodeContext<'_>, tables: &Tables) -> Result<HashSet<(u8, u16)>, CodecError> {
    let mut wanted: HashSet<(u8, u16)> = HashSet::new();
    let mut want = |ctx: &DecodeContext<'_>, code: u8, index: u16| -> Result<(), CodecError> {
        let held = match code {
            0x0e => tables.bridges.contains_key(&index),
            0x0f => tables.loops.contains_key(&index),
            0x10 => tables.edge_uses.contains_key(&index),
            0x11 => tables.coedges.contains_key(&index),
            _ => tables.vertex_uses.contains_key(&index),
        };
        if index > 1 && !held {
            ctx.insert_hash_set(
                &mut wanted,
                (code, index),
                "collect dangling Parasolid references",
            )?;
        }
        Ok(())
    };
    for bridge in tables.bridges.values() {
        ctx.charge_work(1, "collect dangling Parasolid references")?;
        want(ctx, 0x0f, bridge.refs[2])?;
    }
    for record in tables.loops.values() {
        ctx.charge_work(1, "collect dangling Parasolid references")?;
        want(ctx, 0x0e, record.refs[2])?;
        want(ctx, 0x11, record.refs[1])?;
        want(ctx, 0x0f, record.refs[3])?;
    }
    for coedge in tables.coedges.values() {
        ctx.charge_work(1, "collect dangling Parasolid references")?;
        want(ctx, 0x0f, coedge.refs[1])?;
        for slot in [2, 3, 5] {
            want(ctx, 0x11, coedge.refs[slot])?;
        }
        want(ctx, 0x12, coedge.refs[4])?;
        want(ctx, 0x10, coedge.refs[6])?;
    }
    for edge in tables.edge_uses.values() {
        ctx.charge_work(1, "collect dangling Parasolid references")?;
        if let Some(canonical) = edge.references.canonical() {
            want(ctx, 0x11, canonical)?;
        }
    }
    Ok(wanted)
}

/// Admit the records that the tables name and the strict scan left out.
///
/// `bodies` are the stream bodies the tables came from.
pub(in crate::brep) fn admit_referenced(
    ctx: &DecodeContext<'_>,
    bodies: &[&[u8]],
    tables: &mut Tables,
) -> Result<Admitted, CodecError> {
    let mut admitted = Admitted::default();
    for _ in 0..ROUNDS {
        let wanted = dangling(ctx, tables)?;
        if wanted.is_empty() {
            break;
        }
        let mut found: HashMap<(u8, u16), Vec<Relaxed>> = HashMap::new();
        for buf in bodies {
            ctx.charge_work(
                u64_from_index(buf.len() / 32 + 1),
                "scan referenced Parasolid records",
            )?;
            let Some(last) = buf.len().checked_sub(2) else {
                continue;
            };
            for off in 0..last {
                if buf[off] != 0 || !ADMITTED_TYPES.contains(&buf[off + 1]) {
                    continue;
                }
                let code = buf[off + 1];
                let Some(index) = body_start(buf, off, code).and_then(|p| attr_at(buf, p)) else {
                    continue;
                };
                if !wanted.contains(&(code, index)) {
                    continue;
                }
                if let Some(record) = Relaxed::read(buf, off, code, tables) {
                    let slot = found.entry((code, index)).or_default();
                    if slot.len() < 4 {
                        ctx.push_vec(slot, record, "collect referenced Parasolid records")?;
                    }
                }
            }
        }
        let mut round = 0;
        let mut keys: Vec<(u8, u16)> = found.keys().copied().collect();
        ctx.stable_sort_by(
            &mut keys,
            Ord::cmp,
            |_| 0,
            "order referenced Parasolid records",
        )?;
        for key in keys {
            let Some(mut readings) = found.remove(&key) else {
                continue;
            };
            let first = readings.remove(0);
            if readings.iter().any(|other| !first.same(other)) {
                admitted.ambiguous += 1;
                continue;
            }
            let slot = ADMITTED_TYPES.iter().position(|code| *code == key.0);
            match first {
                Relaxed::Bridge(record) => tables.insert_bridge(ctx, record)?,
                Relaxed::Loop(record) => tables.insert_loop(ctx, record)?,
                Relaxed::Edge(record) => tables.insert_edge_use(ctx, record)?,
                Relaxed::Coedge(record) => tables.insert_coedge(ctx, record)?,
                Relaxed::Vertex(record) => tables.insert_vertex_use(ctx, record)?,
            }
            if let Some(slot) = slot {
                admitted.by_type[slot] += 1;
            }
            round += 1;
        }
        if round == 0 {
            break;
        }
        admitted.rounds += 1;
    }
    Ok(admitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coedge_bytes(attr: u16, refs: [u16; 9], gap: usize, marker: u8) -> Vec<u8> {
        let mut bytes = vec![0_u8, 0x11];
        bytes.extend(attr.to_be_bytes());
        for reference in refs {
            bytes.extend(reference.to_be_bytes());
        }
        bytes.extend(vec![0_u8; gap]);
        bytes.push(marker);
        bytes
    }

    #[test]
    fn a_coedge_with_an_extra_cell_is_read_when_a_header_follows_its_marker() {
        let refs = [0, 7, 0, 3, 50, 0, 60, 0, 0];
        let mut bytes = coedge_bytes(3, refs, 2, 0x2d);
        bytes.extend([0_u8, 0x11]);
        bytes.resize(bytes.len() + 24, 0);
        assert!(parse_coedge_candidates(&bytes, 0).is_empty());
        let record = relaxed_coedge(&bytes, 0).expect("relaxed reading");
        assert_eq!(
            (record.attr, record.refs[1], record.sense),
            (3, 7, Sense::Reversed)
        );
        // Without a header after the marker the extra cell is not believed.
        let mut lone = coedge_bytes(3, refs, 2, 0x2d);
        lone.resize(lone.len() + 24, 0);
        assert!(relaxed_coedge(&lone, 0).is_none());
    }

    #[test]
    fn the_rejection_of_a_coedge_names_the_marker_bytes() {
        let refs = [0, 7, 0, 3, 50, 0, 60, 0, 0];
        let mut bytes = coedge_bytes(3, refs, 2, 0x2d);
        bytes.resize(bytes.len() + 24, 0);
        let line = rejection_json(&bytes, 0, 0x11, &Tables::default());
        assert!(line.contains("\"strict\":0"), "{line}");
        assert!(line.contains("\"marker_from_20\":2"), "{line}");
    }

    #[test]
    fn only_a_record_that_a_table_record_names_is_admitted() {
        let refs = [0, 20, 0, 3, 0, 0, 0, 0, 0];
        let mut bytes = coedge_bytes(3, refs, 2, 0x2b);
        bytes.extend([0_u8, 0x11]);
        bytes.resize(bytes.len() + 24, 0);
        // A second such record that nothing names.
        let other = bytes.len();
        bytes.extend(coedge_bytes(9, refs, 2, 0x2b));
        bytes.extend([0_u8, 0x11]);
        bytes.resize(bytes.len() + 24, 0);
        assert!(other > 0);
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(
            &bytes,
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("root");
        let mut tables = Tables::default();
        tables
            .insert_loop(
                &ctx,
                Loop {
                    attr: 20,
                    refs: [0, 3, 10, 0],
                    offset: 0,
                },
            )
            .unwrap();
        let admitted = admit_referenced(&ctx, &[&bytes], &mut tables).unwrap();
        assert!(tables.coedges.contains_key(&3));
        assert!(!tables.coedges.contains_key(&9));
        assert_eq!(admitted.by_type[3], 1);
    }
}
