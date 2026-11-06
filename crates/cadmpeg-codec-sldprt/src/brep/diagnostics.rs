// SPDX-License-Identifier: Apache-2.0
//! Structural diagnostics for edges that a hosted decode drops.
//!
//! A host opts in. For each dropped edge the diagnostics state which record
//! the edge reference names and what the streams hold at that index: type
//! codes, offsets between neighbouring records, flags, and counts. They never
//! carry a name, a string, a value, or a coordinate. Each finding becomes one
//! JSON line whose fields are integers, booleans, and fixed words.

use std::collections::{HashMap, HashSet};

use cadmpeg_core::decode::{u64_from_index, DecodeContext, View};
use cadmpeg_core::CodecError;
use cadmpeg_ir::topology::Sense;

use super::topology::Tables;

/// The most findings a decode keeps, so that a damaged file stays cheap.
pub(crate) const FINDING_LIMIT: usize = 20_000;
/// The most distinct (type, stream) headers the probe keeps for one index.
const HITS_PER_INDEX: usize = 12;
/// The most coedges one finding lists.
const COEDGES_PER_FINDING: usize = 8;
/// The span after a node header in which the probe looks for the null real.
const MAGIC_WINDOW: usize = 64;
/// The span after a node header in which the probe looks for the next header.
const NEXT_HEADER_WINDOW: usize = 160;
/// The node type codes the probe takes for a header.
const PROBE_TYPES: std::ops::RangeInclusive<u8> = 0x01..=0x3f;
/// The node type codes of the topology tables.
const TABLE_TYPES: [u8; 6] = [0x0e, 0x0f, 0x10, 0x11, 0x12, 0x1d];
/// The node type codes of the compact curve and surface carriers that the probe reaches.
const CARRIER_TYPES: std::ops::RangeInclusive<u8> = 0x1e..=0x3f;
/// Probe cost: one work unit per this many stream bytes.
const BYTES_PER_WORK_UNIT: usize = 64;
/// The null real that marks a node without a tolerance of its own.
const NULL_REAL: [u8; 8] = [0xc2, 0xbc, 0x92, 0x8f, 0x99, 0x6e, 0x00, 0x00];

/// Why an endpoint of a dropped edge did not resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropCause {
    NullVertex,
    AbsentVertex,
    AbsentPoint,
}

/// One edge that the topology build did not emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Finding {
    /// An endpoint did not resolve.
    Unresolved {
        edge: u16,
        cause: DropCause,
        /// 0 for the start of the edge, 1 for its end.
        end: u8,
        vertex: u16,
    },
    /// No coedge fixes the direction of the edge.
    NoCanonical { edge: u16, forward: usize },
    /// The canonical coedge is in no ring of a kept face.
    OffRing { edge: u16, canonical: u16 },
}

/// The records of a dropped-edge diagnostic and the counts they summarize.
#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    pub(crate) findings: Vec<Finding>,
    /// Findings that did not fit the limit.
    pub(crate) overflow: usize,
    /// `(bridge, loop reference)` of faces whose first loop reference names no
    /// loop record.
    pub(crate) empty_faces: Vec<(u16, u16)>,
    /// Such faces that did not fit the limit.
    pub(crate) empty_faces_over: usize,
    /// `(bridge, loop, coedges walked, reference where the walk stopped, cause)`
    /// of faces whose first ring did not close.
    pub(crate) ring_stops: Vec<(u16, u16, usize, u16, u8)>,
    /// Curve and surface references of edges and faces whose carrier the decode
    /// does not type, for the probe of the record type that sits at the index.
    pub(crate) unknown_curves: Vec<u16>,
    pub(crate) unknown_surfaces: Vec<u16>,
    /// JSON lines, filled by [`render`].
    pub(crate) lines: Vec<String>,
}

/// The most untyped carrier references that one decode lists.
const UNKNOWN_CARRIER_LIMIT: usize = 20_000;

/// The most faces without a loop record that one decode lists.
const EMPTY_FACE_LIMIT: usize = 2_000;

impl Diagnostics {
    pub(crate) fn record_ring_stop(
        &mut self,
        ctx: &DecodeContext<'_>,
        bridge: u16,
        loop_attr: u16,
        walked: usize,
        at: u16,
        cause: u8,
    ) -> Result<(), CodecError> {
        if self.ring_stops.len() >= EMPTY_FACE_LIMIT {
            self.empty_faces_over += 1;
            return Ok(());
        }
        ctx.push_vec(
            &mut self.ring_stops,
            (bridge, loop_attr, walked, at, cause),
            "collect Parasolid ring diagnostics",
        )
    }

    pub(crate) fn record_unknown_carrier(&mut self, curve: bool, index: u16) {
        let list = if curve {
            &mut self.unknown_curves
        } else {
            &mut self.unknown_surfaces
        };
        if list.len() < UNKNOWN_CARRIER_LIMIT && !list.contains(&index) {
            list.push(index);
        }
    }

