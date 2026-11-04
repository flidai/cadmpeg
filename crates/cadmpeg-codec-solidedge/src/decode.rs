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
use cadmpeg_core::dialect::DialectLayers;
use cadmpeg_core::CodecError;
use cadmpeg_ir::codec::{DecodeBody, Decoded};
use cadmpeg_ir::document::{CadIr, SourceMeta};
use cadmpeg_ir::ids::UnknownId;
use cadmpeg_ir::report::loss::LossNote;
use cadmpeg_ir::topology::EdgeCarrier;
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

/// The streams of one body group that decode reads.
struct Group<'a> {
    /// The stem of the group in ASCII upper case.
    key: String,
    base: BodyStream<'a>,
    deltas: Option<BodyStream<'a>>,
}

impl<'a> Group<'a> {
    fn streams(&self) -> impl Iterator<Item = &BodyStream<'a>> {
        std::iter::once(&self.base).chain(self.deltas.as_ref())
    }
}

/// The structure of one document, shared by inspection and decode.
struct Analysis<'a> {
    /// Every body group with a base state, in decode order.
    groups: Vec<Group<'a>>,
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
    let found = scan.body_groups(ctx)?;
    let mut losses = Vec::new();
    let mut notes = Vec::new();

    for deviation in scan.deviations() {
        let message = ctx.format_retained(
            format_args!(
                "the Compound File was read although {}; readers in common use accept this",
                deviation.description()
            ),
            "retain Solid Edge tolerated container loss",
        )?;
        push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerCfbTolerated,
            message,
        )?;
    }

    let mut groups = Vec::new();
    let mut deltas_only = 0_usize;
    for group in &found {
        let Some((entry, role)) = group.base() else {
            deltas_only += 1;
            continue;
        };
        let base = open_body(ctx, scan, entry, role)?;
        let deltas = match (role, group.deltas) {
            (StreamRole::Partition, Some(entry)) => {
                Some(open_body(ctx, scan, entry, StreamRole::Deltas)?)
            }
            _ => None,
        };
        if role == StreamRole::Partition && deltas.is_none() {
            let message = ctx.format_retained(
                format_args!(
                    "the body group {} has a partition stream and no deltas stream; the \
                     partition stream is decoded alone",
                    group.key
                ),
                "retain Solid Edge deltas loss",
            )?;
            push_loss(
                ctx,
                &mut losses,
                SolidEdgeLossCode::ContainerDeltasMissing,
                message,
            )?;
        }
        let key = ctx.copy_retained_text(group.key, "retain Solid Edge group key")?;
        ctx.push_vec(
            &mut groups,
            Group { key, base, deltas },
            "collect Solid Edge body groups",
        )?;
    }

    let mut kernels = Vec::new();
    for body in groups.iter().flat_map(Group::streams) {
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

    match (scan.storage(), groups.is_empty()) {
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
        (Some(_), true) if deltas_only > 0 => push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerNoPartitionStream,
            "the PARASOLID storage has a deltas stream and no partition or standalone \
             transmit stream; deltas alone have no base state to apply to and are not decoded",
        )?,
        (Some(_), true) => push_loss(
            ctx,
            &mut losses,
            SolidEdgeLossCode::ContainerNoBodyStream,
            "the PARASOLID storage holds no stream that carries a body",
        )?,
        (Some(_), false) => {}
    }

    if !groups.is_empty() {
        for group in found.iter().filter(|group| group.base().is_none()) {
            let message = ctx.format_retained(
                format_args!(
                    "the body group {} has a deltas stream and no base state; its streams are \
                     listed in the container summary and not transferred",
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
    for body in groups.iter().flat_map(Group::streams) {
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
        groups,
        classification,
        losses,
        notes,
    })
}

/// The result for a Compound File whose directory cannot be read: the failing
/// check is named in the loss.
fn unreadable(
    ctx: &DecodeContext<'_>,
    detail: &str,
) -> Result<(DialectLayers, Vec<LossNote>), CodecError> {
    let mut losses = Vec::new();
    let message = ctx.format_retained(
        format_args!(
            "the Compound File directory cannot be read: {detail}; no stream was listed or \
             decoded"
        ),
        "retain Solid Edge unreadable loss",
    )?;
    push_loss(
        ctx,
        &mut losses,
        SolidEdgeLossCode::ContainerUnreadable,
        message,
    )?;
    let (layers, dialect_losses) = dialect::classify_layers(ctx, None, Vec::new())?.into_parts();
    ctx.extend_vec(&mut losses, dialect_losses, "collect Solid Edge losses")?;
    Ok((layers, losses))
}

/// Scan the directory. A structural defect of the Compound File is a typed
/// result and not an error; every other failure stays an error.
fn scan_or_unreadable<'a>(
    ctx: &DecodeContext<'a>,
    root: View<'a>,
) -> Result<Result<Scan<'a>, String>, CodecError> {
    match container::scan(ctx, root) {
        Ok(scan) => Ok(Ok(scan)),
        Err(CodecError::Malformed(detail) | CodecError::NotImplemented(detail)) => Ok(Err(detail)),
        Err(error) => Err(error),
    }
}

