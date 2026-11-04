// SPDX-License-Identifier: Apache-2.0
//! Inspection and decoding of a Solid Edge Compound File.
//!
//! [`analyze`] reads the directory once and classifies the `PARASOLID`
//! storage: which body group decode reads, which entries it skips, and which
//! losses the structure alone charges. [`inspect`] reports that analysis.
//! [`decode`] removes the text transmit header from the base stream and from
//! the deltas stream when present, then hands the binary parts to the shared
//! Parasolid B-rep decoder and writes the result into the document.

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

use crate::container::{self, DocumentKind, ParasolidEntry, Scan, StreamRole};
use crate::dialect::{self, LayerClassification};
use crate::loss::SolidEdgeLossCode;
use crate::transmit::{self, Transmit};

/// Name of the root stream that holds the document version code.
const DOC_VERSION_STREAM: &str = "DocVersion2";

/// One opened stream of the decoded group.
struct BodyStream<'a> {
    entry: &'a ParasolidEntry,
    role: StreamRole,
    view: View<'a>,
    directory_id: u32,
    transmit: Transmit,
    /// The header of the binary part, when the decoder can read it.
    header: Option<HostedHeader>,
}

impl BodyStream<'_> {
    /// The binary part of the stream.
    fn binary(&self) -> &[u8] {
        let window = self.view.window();
        window.get(self.transmit.binary_offset..).unwrap_or(window)
    }

    /// The schema token that classifies the stream: the text header states it
    /// first, the binary header second.
    fn schema(&self) -> Option<&OwnedSchemaToken> {
        self.transmit
            .text_schema
            .as_ref()
            .or_else(|| self.header.as_ref().map(|header| &header.schema))
    }

    /// Whether the decoder can read the binary part.
    fn decodable(&self) -> bool {
        self.header.is_some() && !self.transmit.text_encoded
    }
}

/// The structure of one document, shared by inspection and decode.
struct Analysis<'a> {
    /// The key of the body group that decode reads.
    group: Option<String>,
    base: Option<BodyStream<'a>>,
    deltas: Option<BodyStream<'a>>,
    classification: LayerClassification,
    losses: Vec<LossNote>,
    notes: Vec<String>,
}

fn open_body<'a>(
    ctx: &DecodeContext<'a>,
    scan: &'a Scan<'a>,
    entry: &'a ParasolidEntry,
    role: StreamRole,
) -> Result<BodyStream<'a>, CodecError> {
    let directory_id = scan
        .stream(entry)
        .map(|stream| stream.id().directory_id())
        .ok_or_else(|| CodecError::malformed("Solid Edge stream entry has no directory stream"))?;
    let view = scan.open(ctx, entry)?;
    let transmit = transmit::split(ctx, view.window())?;
    let binary = view
        .window()
        .get(transmit.binary_offset..)
        .unwrap_or(view.window());
    let header = read_hosted_header(ctx, binary)?;
    Ok(BodyStream {
        entry,
        role,
        view,
        directory_id,
        transmit,
        header,
    })
}

fn push_loss(
    ctx: &DecodeContext<'_>,
    losses: &mut Vec<LossNote>,
    code: SolidEdgeLossCode,
    message: impl Into<String>,
) -> Result<(), CodecError> {
    ctx.push_vec(losses, code.note(message), "collect Solid Edge losses")
}

