// SPDX-License-Identifier: Apache-2.0
//! Parasolid B-rep decoding for a foreign host container.
//!
//! A host that stores a Parasolid partition stream, with an optional deltas
//! stream, outside the `SolidWorks` framing locates the streams itself and calls
//! [`decode_hosted_brep`]. The call runs the same partition and deltas merge and
//! the same graph builder as an `.sldprt` body site and writes the neutral
//! B-rep arenas into the caller's document. The host owns the document
//! metadata, the dialect layers, and the retention of the stream bytes.
//!
//! Entity identities keep the `sldprt:brep` namespaces of the shared graph
//! builder. Neutral consumers treat identities as opaque.

use cadmpeg_core::decode::{u64_from_index, DecodeContext};
use cadmpeg_core::CodecError;
use cadmpeg_ir::annotations::Annotations;
use cadmpeg_ir::document::Model;
use cadmpeg_ir::ids::UnknownId;
use cadmpeg_ir::report::loss::LossNote;
use cadmpeg_ir::unknown::UnknownRecord;
use cadmpeg_ir::StreamName;

use crate::brep::graph::{decode_bodies_in, Hosting};
use crate::decode::{bind_opaque_geometry, brep_losses};
use crate::parasolid::{is_body_stream, stream_header, StreamHeader};

/// The transmit header of one hosted Parasolid stream.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct HostedHeader {
    /// Human-readable stream description.
    pub description: String,
    /// `SCH_<modeller>_<schema>_<format>` schema token.
    pub schema: cadmpeg_parasolid::OwnedSchemaToken,
    /// Whether the description names a partition or a deltas stream.
    pub is_body_stream: bool,
}

/// Read the transmit header of one hosted stream, or `None` when the bytes do
/// not start with a complete Parasolid header.
#[doc(hidden)]
pub fn read_hosted_header(
    ctx: &DecodeContext<'_>,
    stream: &[u8],
) -> Result<Option<HostedHeader>, CodecError> {
    Ok(stream_header(ctx, stream)?.map(|header| HostedHeader {
        is_body_stream: is_body_stream(&header),
        description: header.description,
        schema: header.schema,
    }))
}

/// What a hosted decode produced besides the neutral arenas it wrote.
#[doc(hidden)]
#[derive(Debug)]
pub struct HostedBrep {
    /// Source locations of the decoded entities.
    pub annotations: Annotations,
    /// Carriers kept as opaque records.
    pub unknowns: Vec<UnknownRecord>,
    /// Losses of the B-rep decode.
    pub losses: Vec<LossNote>,
    /// Schema token of the first stream.
    pub schema: cadmpeg_parasolid::OwnedSchemaToken,
    /// Number of neutral entities written.
    pub entities: u64,
    /// Number of face color records the neutral model does not take from a
    /// hosted stream.
    pub face_color_records: usize,
    /// Structural diagnostics of the dropped edges as JSON lines, when asked.
    pub diagnostics: Vec<String>,
}

