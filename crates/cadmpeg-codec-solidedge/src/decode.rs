// SPDX-License-Identifier: Apache-2.0
//! Inspection and decoding of a Solid Edge Compound File.
//!
//! [`analyze`] reads the directory once and classifies the `PARASOLID`
//! storage: which streams form the decoded pair, which entries decode skips,
//! and which losses the structure alone charges. [`inspect`] reports that
//! analysis. [`decode`] hands the partition stream, and the deltas stream when
//! present, to the shared Parasolid B-rep decoder and writes the result into
//! the document.

use cadmpeg_codec_sldprt::{decode_hosted_brep, read_hosted_header, HostedHeader};
use cadmpeg_core::decode::{u64_from_index, DecodeContext, View};
use cadmpeg_core::CodecError;
use cadmpeg_ir::codec::{DecodeBody, Decoded};
use cadmpeg_ir::document::{CadIr, SourceMeta};
use cadmpeg_ir::ids::UnknownId;
use cadmpeg_ir::report::loss::LossNote;
use cadmpeg_ir::unknown::UnknownRecord;
use cadmpeg_ir::{ContainerKind, ContainerSummary, SourceFidelity};
use cadmpeg_parasolid::{Carrier, OwnedSchemaToken};

use crate::container::{self, ParasolidEntry, Scan, StreamRole, BODY_STREAM_INDEX};
use crate::dialect::{self, LayerClassification};
use crate::loss::SolidEdgeLossCode;

/// One opened stream of the decoded pair.
struct BodyStream<'a> {
    entry: &'a ParasolidEntry,
    view: View<'a>,
    directory_id: u32,
    header: Option<HostedHeader>,
}

/// The structure of one document, shared by inspection and decode.
struct Analysis<'a> {
    partition: Option<BodyStream<'a>>,
    deltas: Option<BodyStream<'a>>,
    classification: LayerClassification,
    losses: Vec<LossNote>,
    notes: Vec<String>,
}

fn open_body<'a>(
    ctx: &DecodeContext<'a>,
    scan: &'a Scan<'a>,
    role: StreamRole,
) -> Result<Option<BodyStream<'a>>, CodecError> {
    let Some(entry) = scan.body_stream(role) else {
        return Ok(None);
    };
    let directory_id = scan
        .stream(entry)
        .map(|stream| stream.id().directory_id())
        .ok_or_else(|| CodecError::malformed("Solid Edge stream entry has no directory stream"))?;
    let view = scan.open(ctx, entry)?;
    let header = read_hosted_header(ctx, view.window())?;
    Ok(Some(BodyStream {
        entry,
        view,
        directory_id,
        header,
    }))
}

