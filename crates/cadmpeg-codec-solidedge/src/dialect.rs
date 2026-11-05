// SPDX-License-Identifier: Apache-2.0
//! Solid Edge dialect identity: which registry row a document is, and how it
//! was admitted.
//!
//! The enum is internal, registry-generated [`DialectId`] constants are the
//! boundary, [`classify_layers`] is the one construction path, and the
//! vocabulary is closed.
//!
//! # Host classification is structural
//!
//! A Compound File with a `PARASOLID` storage is `solidedge:cfb-parasolid` and
//! is admitted: the storage is the grammar the decoder reads. A Compound File
//! without that storage reaches the codec only through a forced decode. It
//! takes the residual `solidedge:unknown` row, decodes to a container-only
//! document, and charges `source.dialect-unverified`. The writing application
//! version is not read, so no row branches on it.
//!
//! # Kernel layers
//!
//! Each Parasolid stream with a readable transmit header adds one
//! `parasolid:` layer through [`cadmpeg_parasolid`], tagged by the stream path
//! when more than one stream exists. This codec verifies no schema grammar of
//! its own, so every kernel layer is residual and charges
//! `source.kernel-dialect-unverified`.

use cadmpeg_core::decode::{u64_from_index, DecodeContext};
use cadmpeg_core::dialect::{DialectId, DialectLayers, DialectMatch};
use cadmpeg_core::CodecError;
use cadmpeg_ir::report::loss::LossNote;
use cadmpeg_parasolid::{Carrier, OwnedSchemaToken};

use crate::loss::SolidEdgeLossCode;

include!("dialect/registry_ids.rs");

/// Key of the storage name in [`DialectMatch::declared`].
const DECLARED_STORAGE: &str = "storage";

/// One reportable row of `crates/cadmpeg-registry/docs/dialects.toml` under the
/// `solidedge` namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SolidEdgeDialect {
    /// A Compound File with a `PARASOLID` storage.
    CfbParasolid,
    /// A Compound File without a `PARASOLID` storage.
    Unknown,
}

/// The host and kernel identities of one scan, plus recoverable damage to the
/// unique layer set.
pub(crate) struct LayerClassification {
    layers: DialectLayers,
    losses: Vec<LossNote>,
}

impl LayerClassification {
    /// The classified layers and the losses their classification charged.
    pub(crate) fn into_parts(self) -> (DialectLayers, Vec<LossNote>) {
        (self.layers, self.losses)
    }
}

/// Classify the host document and every Parasolid stream with a readable
/// header. `storage` is the `PARASOLID` storage path as the directory spells it.
pub(crate) fn classify_layers(
    ctx: &DecodeContext<'_>,
    storage: Option<&str>,
    kernels: Vec<(OwnedSchemaToken, Carrier)>,
) -> Result<LayerClassification, CodecError> {
    let host = if storage.is_some() {
        SolidEdgeDialect::CfbParasolid
    } else {
        SolidEdgeDialect::Unknown
    };
    let mut layers = DialectLayers::of(host.matched(ctx, storage)?);
    let extra = cadmpeg_parasolid::extra_layers(ctx, kernels, &[])?;
    let mut losses = Vec::new();
    for message in cadmpeg_parasolid::push_extras(ctx, &mut layers, extra)? {
        ctx.push_vec(
            &mut losses,
            SolidEdgeLossCode::DialectLayerCollision.note(message),
            "collect Solid Edge dialect collision losses",
        )?;
    }
    ctx.charge_work(
        u64_from_index(layers.iter().size_hint().0),
        "scan Solid Edge dialect losses",
    )?;
    for layer in layers.iter() {
        if let Some(message) = cadmpeg_parasolid::unverified_message(ctx, layer)? {
            ctx.push_vec(
                &mut losses,
                SolidEdgeLossCode::KernelDialectUnverified.note(message),
                "collect Solid Edge kernel dialect losses",
            )?;
        } else if layer.dialect() == &SOLIDEDGE_UNKNOWN {
            let message = ctx.format_retained(
                format_args!(
                    "the Compound File has no readable PARASOLID storage, so no declared identity was \
                     verified. The directory is reported on the `{}` residual row and no \
                     geometry is read.",
                    layer.dialect()
                ),
                "retain Solid Edge source dialect loss",
            )?;
            ctx.push_vec(
                &mut losses,
                SolidEdgeLossCode::SourceDialectUnverified.note(message),
                "collect Solid Edge source dialect loss",
            )?;
        }
    }
    Ok(LayerClassification { layers, losses })
}

impl SolidEdgeDialect {
    /// Every reportable dialect identity this enum can name.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 2] = [Self::CfbParasolid, Self::Unknown];

    /// The registry-generated id for this variant.
    pub(crate) const fn id(self) -> DialectId {
        match self {
            Self::CfbParasolid => SOLIDEDGE_CFB_PARASOLID,
            Self::Unknown => SOLIDEDGE_UNKNOWN,
        }
    }

    /// Build the wire identity for this row and its source declaration.
    fn matched(
        self,
        ctx: &DecodeContext<'_>,
        storage: Option<&str>,
    ) -> Result<DialectMatch, CodecError> {
        let mut declared = std::collections::BTreeMap::new();
        if let Some(storage) = storage {
            let value = ctx.copy_retained_text(storage, "retain Solid Edge storage name")?;
            ctx.insert_btree_map(
                &mut declared,
                cadmpeg_core::nonblank_const!(DECLARED_STORAGE),
                value,
                "index Solid Edge dialect declaration",
            )?;
        }
        Ok(match self {
            Self::CfbParasolid => DialectMatch::admitted(self.id()),
            Self::Unknown => DialectMatch::residual(self.id()),
        }
        .with_declared(declared))
    }
}

#[cfg(test)]
mod tests;
