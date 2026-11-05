// SPDX-License-Identifier: Apache-2.0
//! Inspection and decoding of a standalone Parasolid transmit file.
//!
//! [`analyze`] reads the text header once and classifies the file: the schema
//! token, the declared encoding of the data, and the losses that the structure
//! alone charges. [`inspect`] reports that analysis. [`decode`] removes the text
//! header, hands the binary part to the shared Parasolid B-rep decoder, and
//! writes the result into the document. A file whose header declares a text
//! encoding is refused with the layers that inspection reports.

use cadmpeg_codec_sldprt::{decode_hosted_brep, read_hosted_header, HostedHeader};
use cadmpeg_core::container::{ContainerRole, EntryStorage, VerbatimLabel};
use cadmpeg_core::decode::{u64_from_index, DecodeContext, View};
use cadmpeg_core::dialect::DialectLayers;
use cadmpeg_core::{CodecError, ContainerEntry};
use cadmpeg_ir::codec::{DecodeBody, Decoded};
use cadmpeg_ir::document::{CadIr, SourceMeta};
use cadmpeg_ir::ids::UnknownId;
use cadmpeg_ir::report::loss::LossNote;
use cadmpeg_ir::unknown::UnknownRecord;
use cadmpeg_ir::{ContainerKind, ContainerSummary, SourceFidelity};
use cadmpeg_parasolid::OwnedSchemaToken;

use crate::dialect::{self, Encoding};
use crate::loss::ParasolidLossCode;
use crate::transmit::{self, Transmit};

/// Name of the one entry of the container summary: the file is the stream.
const ENTRY_NAME: &str = "transmit";

/// The structure of one file, shared by inspection and decode.
struct Analysis<'a> {
    bytes: &'a [u8],
    transmit: Transmit,
    /// The header of the binary part, when the decoder can read it.
    header: Option<HostedHeader>,
    /// The schema token that classifies the file: the text header states it
    /// first, the binary header second.
    schema: OwnedSchemaToken,
    layers: DialectLayers,
    losses: Vec<LossNote>,
    notes: Vec<String>,
}

fn analyze<'a>(ctx: &DecodeContext<'_>, bytes: &'a [u8]) -> Result<Analysis<'a>, CodecError> {
    if !transmit::is_transmit_header(bytes) {
        return Err(CodecError::WrongFormat(
            "input does not start with a Parasolid transmit header".into(),
        ));
    }
    let transmit = transmit::split(ctx, bytes)?;
    let text_encoded = transmit.text_encoded;
    let binary = bytes.get(transmit.binary_offset..).unwrap_or(bytes);
    let header = if text_encoded {
        None
    } else {
        read_hosted_header(ctx, binary)?
    };
    let schema = transmit
        .text_schema
        .clone()
        .or_else(|| header.as_ref().map(|header| header.schema.clone()))
        .ok_or_else(|| {
            CodecError::WrongFormat("the Parasolid transmit header states no schema token".into())
        })?;
    let encoding = if text_encoded {
        Encoding::Text
    } else {
        Encoding::Binary
    };
    let (layers, mut losses) = dialect::classify_layers(ctx, schema.clone(), encoding)?;
    let mut notes = Vec::new();
    if text_encoded {
        ctx.push_vec(
            &mut losses,
            ParasolidLossCode::TextTransmitUnsupported.note(TEXT_MESSAGE),
            "collect Parasolid losses",
        )?;
    } else if header.is_none() {
        ctx.push_vec(
            &mut losses,
            ParasolidLossCode::HeaderUnreadable.note(
                "the text header is not followed by binary transmit data with a stream header; \
                 the file is retained and not decoded",
            ),
            "collect Parasolid losses",
        )?;
    }
    if let Some(header) = header.as_ref() {
        let note = ctx.format_retained(
            format_args!(
                "description {:?}, binary schema {}",
                header.description,
                header.schema.value()
            ),
            "retain Parasolid stream note",
        )?;
        ctx.push_vec(&mut notes, note, "collect Parasolid notes")?;
    }
    Ok(Analysis {
        bytes,
        transmit,
        header,
        schema,
        layers,
        losses,
        notes,
    })
}

/// The message of the text-encoding refusal.
const TEXT_MESSAGE: &str = "parasolid.text-transmit-unsupported: the header declares FORMAT=text; \
     the text encoding of the transmit data is not read";