/// Report the directory structure without building the model.
pub(crate) fn inspect(
    ctx: &DecodeContext<'_>,
    root: View<'_>,
) -> Result<ContainerSummary, CodecError> {
    let scan = match scan_or_unreadable(ctx, root)? {
        Ok(scan) => scan,
        Err(detail) => {
            let (layers, losses) = unreadable(ctx, &detail)?;
            return Ok(ContainerSummary::classified(
                layers,
                ContainerKind::Cfb,
                Vec::new(),
                losses,
                Vec::new(),
            ));
        }
    };
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

/// Withhold every edge range and coedge pcurve range that the model range
/// checks reject, and count them. A range that fails the checks is wrong or not
/// provable from the stream. The vertices bound the edge without it, and a
/// wrong range would make the exported curve wrong.
pub(crate) fn withhold_rejected_ranges(
    ctx: &DecodeContext<'_>,
    ir: &mut CadIr,
) -> Result<(usize, usize), CodecError> {
    let findings = cadmpeg_ir::validate::range_findings(ctx, ir)?;
    if findings.is_empty() {
        return Ok((0, 0));
    }
    let mut rejected = std::collections::HashSet::new();
    for finding in &findings {
        if let Some(id) = finding.entity.as_deref() {
            ctx.insert_hash_set(&mut rejected, id, "index rejected Solid Edge ranges")?;
        }
    }
    let mut edges = 0;
    for edge in &mut ir.model.edges {
        if !rejected.contains(edge.id.as_str()) {
            continue;
        }
        let carrier = std::mem::replace(&mut edge.carrier, EdgeCarrier::Free);
        edge.carrier = match carrier {
            EdgeCarrier::Bounded(curve, _) => {
                edges += 1;
                EdgeCarrier::Curve(curve)
            }
            other => other,
        };
    }
    let mut pcurves = 0;
    for coedge in &mut ir.model.coedges {
        if !rejected.contains(coedge.id.as_str()) {
            continue;
        }
        for use_ in &mut coedge.pcurves {
            if use_.parameter_range.take().is_some() {
                pcurves += 1;
            }
        }
    }
    Ok((edges, pcurves))
}

/// Decode the Parasolid body groups into the neutral B-rep.
pub(crate) fn decode(ctx: &DecodeContext<'_>, root: View<'_>) -> Result<Decoded, CodecError> {
    let scan = match scan_or_unreadable(ctx, root)? {
        Ok(scan) => scan,
        Err(detail) => {
            let (layers, mut losses) = unreadable(ctx, &detail)?;
            let ir = CadIr::decoded(SourceMeta::classified(
                layers,
                std::collections::BTreeMap::new(),
            ));
            let transfer = if ctx.container_only() {
                cadmpeg_ir::report::decode::DecodeTransfer::ContainerOnly {}
            } else {
                push_loss(
                    ctx,
                    &mut losses,
                    SolidEdgeLossCode::GeometryNotTransferred,
                    "no B-rep geometry was transferred; the other losses of this report state why",
                )?;
                cadmpeg_ir::report::decode::DecodeTransfer::full(false)
            };
            return Ok(Decoded {
                ir,
                body: DecodeBody {
                    losses,
                    ..DecodeBody::new(transfer)
                },
                source_fidelity: SourceFidelity::with_annotations(
                    cadmpeg_ir::Annotations::default(),
                ),
            });
        }
    };
    let Analysis {
        groups,
        classification,
        mut losses,
        mut notes,
    } = analyze(ctx, &scan)?;
    let (layers, dialect_losses) = classification.into_parts();
    ctx.extend_vec(&mut losses, dialect_losses, "collect Solid Edge losses")?;

    let mut attributes = std::collections::BTreeMap::new();
    if let Some(first) = groups.first() {
        if let Some(schema) = first.base.schema() {
            attributes.insert(
                cadmpeg_core::nonblank_literal!("parasolid_schema"),
                ctx.copy_retained_text(schema.value(), "retain Solid Edge schema")?,
            );
        }
        if let Some(header) = first.base.header.as_ref() {
            attributes.insert(
                cadmpeg_core::nonblank_literal!("parasolid_description"),
                ctx.copy_retained_text(&header.description, "retain Solid Edge description")?,
            );
        }
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_stream_group"),
            ctx.copy_retained_text(&first.key, "retain Solid Edge group key")?,
        );
        let keys: Vec<&str> = groups.iter().map(|group| group.key.as_str()).collect();
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_body_groups"),
            keys.join(","),
        );
    }
    attributes.insert(
        cadmpeg_core::nonblank_literal!("parasolid_stream_count"),
        scan.entries().len().to_string(),
    );
    let mut ir = CadIr::decoded(SourceMeta::classified(layers, attributes));

    let mut unknowns = Vec::new();
    let mut sources = Vec::new();
    for group in &groups {
        let mut source = None;
        for body in group.streams() {
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
            let bytes =
                ctx.copy_retained(body.view.window(), "retain Solid Edge Parasolid stream")?;
            ctx.push_vec(
                &mut unknowns,
                UnknownRecord::retained(id, u64_from_index(body.view.start()), bytes, Vec::new()),
                "collect Solid Edge retained streams",
            )?;
        }
        ctx.push_vec(&mut sources, source, "collect Solid Edge group sources")?;
    }

    let mut annotations = cadmpeg_ir::Annotations::default();
    let mut decoded_groups = 0_usize;
    let mut admitted = 0_u64;
    let mut face_colors = 0_usize;
    if !ctx.container_only() {
        for (group, source) in groups.iter().zip(&sources) {
            let (true, Some(source)) = (group.base.decodable(), source.as_ref()) else {
                continue;
            };
            if decoded_groups > 0 {
                // The first group is the design body. A later group holds a
                // flat pattern, reference geometry, or a design region (SE-01),
                // and an export of it would add bodies that the design does
                // not have. Its streams stay retained as native records.
                let message = ctx.format_retained(
                    format_args!(
                        "the body group {} is a secondary group; its streams are retained as \
                         native records and not decoded, because the first group is the design \
                         body",
                        group.key
                    ),
                    "retain Solid Edge secondary group loss",
                )?;
                push_loss(
                    ctx,
                    &mut losses,
                    SolidEdgeLossCode::ContainerBodyGroupUntransferred,
                    message,
                )?;
                continue;
            }
            let mut streams: Vec<&[u8]> = ctx.collection_vec(2, "pair Solid Edge streams")?;
            streams.push(group.base.binary());
            if let Some(deltas) = group.deltas.as_ref().filter(|body| body.decodable()) {
                streams.push(deltas.binary());
            }
            let origin = cadmpeg_ir::stream_name!("solidedge:parasolid");
            let hosted = decode_hosted_brep(ctx, &mut ir.model, &streams, &origin, source)?;
            ctx.admit_entities(hosted.entities, &mut admitted, "admit Solid Edge entities")?;
            face_colors += hosted.face_color_records;
            annotations = hosted.annotations;
            ctx.extend_vec(&mut losses, hosted.losses, "collect Solid Edge losses")?;
            ctx.extend_vec(
                &mut unknowns,
                hosted.unknowns,
                "collect Solid Edge unknowns",
            )?;
            let note = ctx.format_retained(
                format_args!(
                    "body group {}: {} entities decoded",
                    group.key, hosted.entities
                ),
                "retain Solid Edge group note",
            )?;
            ctx.push_vec(&mut notes, note, "collect Solid Edge notes")?;
            decoded_groups += 1;
        }
        let (edges, pcurves) = withhold_rejected_ranges(ctx, &mut ir)?;
        if edges + pcurves > 0 {
            let message = ctx.format_retained(
                format_args!(
                    "{edges} edge parameter range(s) and {pcurves} coedge pcurve range(s) failed \
                     the model range checks and are withheld; the vertices still bound each edge"
                ),
                "retain Solid Edge range loss",
            )?;
            push_loss(
                ctx,
                &mut losses,
                SolidEdgeLossCode::GeometryRangeWithheld,
                message,
            )?;
        }
        if face_colors > 0 {
            let message = ctx.format_retained(
                format_args!(
                    "{face_colors} face color record(s) in the Parasolid streams are not \
                     transferred"
                ),
                "retain Solid Edge face color loss",
            )?;
            push_loss(
                ctx,
                &mut losses,
                SolidEdgeLossCode::AppearanceFaceColorUntransferred,
                message,
            )?;
        }
    }
    let geometry_transferred =
        !(ir.model.surfaces.is_empty() && ir.model.points.is_empty() && ir.model.faces.is_empty());

    let transfer = if ctx.container_only() {
        cadmpeg_ir::report::decode::DecodeTransfer::ContainerOnly {}
    } else {
        if !geometry_transferred {
            push_loss(
                ctx,
                &mut losses,
                SolidEdgeLossCode::GeometryNotTransferred,
                "no B-rep geometry was transferred; the other losses of this report state why",
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
