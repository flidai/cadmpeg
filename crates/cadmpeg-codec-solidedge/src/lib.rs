// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Read the Parasolid B-rep of Solid Edge `.par`, `.psm`, and `.asm` files.
//!
//! A Solid Edge document is a Compound File Binary container. Its B-rep is a
//! Parasolid partition stream and a deltas stream in the `PARASOLID` storage:
//! `STREAM0.P_B` and `STREAM0.D_B`. [`SolidEdgeCodec`] locates that pair by
//! directory path, hands it to the Parasolid B-rep decoder shared with the
//! `SolidWorks` codec, and writes the bodies, faces, edges, vertices, and
//! carriers into [`cadmpeg_ir::document::CadIr`]. Content selects the codec,
//! never the file extension.
//!
//! <!-- generated: capability solidedge -->
//! Support: L1 ([ladder](https://github.com/cadmpeg/cadmpeg/blob/main/docs/format-support.md#siemens-solid-edge-par-psm-and-asm)).
//! <!-- /generated: capability solidedge -->
//!
//! # Decode a part
//!
//! ```no_run
//! use std::fs::File;
//!
//! use cadmpeg_codec_solidedge::SolidEdgeCodec;
//! use cadmpeg_ir::{Codec, DecodeOptions};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut input = File::open("part.par")?;
//! let decoded = SolidEdgeCodec.decode(&mut input, &DecodeOptions::default())?;
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
//! Only the B-rep transfers. Feature history, sketches, document properties,
//! materials, and assembly structure are not read; the report does not itemize
//! them. A document without a `PARASOLID` storage, without a partition stream,
//! or whose pair decodes no geometry yields a container-only result with a
//! machine-readable loss and no model. Parasolid lengths are metres; decoded
//! coordinates are millimetres.
//!
//! The Parasolid decode is shared with the `SolidWorks` codec. Entity identities
//! of the transferred B-rep keep the `sldprt:brep` namespaces of that decoder,
//! and its losses keep the `sldprt` loss namespace.

mod container;
mod decode;
mod dialect;
mod loss;

use cadmpeg_core::decode::{DecodeContext, View};
use cadmpeg_core::CodecError;
use cadmpeg_ir::codec::{CodecBackend, Confidence, Decoded, FormatId};
use cadmpeg_ir::ContainerSummary;

/// Decoder and inspector for Solid Edge `.par`, `.psm`, and `.asm` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct SolidEdgeCodec;

impl CodecBackend for SolidEdgeCodec {
    const FORMAT: FormatId = FormatId::new(dialect::FORMAT);

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
        if container::looks_like_solidedge(ctx, prefix)? {
            Ok(Confidence::High)
        } else {
            Ok(Confidence::No)
        }
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