    pub(crate) fn record_empty_face(
        &mut self,
        ctx: &DecodeContext<'_>,
        bridge: u16,
        loop_ref: u16,
    ) -> Result<(), CodecError> {
        if self.empty_faces.len() >= EMPTY_FACE_LIMIT {
            self.empty_faces_over += 1;
            return Ok(());
        }
        ctx.push_vec(
            &mut self.empty_faces,
            (bridge, loop_ref),
            "collect Parasolid face diagnostics",
        )
    }

    pub(crate) fn record(
        &mut self,
        ctx: &DecodeContext<'_>,
        finding: Finding,
    ) -> Result<(), CodecError> {
        if self.findings.len() >= FINDING_LIMIT {
            self.overflow += 1;
            return Ok(());
        }
        ctx.push_vec(
            &mut self.findings,
            finding,
            "collect Parasolid edge diagnostics",
        )
    }
}

/// One node header that carries an index.
#[derive(Debug, Clone, Copy)]
struct Hit {
    node_type: u8,
    delta: bool,
    /// Position of the header in its stream, and which stream.
    at: usize,
    stream: usize,
    /// Headers of this type in this stream with the same index.
    count: usize,
    /// Bytes from the index to the null real, when one follows closely.
    null_real_at: Option<usize>,
    /// Bytes from the index to the next header of a table type, and its type.
    next_header: Option<(usize, u8)>,
}

/// Every position in the streams where a node header carries one of `targets`.
fn probe(
    ctx: &DecodeContext<'_>,
    streams: &[(&[u8], bool)],
    targets: &HashSet<u16>,
) -> Result<HashMap<u16, Vec<Hit>>, CodecError> {
    let mut found: HashMap<u16, Vec<Hit>> = HashMap::new();
    for (stream, (buf, delta)) in streams.iter().enumerate() {
        ctx.charge_work(
            u64_from_index(buf.len() / BYTES_PER_WORK_UNIT + 1),
            "probe Parasolid node headers",
        )?;
        let Some(last) = buf.len().checked_sub(4) else {
            continue;
        };
        for at in 0..last {
            if buf[at] != 0 || !PROBE_TYPES.contains(&buf[at + 1]) {
                continue;
            }
            let index_at = if buf[at + 2] == 0xff { at + 3 } else { at + 2 };
            let Some(index) = View::u16_be_at(buf, index_at) else {
                continue;
            };
            if !targets.contains(&index) {
                continue;
            }
            let slot = found.entry(index).or_default();
            if let Some(same) = slot
                .iter_mut()
                .find(|hit| hit.node_type == buf[at + 1] && hit.delta == *delta)
            {
                same.count += 1;
                continue;
            }
            if slot.len() >= HITS_PER_INDEX {
                continue;
            }
            let tail = &buf[index_at + 2..];
            let window = &tail[..tail.len().min(MAGIC_WINDOW)];
            let null_real_at = window
                .windows(NULL_REAL.len())
                .position(|bytes| bytes == NULL_REAL)
                .map(|distance| distance + 2);
            let next_header = (4..tail.len().min(NEXT_HEADER_WINDOW)).find_map(|distance| {
                (tail[distance - 1] == 0 && TABLE_TYPES.contains(&tail[distance]))
                    .then_some((distance + 1, tail[distance]))
            });
            slot.push(Hit {
                node_type: buf[at + 1],
                delta: *delta,
                at,
                stream,
                count: 1,
                null_real_at,
                next_header,
            });
        }
    }
    Ok(found)
}

/// The table types that hold a record with this index.
fn known_as(tables: &Tables, index: u16) -> Vec<u8> {
    TABLE_TYPES
        .into_iter()
        .filter(|code| table_has(tables, *code, index))
        .collect()
}

/// Whether the table of a node type holds a record with this index.
fn table_has(tables: &Tables, code: u8, index: u16) -> bool {
    match code {
        0x0e => tables.bridges().contains_key(&index),
        0x0f => tables.loops().contains_key(&index),
        0x10 => tables.edge_uses().contains_key(&index),
        0x11 => tables.coedges().contains_key(&index),
        0x12 => tables.vertex_uses().contains_key(&index),
        0x1d => tables.points().contains_key(&index),
        _ => false,
    }
}

fn list<T: std::fmt::Display>(items: impl IntoIterator<Item = T>) -> String {
    let parts: Vec<String> = items.into_iter().map(|item| item.to_string()).collect();
    format!("[{}]", parts.join(","))
}