/// Decode hosted Parasolid streams into the neutral B-rep arenas of `model`.
///
/// `streams` holds binary transmit streams that start with `PS\0\0`; a host
/// that wraps them in the text transmit header removes that header first. A
/// partition stream comes before its deltas stream; the decoder orders them
/// itself. `origin` names the provenance source for annotations. `source` is
/// the unknown-record identity that opaque carriers link to.
///
/// The host decides which streams are bodies. A stream the host passes is
/// decoded whatever its description says, so a standalone transmit stream is
/// admitted as a base state. A stream without a Parasolid header returns
/// [`CodecError::WrongFormat`].
#[doc(hidden)]
pub fn decode_hosted_brep(
    ctx: &DecodeContext<'_>,
    model: &mut Model,
    streams: &[&[u8]],
    origin: &StreamName,
    source: &UnknownId,
    diagnostics: bool,
) -> Result<HostedBrep, CodecError> {
    let mut headers: Vec<StreamHeader> =
        ctx.collection_vec(streams.len(), "collect hosted Parasolid headers")?;
    for stream in streams {
        let header = stream_header(ctx, stream)?.ok_or_else(|| {
            CodecError::WrongFormat("hosted stream has no Parasolid transmit header".into())
        })?;
        headers.push(header);
    }
    let first = headers
        .first()
        .ok_or_else(|| CodecError::WrongFormat("no hosted Parasolid stream".into()))?;
    let schema = first.schema.clone();
    let mut bodies: Vec<(&[u8], &StreamHeader)> =
        ctx.collection_vec(streams.len(), "pair hosted Parasolid streams")?;
    bodies.extend(streams.iter().copied().zip(&headers));
    let mut brep = decode_bodies_in(
        ctx,
        &bodies,
        origin,
        Hosting {
            rules: true,
            diagnostics,
        },
    )?;
    bind_opaque_geometry(ctx, &mut brep, source)?;
    let losses = brep_losses(ctx, &mut brep, true)?;
    let mut lines = std::mem::take(&mut brep.stats.diagnostics.lines);
    if diagnostics {
        let stats = &brep.stats;
        lines.insert(
            0,
            format!(
                "{{\"kind\":\"summary\",\"unresolved\":{},\"null_vertex\":{},\
                 \"absent_vertex\":{},\"absent_point\":{},\"no_canonical\":{},\
                 \"no_forward\":{},\"several_forward\":{},\"off_ring\":{},\
                 \"dropped_loops\":{},\"dropped_loops_missing_coedge\":{},\
                 \"dropped_loops_unemitted_edge\":{},\"faces_without_loops\":{},\
                 \"findings_over_limit\":{},\"empty_faces_over\":{},\
                 \"admitted_14\":{},\"admitted_15\":{},\"admitted_16\":{},\"admitted_17\":{},\
                 \"admitted_18\":{},\"admitted_ambiguous\":{},\"admitted_examined\":{},\"admitted_duplicate_owner\":{},\"admitted_wide\":{},\"faces_empty_end\":{},\
                 \"faces_empty_no_head\":{},\"faces_empty_loop_absent\":{},\
                 \"faces_empty_other_owner\":{},\"faces_empty_ring_coedge_absent\":{},\"faces_empty_ring_foreign_loop\":{},\"faces_empty_ring_cycle\":{},\"faces_empty_ring_null_next\":{}}}",
                stats.unresolved_edges,
                stats.unresolved_null_vertex_use,
                stats.unresolved_absent_vertex_use,
                stats.unresolved_absent_point,
                stats.skipped_edges_no_canonical,
                stats.skipped_edges_no_forward,
                stats.skipped_edges_several_forward,
                stats.skipped_edges_off_ring,
                stats.dropped_loops,
                stats.dropped_loops_missing_coedge,
                stats.dropped_loops_unemitted_edge,
                stats.faces_without_loops,
                stats.diagnostics.overflow,
                stats.diagnostics.empty_faces_over,
                stats.admitted_by_reference.by_type[0],
                stats.admitted_by_reference.by_type[1],
                stats.admitted_by_reference.by_type[2],
                stats.admitted_by_reference.by_type[3],
                stats.admitted_by_reference.by_type[4],
                stats.admitted_by_reference.ambiguous,
                stats.admitted_by_reference.examined,
                stats.admitted_by_reference.duplicate_owner,
                stats.admitted_by_reference.wide,
                stats.faces_walked_empty_end,
                stats.faces_walked_empty_no_head,
                stats.faces_walked_empty_loop_absent,
                stats.faces_walked_empty_other_owner,
                stats.faces_walked_empty_ring_coedge_absent,
                stats.faces_walked_empty_ring_foreign_loop,
                stats.faces_walked_empty_ring_cycle,
                stats.faces_walked_empty_ring_null_next,
            ),
        );
    }
    if diagnostics {
        let stats = &brep.stats;
        lines.insert(
            1,
            format!(
                "{{\"kind\":\"summary2\",\"admitted_29\":{},\"admitted_unreadable\":{},\
                 \"admitted_no_header\":{},\"admitted_inconsistent\":{},\
                 \"admitted_no_point\":{},\"unclaimed_faces\":{},\"unclaimed_admitted\":{},\
                 \"faces_loops_all_dropped\":{}}}",
                stats.admitted_by_reference.by_type[5],
                stats.admitted_by_reference.unreadable,
                stats.admitted_by_reference.no_header,
                stats.admitted_by_reference.inconsistent,
                stats.admitted_by_reference.no_point,
                stats.unclaimed_faces,
                stats.unclaimed_admitted,
                stats.faces_loops_all_dropped,
            ),
        );
    }
    let entities = brep.neutral_entity_count()?;
    model.bodies = brep.bodies;
    model.regions = brep.regions;
    model.shells = brep.shells;
    model.faces = brep.faces;
    model.loops = brep.loops;
    model.coedges = brep.coedges;
    model.edges = brep.edges;
    model.vertices = brep.vertices;
    model.points = brep.points;
    model.surfaces = brep.surfaces;
    model.procedural_surfaces = brep.procedural_surfaces;
    model.curves = brep.curves;
    model.pcurves = brep.pcurves;
    ctx.charge_work(
        u64_from_index(brep.face_colors.len()),
        "count hosted face colors",
    )?;
    Ok(HostedBrep {
        annotations: brep.annotations,
        unknowns: brep.unknowns,
        losses,
        schema,
        entities,
        face_color_records: brep.face_colors.len(),
        diagnostics: lines,
    })
}
