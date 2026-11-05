// SPDX-License-Identifier: Apache-2.0
//! Why the scan left out a record that a reference names, and the second pass
//! that admits such a record.
//!
//! The scan reads every reference as one big-endian u16 cell. A pointer above
//! `0x7ffe` takes two cells: the first has its high bit set and carries the low
//! fifteen bits, the second carries the rest, so the value is
//! `(first & 0x7fff) | (second << 15)`. A record with such a pointer is longer
//! by two bytes per wide pointer, so the fixed offsets of the scan put the
//! sense marker and the tolerance field in the wrong place and the scan leaves
//! the record out. A loop whose first coedge is left out goes with it.
//!
//! The second pass reads such a record with a cursor that follows the wide
//! pointers. It reads only at an index that a record in the tables names and the
//! tables do not hold, so the references vouch for the record.

use std::collections::HashSet;

use cadmpeg_core::decode::{u64_from_index, DecodeContext, View};
use cadmpeg_core::CodecError;
use cadmpeg_ir::topology::Sense;

use super::{
    body_start, parse_bridge, parse_coedge_candidates, parse_edge_use_candidates, parse_loop,
    parse_point, parse_sense, parse_vertex_use, Bridge, Coedge, EdgeReferences, EdgeUse, Loop,
    Tables, VertexUse, MAGIC, MAX_VERTEX_TOLERANCE_M, MIN_VERTEX_TOLERANCE_M,
};

/// The node types the second pass admits.
pub(in crate::brep) const ADMITTED_TYPES: [u8; 5] = [0x0e, 0x0f, 0x10, 0x11, 0x12];
/// The most readings of one index that the pass keeps.
const READINGS_PER_INDEX: usize = 6;
/// Pointers above this value take two cells.
const LAST_NARROW_POINTER: u16 = 0x7ffe;
/// Bytes of the tolerance field.
const TOLERANCE_WIDTH: usize = 8;
/// Bytes of the sequence field.
const SEQUENCE_WIDTH: usize = 4;
/// Scan cost: one work unit per this many stream bytes.
const BYTES_PER_WORK_UNIT: usize = 32;

/// What the second pass did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Admitted {
    /// Records admitted, by position in [`ADMITTED_TYPES`].
    pub(crate) by_type: [usize; 5],
    /// Indices whose readings differ and that the back-references do not settle.
    pub(crate) ambiguous: usize,
    /// Indices examined.
    pub(crate) examined: usize,
    /// Faces left out because an equal owner is already in the tables.
    pub(crate) duplicate_owner: usize,
    /// Admitted records that used at least one wide pointer.
    pub(crate) wide: usize,
}

/// A reader of big-endian cells that follows wide pointers.
struct Cells<'a> {
    buf: &'a [u8],
    at: usize,
    /// Wide pointers read so far.
    wide: usize,
}

impl<'a> Cells<'a> {
    fn new(buf: &'a [u8], at: usize) -> Self {
        Self { buf, at, wide: 0 }
    }

    /// A pointer, narrow or wide. A value that does not fit sixteen bits is
    /// refused, because the tables are keyed by sixteen bits.
    fn pointer(&mut self) -> Option<u16> {
        let first = View::u16_be_at(self.buf, self.at)?;
        self.at += 2;
        if first <= LAST_NARROW_POINTER {
            return Some(first);
        }
        let second = View::u16_be_at(self.buf, self.at)?;
        self.at += 2;
        self.wide += 1;
        u16::try_from(u32::from(first & 0x7fff) | (u32::from(second) << 15)).ok()
    }

    fn pointers<const N: usize>(&mut self) -> Option<[u16; N]> {
        let mut out = [0; N];
        for slot in &mut out {
            *slot = self.pointer()?;
        }
        Some(out)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.at = self.at.checked_add(count)?;
        (self.at <= self.buf.len()).then_some(())
    }

    fn sequence(&mut self) -> Option<u32> {
        let value = View::u32_be_at(self.buf, self.at)?;
        self.at += SEQUENCE_WIDTH;
        Some(value)
    }