fn option(value: Option<impl std::fmt::Display>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn hits_json(hits: Option<&Vec<Hit>>) -> String {
    let parts: Vec<String> = hits
        .into_iter()
        .flatten()
        .map(|hit| {
            format!(
                "{{\"t\":{},\"s\":\"{}\",\"n\":{},\"magic\":{},\"next\":{},\"next_t\":{}}}",
                hit.node_type,
                if hit.delta { "d" } else { "p" },
                hit.count,
                option(hit.null_real_at),
                option(hit.next_header.map(|(distance, _)| distance)),
                option(hit.next_header.map(|(_, kind)| kind)),
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

/// The indices a finding asks the probe about.
fn targets_of(
    ctx: &DecodeContext<'_>,
    tables: &Tables,
    finding: &Finding,
    out: &mut Vec<u16>,
) -> Result<(), CodecError> {
    match *finding {
        Finding::Unresolved {
            edge,
            cause,
            vertex,
            ..
        } => {
            out.push(edge);
            if vertex != 0 {
                out.push(vertex);
            }
            if cause == DropCause::AbsentPoint {
                if let Some(record) = tables.vertex_uses().get(&vertex) {
                    out.push(record.refs[4]);
                }
            }
        }
        Finding::NoCanonical { edge, .. } => {
            out.push(edge);
            for coedge in coedges_of(ctx, tables, edge)? {
                if let Some(loop_) = tables.loops().get(&coedge.refs[1]) {
                    out.push(loop_.refs[2]);
                    if let Some(bridge) = tables.bridges().get(&loop_.refs[2]) {
                        if let Some(shell) = bridge.owner {
                            out.push(shell);
                        }
                    }
                }
            }
        }
        Finding::OffRing { edge, canonical } => {
            out.push(edge);
            out.push(canonical);
        }
    }
    Ok(())
}

fn coedges_of<'a>(
    ctx: &DecodeContext<'_>,
    tables: &'a Tables,
    edge: u16,
) -> Result<Vec<&'a super::topology::Coedge>, CodecError> {
    let mut found: Vec<&super::topology::Coedge> = tables
        .coedges()
        .values()
        .filter(|coedge| coedge.refs[6] == edge)
        .collect();
    ctx.stable_sort_by(
        &mut found,
        |left, right| left.attr.cmp(&right.attr),
        |_| 0,
        "order Parasolid coedge diagnostics",
    )?;
    found.truncate(COEDGES_PER_FINDING);
    Ok(found)
}

/// Render the findings as JSON lines.
pub(crate) fn render(
    ctx: &DecodeContext<'_>,
    tables: &Tables,
    streams: &[(&[u8], bool)],
    diagnostics: &mut Diagnostics,
) -> Result<(), CodecError> {
    let mut wanted = Vec::new();
    for finding in &diagnostics.findings {
        targets_of(ctx, tables, finding, &mut wanted)?;
    }
    let refs = collect_refs(ctx, tables, diagnostics)?;
    for item in &refs {
        wanted.extend([item.value, item.value & MASK_15, item.value & MASK_8]);
    }
    for (_, loop_attr, _, at, _) in &diagnostics.ring_stops {
        wanted.push(*at);
        wanted.extend(tables.loops().get(loop_attr).map(|record| record.refs[1]));
        wanted.extend(tables.coedges().get(at).map(|coedge| coedge.refs[1]));
    }
    wanted.extend(diagnostics.unknown_curves.iter().copied());
    wanted.extend(diagnostics.unknown_surfaces.iter().copied());
    let targets: HashSet<u16> = wanted.into_iter().filter(|index| *index != 0).collect();
    let hits = probe(ctx, streams, &targets)?;
    let at = |index: u16| hits_json(hits.get(&index));
    let at_hits = |index: u16| hits_json(hits.get(&index));
    let mut lines = Vec::new();
    for finding in &diagnostics.findings {
        ctx.charge_work(1, "render Parasolid edge diagnostics")?;
        let line = match *finding {
            Finding::Unresolved {
                edge,
                cause,
                end,
                vertex,
            } => {
                let word = match cause {
                    DropCause::NullVertex => "null_vertex",
                    DropCause::AbsentVertex => "absent_vertex",
                    DropCause::AbsentPoint => "absent_point",
                };
                let record = tables.vertex_uses().get(&vertex);
                let point = record.map(|record| record.refs[4]);
                format!(
                    "{{\"kind\":\"drop\",\"cause\":\"{word}\",\"edge\":{edge},\"edge_known\":{},\
                     \"edge_hits\":{},\"end\":\"{}\",\"vertex\":{vertex},\"vertex_known\":{},\
                     \"vertex_hits\":{},\"vertex_refs_set\":{},\"vertex_tolerant\":{},\
                     \"point\":{},\"point_known\":{},\"point_hits\":{}}}",
                    list(known_as(tables, edge)),
                    at(edge),
                    if end == 0 { "start" } else { "end" },
                    list(known_as(tables, vertex)),
                    at(vertex),
                    option(record.map(|record| {
                        record.refs.iter().filter(|reference| **reference != 0).count()
                    })),
                    option(record.map(|record| record.tolerance_m.is_some())),
                    option(point),
                    list(point.map_or_else(Vec::new, |point| known_as(tables, point))),
                    point.map_or_else(|| "[]".to_owned(), at),
                )
            }
            Finding::NoCanonical { edge, forward } => {
                let canonical_ref = tables
                    .edge_uses()
                    .get(&edge)
                    .and_then(|record| record.references.canonical());
                let coedges: Vec<String> = coedges_of(ctx, tables, edge)?
                    .into_iter()
                    .map(|coedge| {
                        let loop_ = tables.loops().get(&coedge.refs[1]);
                        let owner = loop_.map(|loop_| loop_.refs[2]);
                        let shell = owner
                            .and_then(|owner| tables.bridges().get(&owner))
                            .and_then(|bridge| bridge.owner);
                        format!(
                            "{{\"known\":{},\"sense\":\"{}\",\"loop_known\":{},\
                             \"twin_known\":{},\"owner\":{},\"owner_known\":{},\"owner_hits\":{},\
                             \"face_owner\":{},\"face_owner_known\":{},\"face_owner_hits\":{}}}",
                            list(known_as(tables, coedge.attr)),
                            if coedge.sense == Sense::Forward {
                                "+"
                            } else {
                                "-"
                            },
                            list(known_as(tables, coedge.refs[1])),
                            list(known_as(tables, coedge.refs[5])),
                            option(owner),
                            list(owner.map_or_else(Vec::new, |owner| known_as(tables, owner))),
                            owner.map_or_else(|| "[]".to_owned(), at),
                            option(shell),
                            list(shell.map_or_else(Vec::new, |shell| known_as(tables, shell))),
                            shell.map_or_else(|| "[]".to_owned(), at),
                        )
                    })
                    .collect();
                format!(
                    "{{\"kind\":\"no_canonical\",\"edge\":{edge},\"forward\":{forward},\
                     \"canonical_ref\":{},\"canonical_ref_known\":{},\
                     \"edge_known\":{},\"edge_hits\":{},\"coedges\":[{}]}}",
                    option(canonical_ref),
                    list(canonical_ref.map_or_else(Vec::new, |named| known_as(tables, named))),
                    list(known_as(tables, edge)),
                    at(edge),
                    coedges.join(","),
                )
            }
            Finding::OffRing { edge, canonical } => format!(
                "{{\"kind\":\"off_ring\",\"edge\":{edge},\"edge_known\":{},\"canonical\":{canonical},\
                 \"canonical_known\":{},\"canonical_hits\":{}}}",
                list(known_as(tables, edge)),
                list(known_as(tables, canonical)),
                at(canonical),
            ),
        };
        lines.push(line);
    }
    let mut keys = HashMap::new();
    for item in &refs {
        if let std::collections::hash_map::Entry::Vacant(slot) = keys.entry(item.expected) {
            slot.insert(sorted_keys(ctx, tables, item.expected)?);
        }
    }
    for item in &refs {
        ctx.charge_work(1, "render Parasolid reference diagnostics")?;
        lines.push(ref_line(tables, streams, &hits, &keys, item));
    }
    for (bridge, loop_attr, walked, at, cause) in &diagnostics.ring_stops {
        ctx.charge_work(1, "render Parasolid ring diagnostics")?;
        let first = tables.loops().get(loop_attr).map(|record| record.refs[1]);
        let ce_loop = tables.coedges().get(at).map(|coedge| coedge.refs[1]);
        let same_owner = ce_loop
            .and_then(|value| tables.loops().get(&value))
            .zip(tables.loops().get(loop_attr))
            .map(|(other, own)| other.refs[2] == own.refs[2]);
        let word = match cause {
            0 => "coedge_absent",
            1 => "foreign_loop",
            2 => "cycle",
            _ => "null_next",
        };
        lines.push(format!(
            "{{\"kind\":\"ring\",\"bridge\":{bridge},\"loop\":{loop_attr},\
             \"stop\":\"{word}\",\"walked\":{walked},\"at\":{at},\"at_known\":{},\
             \"at_admit\":\"{}\",\"at_why\":[{}],\"at_hits\":{},\"first\":{},\
             \"first_known\":{},\"first_admit\":\"{}\",\"first_why\":[{}],\
             \"first_hits\":{},\"ce_loop\":{},\"ce_loop_known\":{},\
             \"ce_loop_same_owner\":{}}}",
            list(known_as(tables, *at)),
            tables.admission_word(0x11, *at),
            why_json(tables, streams, &hits, *at, 0x11),
            at_hits(*at),
            option(first),
            list(first.map_or_else(Vec::new, |first| known_as(tables, first))),
            first.map_or("none", |first| tables.admission_word(0x11, first)),
            first.map_or_else(String::new, |first| why_json(
                tables, streams, &hits, first, 0x11
            )),
            first.map_or_else(|| "[]".to_owned(), at_hits),
            option(ce_loop),
            list(ce_loop.map_or_else(Vec::new, |value| known_as(tables, value))),
            option(same_owner),
        ));
    }
    pair_lines(ctx, tables, &mut lines)?;
    census(ctx, tables, streams, &mut lines)?;
    // The record type that sits at the index of each untyped carrier: the first header of a
    // carrier type in the partition stream, else in the deltas stream, counted by type code. The
    // references that only an unrelated header names count as `other_type`, and the references
    // that no header names as `no_header`.
    for (what, list) in [
        ("curve", &diagnostics.unknown_curves),
        ("surface", &diagnostics.unknown_surfaces),
    ] {
        let mut by_type: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();
        let (mut none, mut other) = (0_usize, 0_usize);
        for index in list {
            let Some(found) = hits.get(index) else {
                none += 1;
                continue;
            };
            let carrier = |hit: &&Hit| CARRIER_TYPES.contains(&hit.node_type);
            match found
                .iter()
                .filter(carrier)
                .find(|hit| !hit.delta)
                .or_else(|| found.iter().find(carrier))
            {
                Some(hit) => *by_type.entry(hit.node_type).or_insert(0) += 1,
                None => other += 1,
            }
        }
        let types: Vec<String> = by_type
            .iter()
            .map(|(code, count)| format!(",\"type_{code}\":{count}"))
            .collect();
        lines.push(format!(
            "{{\"kind\":\"summary3\",\"what\":\"{what}\",\"refs\":{},\"no_header\":{none},\"other_type\":{other}{}}}",
            list.len(),
            types.concat()
        ));
    }
    diagnostics.lines = lines;
    Ok(())
}

/// Mask of the low fifteen bits, the first cell of a wide node pointer.
const MASK_15: u16 = 0x7fff;
/// Mask of the low byte.
const MASK_8: u16 = 0x00ff;
/// The most reference lines one decode lists.
const REFERENCE_LIMIT: usize = 20_000;
/// Offsets from a reference that the numbering check tries.
const NEIGHBOURS: std::ops::RangeInclusive<i32> = -8..=8;
/// The most distinct key steps a census line lists.
const STEPS_LISTED: usize = 4;
/// The node type that the owner of a face-use is read against.
const BRIDGE_TYPE: u8 = 0x0e;

/// A reference that names no record of the type it should.
struct RefItem {
    what: &'static str,
    value: u16,
    expected: u8,
}

fn collect_refs(
    ctx: &DecodeContext<'_>,
    tables: &Tables,
    diagnostics: &Diagnostics,
) -> Result<Vec<RefItem>, CodecError> {
    let mut refs: Vec<RefItem> = Vec::new();
    let mut push = |ctx: &DecodeContext<'_>, item: RefItem| -> Result<(), CodecError> {
        if item.value > 1 && refs.len() < REFERENCE_LIMIT {
            ctx.push_vec(&mut refs, item, "collect Parasolid reference diagnostics")?;
        }
        Ok(())
    };
    for finding in &diagnostics.findings {
        match *finding {
            Finding::Unresolved { cause, vertex, .. } => match cause {
                DropCause::AbsentVertex => push(
                    ctx,
                    RefItem {
                        what: "vertex",
                        value: vertex,
                        expected: 0x12,
                    },
                )?,
                DropCause::AbsentPoint => {
                    if let Some(record) = tables.vertex_uses().get(&vertex) {
                        push(
                            ctx,
                            RefItem {
                                what: "point",
                                value: record.refs[4],
                                expected: 0x1d,
                            },
                        )?;
                    }
                }
                DropCause::NullVertex => {}
            },
            Finding::NoCanonical { edge, .. } => {
                if let Some(named) = tables
                    .edge_uses()
                    .get(&edge)
                    .and_then(|record| record.references.canonical())
                {
                    if !tables.coedges().contains_key(&named) {
                        push(
                            ctx,
                            RefItem {
                                what: "canonical",
                                value: named,
                                expected: 0x11,
                            },
                        )?;
                    }
                }
                for coedge in coedges_of(ctx, tables, edge)? {
                    let shell = tables
                        .loops()
                        .get(&coedge.refs[1])
                        .and_then(|loop_| tables.bridges().get(&loop_.refs[2]))
                        .and_then(|bridge| bridge.owner);
                    if let Some(shell) = shell {
                        push(
                            ctx,
                            RefItem {
                                what: "face_owner",
                                value: shell,
                                expected: BRIDGE_TYPE,
                            },
                        )?;
                    }
                }
            }
            Finding::OffRing { .. } => {}
        }
    }
    for (_, loop_ref) in &diagnostics.empty_faces {
        push(
            ctx,
            RefItem {
                what: "face_loop",
                value: *loop_ref,
                expected: 0x0f,
            },
        )?;
    }
    Ok(refs)
}

/// The keys of the table of a node type, in ascending order.
fn sorted_keys(ctx: &DecodeContext<'_>, tables: &Tables, code: u8) -> Result<Vec<u16>, CodecError> {
    let mut keys: Vec<u16> = match code {
        0x0e => tables.bridges().keys().copied().collect(),
        0x0f => tables.loops().keys().copied().collect(),
        0x10 => tables.edge_uses().keys().copied().collect(),
        0x11 => tables.coedges().keys().copied().collect(),
        0x12 => tables.vertex_uses().keys().copied().collect(),
        0x1d => tables.points().keys().copied().collect(),
        _ => Vec::new(),
    };
    ctx.stable_sort_by(&mut keys, Ord::cmp, |_| 0, "order Parasolid table keys")?;
    Ok(keys)
}

/// `[key, value - key]` of the nearest key below and above a value.
fn neighbours(keys: &[u16], value: u16) -> (String, String) {
    let at = keys.partition_point(|key| *key < value);
    let pair = |key: Option<&u16>| {
        key.map_or_else(
            || "null".to_owned(),
            |key| format!("[{key},{}]", i32::from(value) - i32::from(*key)),
        )
    };
    let below = at.checked_sub(1).and_then(|index| keys.get(index));
    let above = keys
        .get(at..)
        .and_then(|rest| rest.iter().find(|key| **key > value));
    (pair(below), pair(above))
}

/// The rejection objects of the headers of the expected type at an index.
fn why_json(
    tables: &Tables,
    streams: &[(&[u8], bool)],
    hits: &HashMap<u16, Vec<Hit>>,
    value: u16,
    expected: u8,
) -> String {
    let why: Vec<String> = hits
        .get(&value)
        .into_iter()
        .flatten()
        .filter(|hit| hit.node_type == expected)
        .filter_map(|hit| {
            let (buf, _) = streams.get(hit.stream)?;
            Some(format!(
                "{{\"s\":\"{}\",\"n\":{},\"r\":{}}}",
                if hit.delta { "d" } else { "p" },
                hit.count,
                super::topology::rejection_json(buf, hit.at, expected, tables),
            ))
        })
        .collect();
    why.join(",")
}

fn ref_line(
    tables: &Tables,
    streams: &[(&[u8], bool)],
    hits: &HashMap<u16, Vec<Hit>>,
    keys: &HashMap<u8, Vec<u16>>,
    item: &RefItem,
) -> String {
    let RefItem {
        what,
        value,
        expected,
    } = *item;
    let sorted = keys.get(&expected).map_or(&[][..], Vec::as_slice);
    let (below, above) = neighbours(sorted, value);
    let header = |index: u16, delta: bool| {
        hits.get(&index).is_some_and(|found| {
            found
                .iter()
                .any(|hit| hit.node_type == expected && hit.delta == delta)
        })
    };
    let variant = |index: u16| {
        format!(
            "{{\"v\":{index},\"key\":{},\"hdr_p\":{},\"hdr_d\":{}}}",
            table_has(tables, expected, index),
            header(index, false),
            header(index, true),
        )
    };
    let plus_k: Vec<i32> = NEIGHBOURS
        .filter(|step| *step != 0)
        .filter(|step| {
            u16::try_from(i32::from(value) + step)
                .is_ok_and(|index| table_has(tables, expected, index))
        })
        .collect();
    let why = why_json(tables, streams, hits, value, expected);
    format!(
        "{{\"kind\":\"ref\",\"what\":\"{what}\",\"value\":{value},\"expected\":{expected},\
         \"known\":{},\"below\":{below},\"above\":{above},\"m7fff\":{},\"low8\":{},\
         \"plus_k\":{},\"hdr_p\":{},\"hdr_d\":{},\"admit\":\"{}\",\"why\":[{}],\"hits\":{}}}",
        list(known_as(tables, value)),
        variant(value & MASK_15),
        variant(value & MASK_8),
        list(plus_k),
        header(value, false),
        header(value, true),
        tables.admission_word(expected, value),
        why,
        hits_json(hits.get(&value)),
    )
}

/// `(offset, key, sequence)` of every record of a node type, by offset.
fn offsets_of(
    ctx: &DecodeContext<'_>,
    tables: &Tables,
    code: u8,
) -> Result<Vec<(usize, u16, Option<u32>)>, CodecError> {
    let mut found: Vec<(usize, u16, Option<u32>)> = match code {
        0x0e => tables
            .bridges()
            .iter()
            .map(|(key, record)| (record.offset, *key, Some(record.sequence)))
            .collect(),
        0x0f => tables
            .loops()
            .iter()
            .map(|(key, record)| (record.offset, *key, None))
            .collect(),
        0x10 => tables
            .edge_uses()
            .iter()
            .map(|(key, record)| (record.offset, *key, Some(record.sequence)))
            .collect(),
        0x11 => tables
            .coedges()
            .iter()
            .map(|(key, record)| (record.offset, *key, None))
            .collect(),
        0x12 => tables
            .vertex_uses()
            .iter()
            .map(|(key, record)| (record.offset, *key, Some(record.sequence)))
            .collect(),
        0x1d => tables
            .points()
            .iter()
            .map(|(key, record)| (record.offset, *key, None))
            .collect(),
        _ => Vec::new(),
    };
    ctx.stable_sort_by(
        &mut found,
        |left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)),
        |_| 0,
        "order Parasolid table records",
    )?;
    Ok(found)
}

