// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Read the Parasolid B-rep of standalone `.x_b` transmit files.
//!
//! A Parasolid transmit file starts with a text header whose lines begin with
//! `**`. The header states the schema token and the encoding of the transmit
//! data that follows. [`ParasolidCodec`] reads the header, removes it, hands the
//! binary data to the Parasolid B-rep decoder shared with the `SolidWorks`
//! codec, and writes the bodies, faces, edges, vertices, and carriers into
//! [`cadmpeg_ir::document::CadIr`]. Content selects the codec, never the file
//! extension.
//!
//! A header that declares `FORMAT=text` (a `.x_t` file) is identified and
//! refused: inspection reports the `parasolid:transmit-text` row and the loss
//! `parasolid.text-transmit-unsupported`, and decode returns
//! [`cadmpeg_core::CodecError::UnsupportedDialect`] with the same layers.
//!
//! <!-- generated: capability parasolid -->
//! Support: L1 ([ladder](https://github.com/cadmpeg/cadmpeg/blob/main/docs/format-support.md#parasolid-transmit-xb-and-xt-files)).
//! <!-- /generated: capability parasolid -->
//!
//! # Decode a part
//!
//! ```no_run
//! use std::fs::File;
//!
//! use cadmpeg_codec_parasolid::ParasolidCodec;
//! use cadmpeg_ir::{Codec, DecodeOptions};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut input = File::open("part.x_b")?;
//! let decoded = ParasolidCodec.decode(&mut input, &DecodeOptions::default())?;
//! println!("{} faces", decoded.ir().model.faces.len());
//! for loss in &decoded.report().losses {
//!     eprintln!("{:?}: {}", loss.severity, loss.message);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Model and loss boundaries
//!
//! Only the B-rep transfers. Parasolid lengths are metres; decoded coordinates
//! are millimetres. A file whose binary data has no readable stream header, or
//! whose body decodes no geometry, yields a result with a machine-readable loss
//! and no model.
//!
//! The Parasolid decode is shared with the `SolidWorks` codec. Entity identities
//! of the transferred B-rep keep the `sldprt:brep` namespaces of that decoder,
//! and its losses keep the `sldprt` loss namespace.

mod decode;
mod dialect;
mod loss;
mod transmit;

use cadmpeg_core::decode::{DecodeContext, View};
use cadmpeg_core::CodecError;
use cadmpeg_ir::codec::{CodecBackend, Confidence, Decoded, FormatId};
use cadmpeg_ir::ContainerSummary;

/// Decoder and inspector for standalone Parasolid transmit files.
#[derive(Debug, Default, Clone, Copy)]
pub struct ParasolidCodec;

impl CodecBackend for ParasolidCodec {
    const FORMAT: FormatId = FormatId::new(cadmpeg_parasolid::FORMAT);

    fn detect_impl(
        &self,
        ctx: &DecodeContext<'_>,
        prefix: View<'_>,
    ) -> Result<Confidence, CodecError> {
        let prefix = prefix.window();
        ctx.charge_work(
            cadmpeg_core::decode::u64_from_index(prefix.len()),
            "detect input",
        )?;
        Ok(if transmit::is_transmit_header(prefix) {
            Confidence::High
        } else {
            Confidence::No
        })
    }

    fn inspect_impl(
        &self,
        ctx: &DecodeContext<'_>,
        root: View<'_>,
    ) -> Result<ContainerSummary, CodecError> {
        decode::inspect(ctx, root)
    }

    fn decode_impl(&self, ctx: &DecodeContext<'_>, root: View<'_>) -> Result<Decoded, CodecError> {
        decode::decode(ctx, root)
    }
}

#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod test_support;