    fn marker(&mut self) -> Option<Sense> {
        let sense = self.buf.get(self.at).copied().and_then(parse_sense)?;
        self.at += 1;
        Some(sense)
    }
}

/// The index of the record whose header is at `off`, and the cursor behind it.
fn header_cursor(buf: &[u8], off: usize, code: u8) -> Option<(u16, Cells<'_>)> {
    let mut cells = Cells::new(buf, body_start(buf, off, code)?);
    let index = cells.pointer().filter(|index| *index > 1)?;
    Some((index, cells))
}

/// A record read with the wide-pointer rule, and the wide pointers it used.
enum Relaxed {
    Bridge(Bridge),
    Loop(Loop),
    Edge(EdgeUse),
    Coedge(Coedge),
    Vertex(VertexUse),
}

fn read_coedge(buf: &[u8], off: usize) -> Option<(Relaxed, usize)> {
    let (attr, mut cells) = header_cursor(buf, off, 0x11)?;
    let refs = cells.pointers::<9>()?;
    let sense = cells.marker()?;
    let record = Coedge {
        attr,
        refs,
        sense,
        offset: off,
    };
    Some((Relaxed::Coedge(record), cells.wide))
}

fn read_loop(buf: &[u8], off: usize) -> Option<(Relaxed, usize)> {
    let (attr, mut cells) = header_cursor(buf, off, 0x0f)?;
    cells.skip(SEQUENCE_WIDTH)?;
    let refs = cells.pointers::<4>()?;
    let record = Loop {
        attr,
        refs,
        offset: off,
    };
    Some((Relaxed::Loop(record), cells.wide))
}

fn read_bridge(buf: &[u8], off: usize) -> Option<(Relaxed, usize)> {
    let (attr, mut cells) = header_cursor(buf, off, 0x0e)?;
    let sequence = cells.sequence()?;
    let owner = cells.pointer()?;
    cells.skip(TOLERANCE_WIDTH)?;
    let refs = cells.pointers::<5>()?;
    let sense = cells.marker()?;
    let record = Bridge {
        attr,
        refs,
        sequence,
        sense,
        owner: (owner > 1).then_some(owner),
        offset: off,
    };
    Some((Relaxed::Bridge(record), cells.wide))
}

fn read_vertex(buf: &[u8], off: usize) -> Option<(Relaxed, usize)> {
    let (attr, mut cells) = header_cursor(buf, off, 0x12)?;
    let sequence = cells.sequence()?;
    let refs = cells.pointers::<5>()?;
    let record = VertexUse {
        attr,
        refs,
        sequence,
        offset: off,
        tolerance_m: None,
    };
    Some((Relaxed::Vertex(record), cells.wide))
}

fn read_edge(buf: &[u8], off: usize) -> Option<(Relaxed, usize)> {
    let (attr, mut cells) = header_cursor(buf, off, 0x10)?;
    let sequence = cells.sequence()?;
    cells.pointer()?;
    cells.skip(TOLERANCE_WIDTH)?;
    let refs = cells.pointers::<6>()?;
    let record = EdgeUse {
        attr,
        references: EdgeReferences::Bare(refs),
        sequence,
        offset: off,
    };
    Some((Relaxed::Edge(record), cells.wide))
}