/// Per node type and stream kind: headers, headers without a table record, and
/// the largest index.
type RawCounts = HashMap<(u8, bool), (usize, usize, u16)>;

/// Raw header counts: per node type and stream kind, the headers, those whose
/// index no table of the type holds, and the largest index.
fn raw_census(
    ctx: &DecodeContext<'_>,
    tables: &Tables,
    streams: &[(&[u8], bool)],
) -> Result<RawCounts, CodecError> {
    let mut census = RawCounts::new();
    for (buf, delta) in streams {
        ctx.charge_work(
            u64_from_index(buf.len() / BYTES_PER_WORK_UNIT + 1),
            "count Parasolid node headers",
        )?;
        let Some(last) = buf.len().checked_sub(4) else {
            continue;
        };
        for at in 0..last {
            if buf[at] != 0 || !TABLE_TYPES.contains(&buf[at + 1]) {
                continue;
            }
            let index_at = if buf[at + 2] == 0xff { at + 3 } else { at + 2 };
            let Some(index) = View::u16_be_at(buf, index_at).filter(|index| *index > 1) else {
                continue;
            };
            let entry = census.entry((buf[at + 1], *delta)).or_default();
            entry.0 += 1;
            if !table_has(tables, buf[at + 1], index) {
                entry.1 += 1;
            }
            entry.2 = entry.2.max(index);
        }
    }
    Ok(census)
}