fn analyze<'a>(ctx: &DecodeContext<'a>, scan: &'a Scan<'a>) -> Result<Analysis<'a>, CodecError> {
    let groups = scan.body_groups(ctx)?;
    let primary = groups.iter().position(|group| group.base().is_some());
    let mut losses = Vec::new();
    let mut notes = Vec::new();

    let mut base = None;
    let mut deltas = None;
    let mut group_key = None;
    if let Some(group) = primary.and_then(|index| groups.get(index)) {
        group_key = Some(ctx.copy_retained_text(group.key, "retain Solid Edge group key")?);
        if let Some((entry, role)) = group.base() {
            base = Some(open_body(ctx, scan, entry, role)?);
        }
        if base
            .as_ref()
            .is_some_and(|body| body.role == StreamRole::Partition)
        {
            if let Some(entry) = group.deltas {
                deltas = Some(open_body(ctx, scan, entry, StreamRole::Deltas)?);
            }
        }
    }

    let mut kernels = Vec::new();
    for body in [&base, &deltas].into_iter().flatten() {
        if let Some(schema) = body.schema() {
            let carrier = ctx.copy_retained_text(&body.entry.path, "retain Solid Edge carrier")?;
            let schema = OwnedSchemaToken::try_from(
                ctx.copy_retained_text(schema.value(), "retain Solid Edge schema")?,
            )
            .map_err(|_| CodecError::malformed("invalid admitted Parasolid schema"))?;
            ctx.push_vec(
                &mut kernels,
                (schema, Carrier::new(carrier)),
                "collect Solid Edge kernel layers",
            )?;
        }
        if !body.decodable() {
            let reason = if body.transmit.text_encoded {
                "declares a text encoding of the transmit data"
            } else if body.transmit.has_text_header {
                "has a text header that the binary transmit data does not follow"
            } else {
                "has no Parasolid transmit header"
            };
            let message = ctx.format_retained(
                format_args!(
                    "{} {reason}; the stream is retained and not decoded",
                    body.entry.path
                ),
                "retain Solid Edge header loss",
            )?;
            push_loss(
                ctx,
                &mut losses,
                SolidEdgeLossCode::ParasolidHeaderUnreadable,
                message,
            )?;
        }
    }

    match (scan.storage(), primary) {
        (None, _) if scan.kind() == DocumentKind::FamilyMaster => push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerNotModelDocument,
            "the Compound File is a family-of-parts master; it holds no model and no \
             geometry stream",
        )?,
        (None, _) => push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerNoParasolidStorage,
            "the Compound File has no PARASOLID storage; no geometry stream exists to read",
        )?,
        (Some(_), None) => {
            if groups.iter().any(|group| group.deltas.is_some()) {
                push_loss(
                    ctx,
                    &mut losses,
                    SolidEdgeLossCode::ContainerNoPartitionStream,
                    "the PARASOLID storage has a deltas stream and no partition or standalone \
                     transmit stream; deltas alone have no base state to apply to and are not \
                     decoded",
                )?;
            } else {
                push_loss(
                    ctx,
                    &mut losses,
                    SolidEdgeLossCode::ContainerNoBodyStream,
                    "the PARASOLID storage holds no stream that carries a body",
                )?;
            }
        }
        (Some(_), Some(_)) => {
            if base
                .as_ref()
                .is_some_and(|body| body.role == StreamRole::Partition)
                && deltas.is_none()
            {
                push_loss(
                    ctx,
                    &mut losses,
                    SolidEdgeLossCode::ContainerDeltasMissing,
                    "the body group has a partition stream and no deltas stream; the partition \
                     stream is decoded alone",
                )?;
            }
        }
    }

    for (index, group) in groups.iter().enumerate() {
        if Some(index) == primary || (primary.is_none() && group.base().is_none()) {
            continue;
        }
        let message = ctx.format_retained(
            format_args!(
                "the body group {} is not the group that decode reads; its streams are listed \
                 in the container summary and not transferred",
                group.key
            ),
            "retain Solid Edge body group loss",
        )?;
        push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerBodyGroupUntransferred,
            message,
        )?;
    }
    for entry in scan.sketches() {
        let message = ctx.format_retained(
            format_args!(
                "{} is a sketch stream; sketches are not transferred",
                entry.path
            ),
            "retain Solid Edge sketch loss",
        )?;
        push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerSketchUntransferred,
            message,
        )?;
    }
    for entry in scan.entries().iter().filter(|entry| entry.name.is_none()) {
        let message = ctx.format_retained(
            format_args!(
                "{} is not a Parasolid transmit stream; it is listed in the container \
                 summary and not transferred",
                entry.path
            ),
            "retain Solid Edge untransferred stream loss",
        )?;
        push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerStreamUntransferred,
            message,
        )?;
    }

    let classification = dialect::classify_layers(ctx, scan.storage(), kernels)?;
    for body in [&base, &deltas].into_iter().flatten() {
        let header = body.header.as_ref();
        let note = ctx.format_retained(
            format_args!(
                "{}: {} stream{}{}",
                body.entry.path,
                body.role.label(),
                body.transmit
                    .text_schema
                    .as_ref()
                    .map_or(String::new(), |schema| format!(
                        ", text header schema {}",
                        schema.value()
                    )),
                header.map_or(String::new(), |header| format!(
                    ", description {:?}, binary schema {}",
                    header.description,
                    header.schema.value()
                ))
            ),
            "retain Solid Edge stream note",
        )?;
        ctx.push_vec(&mut notes, note, "collect Solid Edge notes")?;
    }
    if let Some(mut view) = scan.open_root_stream(ctx, DOC_VERSION_STREAM)? {
        let length = view.window().len();
        let code = view.skip(2).and_then(|()| view.u16_le());
        if let (4, Some(code)) = (length, code) {
            let note = ctx.format_retained(
                format_args!("{DOC_VERSION_STREAM}: second u16 is {code}"),
                "retain Solid Edge version note",
            )?;
            ctx.push_vec(&mut notes, note, "collect Solid Edge notes")?;
        }
    }
    Ok(Analysis {
        group: group_key,
        base,
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
        group,
        base,
        deltas,
        classification,
        mut losses,
        notes,
    } = analyze(ctx, &scan)?;
    let (layers, dialect_losses) = classification.into_parts();
    ctx.extend_vec(&mut losses, dialect_losses, "collect Solid Edge losses")?;

    let mut attributes = std::collections::BTreeMap::new();
    if let Some(schema) = base.as_ref().and_then(BodyStream::schema) {
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_schema"),
            ctx.copy_retained_text(schema.value(), "retain Solid Edge schema")?,
        );
    }
    if let Some(header) = base.as_ref().and_then(|body| body.header.as_ref()) {
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_description"),
            ctx.copy_retained_text(&header.description, "retain Solid Edge description")?,
        );
    }
    if let Some(group) = group {
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_stream_group"),
            group,
        );
    }
    attributes.insert(
        cadmpeg_core::nonblank_literal!("parasolid_stream_count"),
        scan.entries().len().to_string(),
    );
    let mut ir = CadIr::decoded(SourceMeta::classified(layers, attributes));

    let mut unknowns = Vec::new();
    let mut source = None;
    for body in [&base, &deltas].into_iter().flatten() {
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

    let decodable = base.as_ref().filter(|body| body.decodable()).zip(source);
    let mut annotations = cadmpeg_ir::Annotations::default();
    let mut geometry_transferred = false;
    if let (false, Some((base, source))) = (ctx.container_only(), decodable) {
        let mut streams: Vec<&[u8]> = ctx.collection_vec(2, "pair Solid Edge streams")?;
        streams.push(base.binary());
        if let Some(deltas) = deltas.as_ref().filter(|body| body.decodable()) {
            streams.push(deltas.binary());
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
                    "no B-rep geometry was transferred; the other losses of this report state why",
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