/// Report the structure of the file without building the model.
pub(crate) fn inspect(
    ctx: &DecodeContext<'_>,
    root: View<'_>,
) -> Result<ContainerSummary, CodecError> {
    let analysis = analyze(ctx, root.window())?;
    let entry = ContainerEntry {
        name: ctx.copy_retained_text(ENTRY_NAME, "retain Parasolid entry name")?,
        role: ContainerRole::ParasolidStream,
        storage: EntryStorage::verbatim(
            VerbatimLabel::Stored,
            u64_from_index(analysis.bytes.len()),
        ),
        attributes: std::collections::BTreeMap::new(),
    };
    Ok(ContainerSummary::classified(
        analysis.layers,
        ContainerKind::Stream,
        vec![entry],
        analysis.losses,
        analysis.notes,
    ))
}

/// Decode the Parasolid transmit data into the neutral B-rep.
pub(crate) fn decode(ctx: &DecodeContext<'_>, root: View<'_>) -> Result<Decoded, CodecError> {
    let analysis = analyze(ctx, root.window())?;
    if analysis.transmit.text_encoded {
        return Err(CodecError::UnsupportedDialect {
            dialects: Box::new(analysis.layers),
            message: TEXT_MESSAGE.into(),
        });
    }
    let Analysis {
        bytes,
        transmit,
        header,
        schema,
        layers,
        mut losses,
        mut notes,
    } = analysis;

    let mut attributes = std::collections::BTreeMap::new();
    attributes.insert(
        cadmpeg_core::nonblank_literal!("parasolid_schema"),
        ctx.copy_retained_text(schema.value(), "retain Parasolid schema")?,
    );
    if let Some(header) = header.as_ref() {
        attributes.insert(
            cadmpeg_core::nonblank_literal!("parasolid_description"),
            ctx.copy_retained_text(&header.description, "retain Parasolid description")?,
        );
    }
    let mut ir = CadIr::decoded(SourceMeta::classified(layers, attributes));

    ctx.charge_entities(1, "admit Parasolid transmit stream")?;
    let id = UnknownId::compose(
        &cadmpeg_ir::identity_namespace!("parasolid", "file", "transmit"),
        0,
    );
    let source = UnknownId::mint(ctx.copy_retained_text(id.as_str(), "retain Parasolid identity")?)
        .map_err(CodecError::malformed)?;
    let retained = ctx.copy_retained(bytes, "retain Parasolid transmit file")?;
    let mut unknowns = Vec::new();
    ctx.push_vec(
        &mut unknowns,
        UnknownRecord::retained(id, 0, retained, Vec::new()),
        "collect Parasolid retained stream",
    )?;

    let mut annotations = cadmpeg_ir::Annotations::default();
    let mut admitted = 0_u64;
    if header.is_some() && !ctx.container_only() {
        let binary = bytes.get(transmit.binary_offset..).unwrap_or(bytes);
        let origin = cadmpeg_ir::stream_name!("parasolid:transmit");
        // A standalone transmit file has no diagnostics switch of its own.
        let hosted = decode_hosted_brep(ctx, &mut ir.model, &[binary], &origin, &source, false)?;
        ctx.admit_entities(hosted.entities, &mut admitted, "admit Parasolid entities")?;
        annotations = hosted.annotations;
        ctx.extend_vec(&mut losses, hosted.losses, "collect Parasolid losses")?;
        ctx.extend_vec(&mut unknowns, hosted.unknowns, "collect Parasolid unknowns")?;
        let note = ctx.format_retained(
            format_args!("{} entities decoded", hosted.entities),
            "retain Parasolid entity note",
        )?;
        ctx.push_vec(&mut notes, note, "collect Parasolid notes")?;
        if hosted.face_color_records > 0 {
            let message = ctx.format_retained(
                format_args!(
                    "{} face color record(s) in the transmit data are not transferred",
                    hosted.face_color_records
                ),
                "retain Parasolid face color loss",
            )?;
            ctx.push_vec(
                &mut losses,
                ParasolidLossCode::AppearanceFaceColorUntransferred.note(message),
                "collect Parasolid losses",
            )?;
        }
    }
    let geometry_transferred =
        !(ir.model.surfaces.is_empty() && ir.model.points.is_empty() && ir.model.faces.is_empty());

    let transfer = if ctx.container_only() {
        cadmpeg_ir::report::decode::DecodeTransfer::ContainerOnly {}
    } else {
        if !geometry_transferred {
            ctx.push_vec(
                &mut losses,
                ParasolidLossCode::GeometryNotTransferred.note(
                    "no B-rep geometry was transferred; the other losses of this report state why",
                ),
                "collect Parasolid losses",
            )?;
        }
        cadmpeg_ir::report::decode::DecodeTransfer::full(geometry_transferred)
    };
    let mut source_fidelity = SourceFidelity::with_annotations(annotations);
    source_fidelity.attach_native_unknown_records(
        &mut ir,
        cadmpeg_parasolid::FORMAT,
        unknowns,
        ctx,
    )?;
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