/// The most pair lines one decode lists.
const PAIR_LINE_LIMIT: usize = 200;

/// Counts of coedge pairs that name each other as twin, by how many members
/// the second pass admitted, and the pairs whose edges differ or whose senses
/// are equal.
fn pair_lines(
    ctx: &DecodeContext<'_>,
    tables: &Tables,
    lines: &mut Vec<String>,
) -> Result<(), CodecError> {
    let mut keys: Vec<u16> = tables.coedges().keys().copied().collect();
    ctx.stable_sort_by(&mut keys, Ord::cmp, |_| 0, "order Parasolid coedge pairs")?;
    let mut total = [0_usize; 3];
    let mut edge_mismatch = [0_usize; 3];
    let mut same_sense = [0_usize; 3];
    let mut listed = 0;
    for key in keys {
        ctx.charge_work(1, "pair Parasolid coedges")?;
        let Some(coedge) = tables.coedges().get(&key) else {
            continue;
        };
        let twin = coedge.refs[5];
        let Some(other) = tables
            .coedges()
            .get(&twin)
            .filter(|other| other.refs[5] == key && twin > key)
        else {
            continue;
        };
        let admitted = usize::from(tables.is_admitted(0x11, key))
            + usize::from(tables.is_admitted(0x11, twin));
        total[admitted] += 1;
        let edges_differ = coedge.refs[6] != other.refs[6];
        let equal_sense = coedge.sense == other.sense;
        edge_mismatch[admitted] += usize::from(edges_differ);
        same_sense[admitted] += usize::from(equal_sense);
        if (edges_differ || equal_sense) && admitted > 0 && listed < PAIR_LINE_LIMIT {
            listed += 1;
            lines.push(format!(
                "{{\"kind\":\"pair\",\"a\":{key},\"b\":{twin},\"adm\":{admitted},\
                 \"edge_equal\":{},\"sense_a\":\"{}\",\"sense_b\":\"{}\",\
                 \"loop_equal\":{},\"a_edge_known\":{},\"b_edge_known\":{}}}",
                !edges_differ,
                sense_word(coedge.sense),
                sense_word(other.sense),
                coedge.refs[1] == other.refs[1],
                list(known_as(tables, coedge.refs[6])),
                list(known_as(tables, other.refs[6])),
            ));
        }
    }
    lines.push(format!(
        "{{\"kind\":\"pairs\",\"total\":{},\"edge_mismatch\":{},\"same_sense\":{}}}",
        list(total),
        list(edge_mismatch),
        list(same_sense),
    ));
    Ok(())
}