fn analyze<'a>(ctx: &DecodeContext<'a>, scan: &'a Scan<'a>) -> Result<Analysis<'a>, CodecError> {
    let partition = open_body(ctx, scan, StreamRole::Partition)?;
    let deltas = open_body(ctx, scan, StreamRole::Deltas)?;
    let mut losses = Vec::new();
    let mut notes = Vec::new();

    let mut kernels = Vec::new();
    for body in [&partition, &deltas].into_iter().flatten() {
        if let Some(header) = &body.header {
            let carrier = ctx.copy_retained_text(&body.entry.path, "retain Solid Edge carrier")?;
            let schema = OwnedSchemaToken::try_from(
                ctx.copy_retained_text(header.schema.value(), "retain Solid Edge schema")?,
            )
            .map_err(|_| CodecError::malformed("invalid admitted Parasolid schema"))?;
            ctx.push_vec(
                &mut kernels,
                (schema, Carrier::new(carrier)),
                "collect Solid Edge kernel layers",
            )?;
        } else {
            let message = ctx.format_retained(
                format_args!(
                    "{} has no Parasolid transmit header; the stream is retained and not decoded",
                    body.entry.path
                ),
                "retain Solid Edge header loss",
            )?;
            ctx.push_vec(
                &mut losses,
                SolidEdgeLossCode::ParasolidHeaderUnreadable.note(message),
                "collect Solid Edge losses",
            )?;
        }
    }

    match (scan.storage(), &partition, &deltas) {
        (None, ..) => ctx.push_vec(
            &mut losses,
            SolidEdgeLossCode::ContainerNoParasolidStorage.note(
                "the Compound File has no PARASOLID storage; no geometry stream exists to read",
            ),
            "collect Solid Edge losses",
        )?,
        (Some(_), None, None) => ctx.push_vec(
            &mut losses,
            SolidEdgeLossCode::ContainerNoPartitionStream.note(
                "the PARASOLID storage has no STREAM0.P_B partition stream and no deltas stream",
            ),
            "collect Solid Edge losses",
        )?,
        (Some(_), None, Some(_)) => ctx.push_vec(
            &mut losses,
            SolidEdgeLossCode::ContainerNoPartitionStream.note(
                "the PARASOLID storage has no STREAM0.P_B partition stream; a deltas stream \
                 alone has no base state to apply to and is not decoded",
            ),
            "collect Solid Edge losses",
        )?,
        (Some(_), Some(_), None) => ctx.push_vec(
            &mut losses,
            SolidEdgeLossCode::ContainerDeltasMissing.note(
                "the PARASOLID storage has no STREAM0.D_B deltas stream; the partition stream \
                 is decoded alone",
            ),
            "collect Solid Edge losses",
        )?,
        (Some(_), Some(_), Some(_)) => {}
    }

    for entry in scan.entries() {
        let in_pair = entry
            .name
            .is_some_and(|name| name.index == BODY_STREAM_INDEX);
        if in_pair {
            continue;
        }
        let message = ctx.format_retained(
            format_args!(
                "{} is not part of the STREAM0 pair that decode reads; it is listed in the \
                 container summary and not transferred",
                entry.path
            ),
            "retain Solid Edge untransferred stream loss",
        )?;
        ctx.push_vec(
            &mut losses,
            SolidEdgeLossCode::ContainerStreamUntransferred.note(message),
            "collect Solid Edge losses",
        )?;
    }

    let classification = dialect::classify_layers(ctx, scan.storage(), kernels)?;
    for body in [&partition, &deltas].into_iter().flatten() {
        let header = body.header.as_ref();
        let note = ctx.format_retained(
            format_args!(
                "{}: {} stream{}",
                body.entry.path,
                body.entry
                    .name
                    .map_or("unclassified", |name| name.role.label()),
                header.map_or(String::new(), |header| format!(
                    ", description {:?}, schema {}",
                    header.description,
                    header.schema.value()
                ))
            ),
            "retain Solid Edge stream note",
        )?;
        ctx.push_vec(&mut notes, note, "collect Solid Edge notes")?;
    }
    Ok(Analysis {
        partition,
        deltas,
        classification,
        losses,
        notes,
    })
}

/// Report the directory structure without building the model.
pub(crate) fn inspect(
    ctx: &DecodeContext<'_>,
    root: View<'_>,
) -> Result<ContainerSummary, CodecError> {
    let scan = container::scan(ctx, root)?;
    let Analysis {
        classification,
        mut losses,
        notes,
        ..
    } = analyze(ctx, &scan)?;
    let (layers, dialect_losses) = classification.into_parts();
    ctx.extend_vec(&mut losses, dialect_losses, "collect Solid Edge losses")?;
    Ok(ContainerSummary::classified(
        layers,
        ContainerKind::Cfb,
        scan.container_entries(ctx)?,
        losses,
        notes,
    ))
}

