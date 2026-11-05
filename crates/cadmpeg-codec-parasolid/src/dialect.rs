// SPDX-License-Identifier: Apache-2.0
//! Dialect identity of a standalone Parasolid transmit file.
//!
//! A transmit file has no host container. Its identity is the kernel layer
//! that [`cadmpeg_parasolid`] classifies from the schema token: the
//! `parasolid:` row that the token names, or the residual `parasolid:unknown`
//! row. The layer is the only one, so it is the primary layer. It records the
//! complete schema token, the carrier `transmit`, and the encoding the header
//! declares, `binary` or `text`.
//!
//! This codec verifies no schema grammar of its own, so the layer is always
//! residual or unverified and charges `source.kernel-dialect-unverified`.
//!
//! The schema token comes from the `SCH=` line of the text header, or from the
//! binary stream header when the text header states none.

use cadmpeg_core::decode::DecodeContext;
use cadmpeg_core::dialect::{DialectLayers, LayerInstance};
use cadmpeg_core::CodecError;
use cadmpeg_ir::report::loss::LossNote;
use cadmpeg_parasolid::{Carrier, OwnedSchemaToken};

use crate::loss::ParasolidLossCode;

/// Location of the stream in the declarations: the file is the stream.
const CARRIER: &str = "transmit";

/// Key of the data encoding in [`cadmpeg_core::dialect::DialectMatch::declared`].
const DECLARED_ENCODING: &str = "encoding";

/// The encoding that the text header declares for the transmit data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    /// Binary transmit data behind the text header.
    Binary,
    /// Text transmit data behind the text header (`FORMAT=text`).
    Text,
}

impl Encoding {
    /// The declaration value.
    const fn label(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Text => "text",
        }
    }
}

/// Classify the kernel layer of the file and charge the loss that the
/// classification states.
pub(crate) fn classify_layers(
    ctx: &DecodeContext<'_>,
    schema: OwnedSchemaToken,
    encoding: Encoding,
) -> Result<(DialectLayers, Vec<LossNote>), CodecError> {
    let carrier = Carrier::new(ctx.copy_retained_text(CARRIER, "retain Parasolid carrier")?);
    let layer = cadmpeg_parasolid::classify_layer(ctx, schema, carrier, LayerInstance::Sole, &[])?;
    let matched = layer.into_matched().with_declared_entry(
        ctx,
        cadmpeg_core::nonblank_const!(DECLARED_ENCODING),
        encoding.label(),
        "retain Parasolid encoding declaration",
    )?;
    let mut losses = Vec::new();
    if let Some(message) = cadmpeg_parasolid::unverified_message(ctx, &matched)? {
        ctx.push_vec(
            &mut losses,
            ParasolidLossCode::KernelDialectUnverified.note(message),
            "collect Parasolid kernel dialect losses",
        )?;
    }
    Ok((DialectLayers::of(matched), losses))
}

#[cfg(test)]
mod tests;