fn sense_word(sense: Sense) -> &'static str {
    if sense == Sense::Forward {
        "+"
    } else {
        "-"
    }
}

/// One line per table type on how the keys of its records relate to the
/// records themselves.
fn census(
    ctx: &DecodeContext<'_>,
    tables: &Tables,
    streams: &[(&[u8], bool)],
    lines: &mut Vec<String>,
) -> Result<(), CodecError> {
    let raw = raw_census(ctx, tables, streams)?;
    for code in TABLE_TYPES {
        let records = offsets_of(ctx, tables, code)?;
        let (Some(min), Some(max)) = (
            records.iter().map(|record| record.1).min(),
            records.iter().map(|record| record.1).max(),
        ) else {
            continue;
        };
        let over = records.iter().filter(|record| record.1 > MASK_15).count();
        let equal_sequence = if records.iter().any(|record| record.2.is_some()) {
            records
                .iter()
                .filter(|record| record.2 == Some(u32::from(record.1)))
                .count()
                .to_string()
        } else {
            "null".to_owned()
        };
        let mut steps: HashMap<i32, usize> = HashMap::new();
        for pair in records.windows(2) {
            *steps
                .entry(i32::from(pair[1].1) - i32::from(pair[0].1))
                .or_insert(0) += 1;
        }
        let mut ranked: Vec<(i32, usize)> = steps.into_iter().collect();
        ctx.stable_sort_by(
            &mut ranked,
            |left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)),
            |_| 0,
            "order Parasolid key steps",
        )?;
        ranked.truncate(STEPS_LISTED);
        let steps = format!(
            "[{}]",
            ranked
                .iter()
                .map(|(step, count)| format!("[{step},{count}]"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let side = |delta: bool| {
            let (total, unkeyed, largest) = raw.get(&(code, delta)).copied().unwrap_or_default();
            format!("{total},{unkeyed},{largest}")
        };
        lines.push(format!(
            "{{\"kind\":\"keys\",\"t\":{code},\"n\":{},\"min\":{min},\"max\":{max},\
             \"over_7fff\":{over},\"eq_seq\":{equal_sequence},\"steps\":{steps},\
             \"raw_p\":[{}],\"raw_d\":[{}]}}",
            records.len(),
            side(false),
            side(true),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_names_the_type_and_stream_of_a_header_without_values() {
        // A node header of type 0x13 with index 77, then a null real.
        let mut partition = vec![0_u8, 0x13, 0, 77, 0, 0, 0, 7];
        partition.extend(NULL_REAL);
        let delta = vec![0_u8, 0x12, 0, 77, 0, 0, 0, 0];
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("context");
        let targets = HashSet::from([77_u16, 78]);
        let found = probe(&ctx, &[(&partition, false), (&delta, true)], &targets).unwrap();
        let hits = &found[&77];
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].node_type, hits[0].delta), (0x13, false));
        assert_eq!(hits[0].null_real_at, Some(6));
        assert_eq!((hits[1].node_type, hits[1].delta), (0x12, true));
        assert!(!found.contains_key(&78));
    }

    #[test]
    fn untyped_carriers_are_counted_by_the_record_type_at_their_index() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("context");
        // A node header of type 0x13 with index 77, and nothing at index 99.
        let stream = vec![0_u8, 0x13, 0, 77, 0, 0, 0, 7];
        let mut diagnostics = Diagnostics::default();
        diagnostics.record_unknown_carrier(true, 77);
        diagnostics.record_unknown_carrier(true, 77);
        diagnostics.record_unknown_carrier(true, 99);
        diagnostics.record_unknown_carrier(false, 77);
        render(
            &ctx,
            &Tables::default(),
            &[(&stream, false)],
            &mut diagnostics,
        )
        .unwrap();
        assert!(diagnostics.lines.contains(&String::from(
            "{\"kind\":\"summary3\",\"what\":\"curve\",\"refs\":2,\"no_header\":1,\"other_type\":1}"
        )));
        assert!(diagnostics.lines.contains(&String::from(
            "{\"kind\":\"summary3\",\"what\":\"surface\",\"refs\":1,\"no_header\":0,\"other_type\":1}"
        )));
    }

    #[test]
    fn a_finding_renders_as_one_json_line_of_numbers_and_words() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("context");
        let stream = vec![0_u8, 0x10, 0, 9, 0, 0, 0, 1];
        let mut diagnostics = Diagnostics::default();
        diagnostics
            .record(
                &ctx,
                Finding::Unresolved {
                    edge: 9,
                    cause: DropCause::AbsentVertex,
                    end: 1,
                    vertex: 300,
                },
            )
            .unwrap();
        render(
            &ctx,
            &Tables::default(),
            &[(&stream, false)],
            &mut diagnostics,
        )
        .unwrap();
        let line = &diagnostics.lines[0];
        assert!(line.starts_with("{\"kind\":\"drop\",\"cause\":\"absent_vertex\",\"edge\":9"));
        assert!(line.contains("\"end\":\"end\",\"vertex\":300"));
        assert!(line.contains("\"edge_hits\":[{\"t\":16,\"s\":\"p\""));
        assert!(line.contains("\"vertex_hits\":[]"));
    }

    #[test]
    fn the_nearest_keys_of_an_unnamed_reference_are_listed_with_their_distance() {
        let keys = [10_u16, 20, 30];
        assert_eq!(
            neighbours(&keys, 25),
            ("[20,5]".to_owned(), "[30,-5]".to_owned())
        );
        assert_eq!(
            neighbours(&keys, 5),
            ("null".to_owned(), "[10,-5]".to_owned())
        );
        assert_eq!(
            neighbours(&keys, 40),
            ("[30,10]".to_owned(), "null".to_owned())
        );
    }

    #[test]
    fn a_reference_line_states_the_numbering_checks_without_values() {
        let tables = Tables::default();
        let hits = HashMap::from([(
            0x8005_u16,
            vec![Hit {
                node_type: 0x0f,
                delta: true,
                at: 0,
                stream: 0,
                count: 1,
                null_real_at: None,
                next_header: None,
            }],
        )]);
        let keys = HashMap::from([(0x0f_u8, vec![3_u16, 9])]);
        let item = RefItem {
            what: "face_loop",
            value: 0x8005,
            expected: 0x0f,
        };
        let line = ref_line(&tables, &[], &hits, &keys, &item);
        assert!(line.starts_with("{\"kind\":\"ref\",\"what\":\"face_loop\",\"value\":32773,"));
        assert!(line.contains("\"below\":[9,32764],\"above\":null"));
        assert!(line.contains("\"hdr_p\":false,\"hdr_d\":true"));
        assert!(line.contains("\"m7fff\":{\"v\":5,"));
    }
}