/// Decode the Parasolid pair into the neutral B-rep.
pub(crate) fn decode(ctx: &DecodeContext<'_>, root: View<'_>) -> Result<Decoded, CodecError> {
    let scan = container::scan(ctx, root)?;
    let Analysis {
        partition,
        deltas,
        classification,
        mut losses,
        notes,
    } = analyze(ctx, &scan)?;
    let (layers, dialect_losses) = classification.into_parts();
    ctx.extend_vec(&mut losses, dialect_losses, "collect Solid Edge losses")?;

    let mut attributes = std::collections::BTreeMap::new();
    if let Some(header) = partition.as_ref().and_then(|body| body.header.as_ref()) {
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_schema"),
            ctx.copy_retained_text(header.schema.value(), "retain Solid Edge schema")?,
        );
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_description"),
            ctx.copy_retained_text(&header.description, "retain Solid Edge description")?,
        );
    }
    attributes.insert(
        cadmpeg_core::nonblank_literal!("parasolid_stream_count"),
        scan.entries().len().to_string(),
    );
    let mut ir = CadIr::decoded(SourceMeta::classified(layers, attributes));

    let mut unknowns = Vec::new();
    let mut source = None;
    for body in [&partition, &deltas].into_iter().flatten() {
        ctx.charge_entities(1, "admit Solid Edge Parasolid streams")?;
        let id = UnknownId::compose(
            &cadmpeg_ir::identity_namespace!("solidedge", "file", "parasolid-stream"),
            body.directory_id,
        );
        if source.is_none() {
            source = Some(
                UnknownId::mint(
                    ctx.copy_retained_text(id.as_str(), "retain Solid Edge source identity")?,
                )
                .map_err(CodecError::malformed)?,
            );
        }
        let bytes = ctx.copy_retained(body.view.window(), "retain Solid Edge Parasolid stream")?;
        ctx.push_vec(
            &mut unknowns,
            UnknownRecord::retained(id, u64_from_index(body.view.start()), bytes, Vec::new()),
            "collect Solid Edge retained streams",
        )?;
    }

    let decodable = partition
        .as_ref()
        .filter(|body| body.header.is_some())
        .zip(source);
    let mut annotations = cadmpeg_ir::Annotations::default();
    let mut geometry_transferred = false;
    if let (false, Some((partition, source))) = (ctx.container_only(), decodable) {
        let mut streams: Vec<&[u8]> = ctx.collection_vec(2, "pair Solid Edge streams")?;
        streams.push(partition.view.window());
        if let Some(deltas) = deltas.as_ref().filter(|body| body.header.is_some()) {
            streams.push(deltas.view.window());
        }
        let hosted = decode_hosted_brep(
            ctx,
            &mut ir,
            &streams,
            &cadmpeg_ir::stream_name!("solidedge:parasolid"),
            &source,
        )?;
        let mut admitted = 0_u64;
        ctx.admit_entities(hosted.entities, &mut admitted, "admit Solid Edge entities")?;
        ctx.extend_vec(&mut losses, hosted.losses, "collect Solid Edge losses")?;
        ctx.extend_vec(
            &mut unknowns,
            hosted.unknowns,
            "collect Solid Edge unknowns",
        )?;
        if hosted.face_color_records > 0 {
            let message = ctx.format_retained(
                format_args!(
                    "{} face color record(s) in the Parasolid streams are not transferred",
                    hosted.face_color_records
                ),
                "retain Solid Edge face color loss",
            )?;
            ctx.push_vec(
                &mut losses,
                SolidEdgeLossCode::AppearanceFaceColorUntransferred.note(message),
                "collect Solid Edge losses",
            )?;
        }
        annotations = hosted.annotations;
        geometry_transferred = !(ir.model.surfaces.is_empty()
            && ir.model.points.is_empty()
            && ir.model.faces.is_empty());
    }

    let transfer = if ctx.container_only() {
        cadmpeg_ir::report::decode::DecodeTransfer::ContainerOnly {}
    } else {
        if !geometry_transferred {
            ctx.push_vec(
                &mut losses,
                SolidEdgeLossCode::GeometryNotTransferred.note(
                    "no B-rep geometry was transferred: the Parasolid streams, when present, \
                     decoded no surfaces, points, or faces; they are retained as unknown records",
                ),
                "collect Solid Edge losses",
            )?;
        }
        cadmpeg_ir::report::decode::DecodeTransfer::full(geometry_transferred)
    };
    let mut source_fidelity = SourceFidelity::with_annotations(annotations);
    source_fidelity.attach_native_unknown_records(&mut ir, dialect::FORMAT, unknowns, ctx)?;
    Ok(Decoded {
        ir,
        body: DecodeBody {
            losses,
            notes,
            ..DecodeBody::new(transfer)
        },
        source_fidelity,
    })
}