impl Relaxed {
    fn read(buf: &[u8], off: usize, code: u8) -> Option<(Self, usize)> {
        match code {
            0x0e => read_bridge(buf, off),
            0x0f => read_loop(buf, off),
            0x10 => read_edge(buf, off),
            0x11 => read_coedge(buf, off),
            0x12 => read_vertex(buf, off),
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

    /// How many of the references of the reading resolve in the tables, and for
    /// a loop or a coedge whether the record that it names points back at it.
    fn agreement(&self, tables: &Tables) -> usize {
        match self {
            Self::Bridge(b) => usize::from(
                tables
                    .loops
                    .get(&b.refs[2])
                    .is_some_and(|l| l.refs[2] == b.attr),
            ),
            Self::Loop(l) => {
                usize::from(tables.bridges.contains_key(&l.refs[2]))
                    + usize::from(tables.loops.contains_key(&l.refs[3]))
                    + 2 * usize::from(
                        tables
                            .coedges
                            .get(&l.refs[1])
                            .is_some_and(|coedge| coedge.refs[1] == l.attr),
                    )
            }
            Self::Edge(e) => usize::from(
                e.references
                    .canonical()
                    .is_some_and(|named| tables.coedges.contains_key(&named)),
            ),
            Self::Coedge(c) => {
                usize::from(tables.loops.contains_key(&c.refs[1]))
                    + usize::from(tables.vertex_uses.contains_key(&c.refs[4]))
                    + usize::from(tables.edge_uses.contains_key(&c.refs[6]))
                    + usize::from(
                        tables
                            .loops
                            .get(&c.refs[1])
                            .is_some_and(|l| l.refs[1] == c.attr),
                    )
                    + [2, 3, 5]
                        .into_iter()
                        .filter(|slot| tables.coedges.contains_key(&c.refs[*slot]))
                        .count()
            }
            Self::Vertex(v) => usize::from(tables.points.contains_key(&v.refs[4])),
        }
    }
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

/// The checks that the strict reading of the record at `off` fails, and what the
/// wide-pointer reading makes of it, as a JSON object. Every field is a number,
/// a flag, or a fixed word.
pub(in crate::brep) fn rejection_json(buf: &[u8], off: usize, code: u8, tables: &Tables) -> String {
    let Some(p) = body_start(buf, off, code) else {
        return "{\"header\":false}".to_owned();
    };
    let Some(remaining) = buf.len().checked_sub(p) else {
        return "{\"header\":false}".to_owned();
    };
    let wide = Relaxed::read(buf, off, code);
    let relaxed = format!(
        "\"relaxed\":{},\"wide\":{}",
        wide.is_some(),
        wide.as_ref()
            .map_or_else(|| "null".to_owned(), |(_, wide)| wide.to_string()),
    );
    match code {
        0x11 => {
            let candidates = parse_coedge_candidates(buf, off);
            format!(
                "{{\"remaining\":{remaining},\"strict\":{},\"m20\":{},\"m29\":{},\
                 \"marker_from_20\":{},\"marker_from_29\":{},{relaxed}}}",
                candidates.len(),
                byte_or_null(buf, p + 20),
                byte_or_null(buf, p + 29),
                marker_within(buf, p + 20, 16),
                marker_within(buf, p + 29, 16),
            )
        }
        0x0f => {
            let parsed = parse_loop(buf, off);
            let read = wide.as_ref().and_then(|(record, _)| match record {
                Relaxed::Loop(record) => Some(record),
                _ => None,
            });
            let owner = read.map(|record| record.refs[2]);
            let first = read.map(|record| record.refs[1]);
            format!(
                "{{\"remaining\":{remaining},\"strict\":{},\"owner_in_bridges\":{},\
                 \"first_in_coedges\":{},\"first_names_this_loop\":{},{relaxed}}}",
                usize::from(parsed.is_some()),
                owner.is_some_and(|owner| tables.bridges.contains_key(&owner)),
                first.is_some_and(|first| tables.coedges.contains_key(&first)),
                read.is_some_and(|record| tables
                    .coedges
                    .get(&record.refs[1])
                    .is_some_and(|coedge| coedge.refs[1] == record.attr)),
            )
        }
        0x0e => format!(
            "{{\"remaining\":{remaining},\"strict\":{},\"field8\":\"{}\",\"field9\":\"{}\",\
             \"tripled\":{},\"marker_from_26\":{},\"marker_from_31\":{},{relaxed}}}",
            usize::from(parse_bridge(buf, off).is_some()),
            field_class(buf, p + 8),
            field_class(buf, p + 9),
            (0..5).all(|index| buf.get(p + 18 + index * 3) == Some(&1)),
            marker_within(buf, p + 26, 16),
            marker_within(buf, p + 31, 16),
        ),
        0x10 => format!(
            "{{\"remaining\":{remaining},\"strict\":{},\"field8\":\"{}\",{relaxed}}}",
            parse_edge_use_candidates(buf, off).len(),
            field_class(buf, p + 8),
        ),
        0x12 => {
            let point_held = wide.as_ref().is_some_and(|(record, _)| match record {
                Relaxed::Vertex(record) => tables.points.contains_key(&record.refs[4]),
                _ => false,
            });
            format!(
                "{{\"remaining\":{remaining},\"strict\":{},\"field16\":\"{}\",\
                 \"field21\":\"{}\",\"point_held\":{point_held},{relaxed}}}",
                usize::from(parse_vertex_use(buf, off).is_some()),
                field_class(buf, p + 16),
                field_class(buf, p + 21),
            )
        }
        _ => format!("{{\"remaining\":{remaining}}}"),
    }
}

/// One header in a stream that the tables do not hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Header {
    code: u8,
    index: u16,
    stream: usize,
    off: usize,
}

/// The `(type, index)` pairs that a record names.
fn names(record: &Relaxed, out: &mut Vec<(u8, u16)>) {
    match record {
        Relaxed::Bridge(b) => out.push((0x0f, b.refs[2])),
        Relaxed::Loop(l) => out.extend([(0x0e, l.refs[2]), (0x11, l.refs[1]), (0x0f, l.refs[3])]),
        Relaxed::Coedge(c) => {
            out.push((0x0f, c.refs[1]));
            out.extend([2, 3, 5].map(|slot| (0x11, c.refs[slot])));
            out.extend([(0x12, c.refs[4]), (0x10, c.refs[6])]);
        }
        Relaxed::Edge(e) => out.extend(e.references.canonical().map(|named| (0x11, named))),
        Relaxed::Vertex(_) => {}
    }
}

fn held(tables: &Tables, code: u8, index: u16) -> bool {
    match code {
        0x0e => tables.bridges.contains_key(&index),
        0x0f => tables.loops.contains_key(&index),
        0x10 => tables.edge_uses.contains_key(&index),
        0x11 => tables.coedges.contains_key(&index),
        _ => tables.vertex_uses.contains_key(&index),
    }
}

/// The `(type, index)` pairs that records in the tables name and the tables do
/// not hold, in ascending order.
fn dangling(ctx: &DecodeContext<'_>, tables: &Tables) -> Result<Vec<(u8, u16)>, CodecError> {
    let mut start: Vec<(u8, u16)> = Vec::new();
    for record in tables.bridges.values() {
        names(&Relaxed::Bridge(record.clone()), &mut start);
    }
    for record in tables.loops.values() {
        names(&Relaxed::Loop(record.clone()), &mut start);
    }
    for record in tables.coedges.values() {
        names(&Relaxed::Coedge(record.clone()), &mut start);
    }
    for record in tables.edge_uses.values() {
        names(&Relaxed::Edge(record.clone()), &mut start);
    }
    ctx.charge_work(
        u64_from_index(start.len()),
        "collect dangling Parasolid references",
    )?;
    start.retain(|(code, index)| *index > 1 && !held(tables, *code, *index));
    ctx.stable_sort_by(
        &mut start,
        Ord::cmp,
        |_| 0,
        "order dangling Parasolid references",
    )?;
    start.dedup();
    Ok(start)
}

/// Admit the records that the tables name and the strict scan left out.
///
/// `bodies` are the stream bodies the tables came from, each with whether it is
/// a deltas stream. The pass runs until no admitted record names a new index.
pub(in crate::brep) fn admit_referenced(
    ctx: &DecodeContext<'_>,
    bodies: &[(&[u8], bool)],
    tables: &mut Tables,
) -> Result<Admitted, CodecError> {
    let mut admitted = Admitted::default();
    let mut queue = dangling(ctx, tables)?;
    if queue.is_empty() {
        return Ok(admitted);
    }
    // Every header of an admitted type that the tables do not hold, once.
    let mut headers: Vec<Header> = Vec::new();
    for (stream, (buf, _)) in bodies.iter().enumerate() {
        ctx.charge_work(
            u64_from_index(buf.len() / BYTES_PER_WORK_UNIT + 1),
            "scan Parasolid headers for referenced records",
        )?;
        let Some(last) = buf.len().checked_sub(2) else {
            continue;
        };
        for off in 0..last {
            if buf[off] != 0 || !ADMITTED_TYPES.contains(&buf[off + 1]) {
                continue;
            }
            let code = buf[off + 1];
            let Some((index, _)) = header_cursor(buf, off, code) else {
                continue;
            };
            if !held(tables, code, index) {
                ctx.push_vec(
                    &mut headers,
                    Header {
                        code,
                        index,
                        stream,
                        off,
                    },
                    "collect Parasolid headers for referenced records",
                )?;
            }
        }
    }
    ctx.stable_sort_by(&mut headers, Ord::cmp, |_| 0, "order Parasolid headers")?;
    let mut seen: HashSet<(u8, u16)> = HashSet::new();
    let mut owners: HashSet<u16> = tables
        .bridges
        .values()
        .filter_map(|bridge| bridge.owner)
        .collect();
    while let Some((code, index)) = queue.pop() {
        ctx.charge_work(1, "admit referenced Parasolid records")?;
        if held(tables, code, index)
            || !ctx.insert_hash_set(
                &mut seen,
                (code, index),
                "track referenced Parasolid records",
            )?
        {
            continue;
        }
        admitted.examined += 1;
        let from = headers.partition_point(|header| (header.code, header.index) < (code, index));
        let mut readings: Vec<(Relaxed, usize, bool)> = Vec::new();
        for header in headers[from..]
            .iter()
            .take_while(|header| (header.code, header.index) == (code, index))
        {
            let (buf, delta) = bodies[header.stream];
            if let Some((record, wide)) = Relaxed::read(buf, header.off, code) {
                if readings.len() < READINGS_PER_INDEX {
                    ctx.push_vec(
                        &mut readings,
                        (record, wide, delta),
                        "collect referenced Parasolid readings",
                    )?;
                }
            }
        }
        let Some(chosen) = choose(readings, tables) else {
            if headers[from..]
                .first()
                .is_some_and(|header| (header.code, header.index) == (code, index))
            {
                admitted.ambiguous += 1;
            }
            continue;
        };
        let (record, wide, _) = chosen;
        if let Relaxed::Bridge(bridge) = &record {
            if bridge.owner.is_some_and(|owner| owners.contains(&owner)) {
                admitted.duplicate_owner += 1;
                continue;
            }
            if let Some(owner) = bridge.owner {
                owners.insert(owner);
            }
        }
        let mut next = Vec::new();
        names(&record, &mut next);
        match record {
            Relaxed::Bridge(record) => {
                ctx.insert_hash_set(
                    &mut tables.admitted_bridges,
                    record.attr,
                    "track admitted Parasolid bridges",
                )?;
                tables.insert_bridge(ctx, record)?;
            }
            Relaxed::Loop(record) => tables.insert_loop(ctx, record)?,
            Relaxed::Edge(record) => tables.insert_edge_use(ctx, record)?,
            Relaxed::Coedge(record) => tables.insert_coedge(ctx, record)?,
            Relaxed::Vertex(record) => {
                // A vertex needs its point: admit the point when only its
                // header is in the stream.
                let point = record.refs[4];
                if !tables.points.contains_key(&point) {
                    admit_point(ctx, bodies, point, tables)?;
                }
                if !tables.points.contains_key(&point) {
                    continue;
                }
                tables.insert_vertex_use(ctx, record)?;
            }
        }
        if let Some(slot) = ADMITTED_TYPES.iter().position(|known| *known == code) {
            admitted.by_type[slot] += 1;
        }
        admitted.wide += usize::from(wide > 0);
        for (code, index) in next {
            if index > 1 && !held(tables, code, index) && !seen.contains(&(code, index)) {
                ctx.push_vec(
                    &mut queue,
                    (code, index),
                    "queue referenced Parasolid records",
                )?;
            }
        }
    }
    Ok(admitted)
}

/// Pick the reading of an index: the only one, the one that the tables agree
/// with most, or the only one from the partition.
fn choose(
    readings: Vec<(Relaxed, usize, bool)>,
    tables: &Tables,
) -> Option<(Relaxed, usize, bool)> {
    let mut distinct: Vec<(Relaxed, usize, bool)> = Vec::new();
    for reading in readings {
        if !distinct.iter().any(|known| known.0.same(&reading.0)) {
            distinct.push(reading);
        }
    }
    if distinct.len() <= 1 {
        return distinct.pop();
    }
    let best = distinct
        .iter()
        .map(|reading| reading.0.agreement(tables))
        .max()?;
    let mut leaders: Vec<_> = distinct
        .into_iter()
        .filter(|reading| reading.0.agreement(tables) == best)
        .collect();
    if leaders.len() > 1 {
        leaders.retain(|reading| !reading.2);
    }
    if leaders.len() == 1 {
        leaders.pop()
    } else {
        None
    }
}

/// Admit the world point behind a vertex whose point header the strict scan
/// did not accept.
fn admit_point(
    ctx: &DecodeContext<'_>,
    bodies: &[(&[u8], bool)],
    index: u16,
    tables: &mut Tables,
) -> Result<(), CodecError> {
    for (buf, _) in bodies {
        let Some(last) = buf.len().checked_sub(2) else {
            continue;
        };
        for off in 0..last {
            if buf[off] != 0 || buf[off + 1] != 0x1d {
                continue;
            }
            let Some(p) = body_start(buf, off, 0x1d) else {
                continue;
            };
            if View::u16_be_at(buf, p) != Some(index) {
                continue;
            }
            if let Some(point) = parse_point(ctx, buf, off, false)? {
                tables.insert_point(ctx, point)?;
                return Ok(());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A coedge record whose pointers are written narrow or wide.
    fn coedge_bytes(attr: u16, refs: [u32; 9], marker: u8) -> Vec<u8> {
        let mut bytes = vec![0_u8, 0x11];
        for value in std::iter::once(u32::from(attr)).chain(refs) {
            if value <= u32::from(LAST_NARROW_POINTER) {
                bytes.extend(u16::try_from(value).unwrap().to_be_bytes());
            } else {
                bytes.extend((0x8000 | u16::try_from(value & 0x7fff).unwrap()).to_be_bytes());
                bytes.extend(u16::try_from(value >> 15).unwrap().to_be_bytes());
            }
        }
        bytes.push(marker);
        bytes
    }

    fn context<'a>(
        bytes: &'a [u8],
        arena: &'a cadmpeg_core::decode::DecodeArena,
    ) -> DecodeContext<'a> {
        DecodeContext::from_root_bytes(bytes, arena, &cadmpeg_core::decode::DecodePolicy::service())
            .expect("root")
            .0
    }

    #[test]
    fn a_coedge_with_wide_pointers_is_read_where_the_strict_scan_fails() {
        let refs = [0, 20, 0, 3, 40_000, 0, 50_000, 0, 0];
        let mut bytes = coedge_bytes(33_000, refs, 0x2d);
        bytes.resize(bytes.len() + 24, 0);
        assert!(parse_coedge_candidates(&bytes, 0).is_empty());
        let (Relaxed::Coedge(record), wide) = read_coedge(&bytes, 0).expect("wide reading") else {
            panic!("coedge");
        };
        assert_eq!(wide, 3);
        assert_eq!(
            (record.attr, record.refs[4], record.refs[6], record.sense),
            (33_000, 40_000, 50_000, Sense::Reversed)
        );
        let line = rejection_json(&bytes, 0, 0x11, &Tables::default());
        assert!(
            line.contains("\"strict\":0") && line.contains("\"wide\":3"),
            "{line}"
        );
    }

    #[test]
    fn only_a_record_that_a_table_record_names_is_admitted() {
        let refs = [0, 20, 0, 3, 0, 0, 0, 0, 0];
        let mut bytes = coedge_bytes(33_000, refs, 0x2b);
        bytes.resize(bytes.len() + 24, 0);
        // A second such record that nothing names.
        bytes.extend(coedge_bytes(34_000, refs, 0x2b));
        bytes.resize(bytes.len() + 24, 0);
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let ctx = context(&bytes, &arena);
        let mut tables = Tables::default();
        tables
            .insert_loop(
                &ctx,
                Loop {
                    attr: 20,
                    refs: [0, 33_000, 10, 0],
                    offset: 0,
                },
            )
            .unwrap();
        let admitted = admit_referenced(&ctx, &[(&bytes, false)], &mut tables).unwrap();
        assert!(tables.coedges.contains_key(&33_000));
        assert!(!tables.coedges.contains_key(&34_000));
        assert_eq!((admitted.by_type[3], admitted.wide), (1, 1));
    }

    #[test]
    fn a_chain_of_coedges_is_followed_without_a_round_limit() {
        let mut bytes = Vec::new();
        let count: u16 = 40;
        for step in 0..count {
            let attr = 100 + step;
            let next = if step + 1 == count { 100 } else { attr + 1 };
            bytes.extend(coedge_bytes(
                attr,
                [0, 20, 0, u32::from(next), 0, 0, 0, 0, 0],
                0x2b,
            ));
            bytes.resize(bytes.len() + 4, 0);
        }
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let ctx = context(&bytes, &arena);
        let mut tables = Tables::default();
        tables
            .insert_loop(
                &ctx,
                Loop {
                    attr: 20,
                    refs: [0, 100, 10, 0],
                    offset: 0,
                },
            )
            .unwrap();
        let admitted = admit_referenced(&ctx, &[(&bytes, false)], &mut tables).unwrap();
        assert_eq!(tables.coedges.len(), usize::from(count));
        assert_eq!(admitted.by_type[3], usize::from(count));
    }

    #[test]
    fn two_readings_of_an_index_are_settled_by_the_partition_or_by_the_tables() {
        let refs = [0, 20, 0, 3, 0, 0, 0, 0, 0];
        let other = [0, 21, 0, 3, 0, 0, 0, 0, 0];
        let mut partition = coedge_bytes(500, refs, 0x2b);
        partition.resize(partition.len() + 24, 0);
        let mut delta = coedge_bytes(500, other, 0x2b);
        delta.resize(delta.len() + 24, 0);
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let ctx = context(&partition, &arena);
        let mut tables = Tables::default();
        tables
            .insert_loop(
                &ctx,
                Loop {
                    attr: 20,
                    refs: [0, 500, 10, 0],
                    offset: 0,
                },
            )
            .unwrap();
        let admitted =
            admit_referenced(&ctx, &[(&partition, false), (&delta, true)], &mut tables).unwrap();
        assert_eq!(admitted.by_type[3], 1);
        assert_eq!(tables.coedges[&500].refs[1], 20);
    }

    fn push_pointer(bytes: &mut Vec<u8>, value: u32) {
        if value <= u32::from(LAST_NARROW_POINTER) {
            bytes.extend(u16::try_from(value).unwrap().to_be_bytes());
        } else {
            bytes.extend((0x8000 | u16::try_from(value & 0x7fff).unwrap()).to_be_bytes());
            bytes.extend(u16::try_from(value >> 15).unwrap().to_be_bytes());
        }
    }

    #[test]
    fn a_bridge_with_wide_pointers_is_admitted_and_marked_as_admitted() {
        let mut bytes = vec![0_u8, 0x0e];
        push_pointer(&mut bytes, 40_000);
        bytes.extend(9_u32.to_be_bytes());
        push_pointer(&mut bytes, 77);
        bytes.extend([0_u8; TOLERANCE_WIDTH]);
        for value in [0, 0, 41_000, 0, 5] {
            push_pointer(&mut bytes, value);
        }
        bytes.push(0x2b);
        bytes.resize(bytes.len() + 24, 0);
        assert!(parse_bridge(&bytes, 0).is_none());
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let ctx = context(&bytes, &arena);
        let mut tables = Tables::default();
        tables
            .insert_loop(
                &ctx,
                Loop {
                    attr: 20,
                    refs: [0, 3, 40_000, 0],
                    offset: 0,
                },
            )
            .unwrap();
        let admitted = admit_referenced(&ctx, &[(&bytes, false)], &mut tables).unwrap();
        assert_eq!(admitted.by_type[0], 1);
        assert!(tables.is_admitted_bridge(40_000));
        assert_eq!(tables.bridges[&40_000].refs[2], 41_000);
        // The same owner a second time is a duplicate use and stays out.
        assert_eq!(admitted.duplicate_owner, 0);
    }
}
