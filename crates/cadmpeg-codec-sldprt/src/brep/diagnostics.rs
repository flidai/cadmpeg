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
/// The most stream positions the probe keeps for one index.
const HITS_PER_INDEX: usize = 4;
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
    /// JSON lines, filled by [`render`].
    pub(crate) lines: Vec<String>,
}

impl Diagnostics {
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
    for (buf, delta) in streams {
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
                null_real_at,
                next_header,
            });
        }
    }
    Ok(found)
}

/// The table types that hold a record with this index.
fn known_as(tables: &Tables, index: u16) -> Vec<u8> {
    let mut codes = Vec::new();
    for (code, present) in [
        (0x0e, tables.bridges().contains_key(&index)),
        (0x0f, tables.loops().contains_key(&index)),
        (0x10, tables.edge_uses().contains_key(&index)),
        (0x11, tables.coedges().contains_key(&index)),
        (0x12, tables.vertex_uses().contains_key(&index)),
        (0x1d, tables.points().contains_key(&index)),
    ] {
        if present {
            codes.push(code);
        }
    }
    codes
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
                "{{\"t\":{},\"s\":\"{}\",\"magic\":{},\"next\":{},\"next_t\":{}}}",
                hit.node_type,
                if hit.delta { "d" } else { "p" },
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
    let targets: HashSet<u16> = wanted.into_iter().filter(|index| *index != 0).collect();
    let hits = probe(ctx, streams, &targets)?;
    let at = |index: u16| hits_json(hits.get(&index));
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
                     \"edge_known\":{},\"edge_hits\":{},\"coedges\":[{}]}}",
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
    diagnostics.lines = lines;
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
}
