// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(test, allow(clippy::unwrap_used))]
//! Read the Parasolid B-rep of Solid Edge `.par`, `.psm`, and `.asm` files.
//!
//! A Solid Edge document is a Compound File Binary container. Its B-rep is a
//! Parasolid partition stream and a deltas stream in the `PARASOLID` storage,
//! for example `STREAM0.P_B` and `STREAM0.D_B`, or a standalone transmit stream
//! `<name>.X_B`. Each stream starts with the text transmit header of a
//! Parasolid transmit file. [`SolidEdgeCodec`] locates every body group by
//! directory path, removes the text header, hands the binary part to the
//! Parasolid B-rep decoder shared with the `SolidWorks` codec, and writes the
//! bodies, faces, edges, vertices, and carriers into
//! [`cadmpeg_ir::document::CadIr`]. Content selects the codec, never the file
//! extension.
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
//! Only the B-rep transfers. Feature history, document properties, materials,
//! and assembly structure are not read; the report does not itemize them. A
//! sketch stream gets a typed loss. A Compound File whose directory cannot be
//! read is a typed result that names the failing check. A document
//! without a `PARASOLID` storage, a family-of-parts master, a document whose
//! storage holds only sketch streams, or one whose body decodes no geometry
//! yields a container-only result with a machine-readable loss and no model. Parasolid lengths are metres; decoded
//! coordinates are millimetres.
//!
//! The Parasolid decode is shared with the `SolidWorks` codec. Entity identities
//! of the transferred B-rep keep the `sldprt:brep` namespaces of that decoder,
//! and its losses keep the `sldprt` loss namespace.

mod container;
mod decode;
mod dialect;
mod loss;
mod orientation;
mod regions;
mod transmit;

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
        Ok(match container::looks_like_solidedge(ctx, prefix)? {
            container::Evidence::Marker => Confidence::High,
            container::Evidence::Unreadable => Confidence::Low,
            container::Evidence::None => Confidence::No,
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
