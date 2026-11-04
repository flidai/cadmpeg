// SPDX-License-Identifier: Apache-2.0
//! Stable loss vocabulary for Solid Edge decoding.
//!
//! Every fallback, approximation, and drop the decoder reports carries a
//! stable machine-readable code from [`SolidEdgeLossCode`]. Codes are the
//! gating surface: harness oracles and downstream tooling key on them, never
//! on the human-readable message text, so a reworded message is not a contract
//! change and a new drop path without a code does not compile.
//!
//! [`SolidEdgeLossCode::note`] is the single construction path for a
//! decode-time [`LossNote`] in this crate: it fixes the loss category and
//! severity from the code and leaves only the per-instance message to the
//! caller.
//!
//! The Parasolid B-rep decode is shared with the `SolidWorks` codec. Its losses
//! keep their own `sldprt` namespace and pass through unchanged.
use cadmpeg_ir::report::{
    loss::{LossKind, LossNamespace, LossNote, LossTaxonomy},
    Severity,
};

const NAMESPACE: LossNamespace<'static> = match LossNamespace::new("solidedge") {
    Ok(namespace) => namespace,
    Err(_) => panic!("reserved codec namespace"),
};

/// A stable, machine-readable identifier for one Solid Edge transfer loss.
///
/// Variants are grouped by the record family whose transfer degraded. The
/// string form (via [`SolidEdgeLossCode::code`]) is the stable contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SolidEdgeLossCode {
    /// The Compound File has no `PARASOLID` storage.
    ContainerNoParasolidStorage,
    /// The Compound File directory cannot be read.
    ContainerUnreadable,
    /// The Compound File is a family-of-parts master, which holds no model.
    ContainerNotModelDocument,
    /// The `PARASOLID` storage holds no stream that carries a body.
    ContainerNoBodyStream,
    /// The `PARASOLID` storage holds a deltas stream and no base state for it.
    ContainerNoPartitionStream,
    /// The decoded partition stream has no deltas stream.
    ContainerDeltasMissing,
    /// The `PARASOLID` storage holds an entry that decode does not read.
    ContainerStreamUntransferred,
    /// The `PARASOLID` storage holds a sketch stream, which is not a body.
    ContainerSketchUntransferred,
    /// The `PARASOLID` storage holds a second body group that decode does not read.
    ContainerBodyGroupUntransferred,
    /// A Parasolid stream has no readable transmit header.
    ParasolidHeaderUnreadable,
    /// The Parasolid streams decoded no surfaces, points, or faces.
    GeometryNotTransferred,
    /// The Parasolid streams carry face color records the model does not take.
    AppearanceFaceColorUntransferred,
    /// The document matched no declared Solid Edge row.
    SourceDialectUnverified,
    /// An embedded Parasolid schema has no declared grammar and was recovered as residual.
    KernelDialectUnverified,
    /// Two embedded kernel carriers resolved to one dialect-layer identity.
    DialectLayerCollision,
}

impl SolidEdgeLossCode {
    /// Every code, in declaration order.
    #[cfg(test)]
    const ALL: &'static [SolidEdgeLossCode] = &[
        Self::ContainerNoParasolidStorage,
        Self::ContainerUnreadable,
        Self::ContainerNotModelDocument,
        Self::ContainerNoBodyStream,
        Self::ContainerNoPartitionStream,
        Self::ContainerDeltasMissing,
        Self::ContainerStreamUntransferred,
        Self::ContainerSketchUntransferred,
        Self::ContainerBodyGroupUntransferred,
        Self::ParasolidHeaderUnreadable,
        Self::GeometryNotTransferred,
        Self::AppearanceFaceColorUntransferred,
        Self::SourceDialectUnverified,
        Self::KernelDialectUnverified,
        Self::DialectLayerCollision,
    ];

    /// The stable string identifier. This is the gating contract.
    #[must_use]
    const fn code(self) -> &'static str {
        match self {
            Self::ContainerNoParasolidStorage => "container.no-parasolid-storage",
            Self::ContainerUnreadable => "container.unreadable",
            Self::ContainerNotModelDocument => "container.not-a-model-document",
            Self::ContainerNoBodyStream => "container.no-body-stream",
            Self::ContainerNoPartitionStream => "container.no-partition-stream",
            Self::ContainerDeltasMissing => "container.deltas-missing",
            Self::ContainerStreamUntransferred => "container.stream-untransferred",
            Self::ContainerSketchUntransferred => "container.sketch-untransferred",
            Self::ContainerBodyGroupUntransferred => "container.body-group-untransferred",
            Self::ParasolidHeaderUnreadable => "parasolid.header-unreadable",
            Self::GeometryNotTransferred => "geometry.not-transferred",
            Self::AppearanceFaceColorUntransferred => "appearance.face-color-untransferred",
            Self::SourceDialectUnverified => "source.dialect-unverified",
            Self::KernelDialectUnverified => "source.kernel-dialect-unverified",
            Self::DialectLayerCollision => "source.dialect-layer-collision",
        }
    }

    /// The severity of this loss.
    #[must_use]
    const fn severity(self) -> Severity {
        match self {
            Self::GeometryNotTransferred => Severity::Blocking,
            Self::ContainerNoParasolidStorage
            | Self::ContainerUnreadable
            | Self::ContainerNotModelDocument
            | Self::ContainerNoBodyStream
            | Self::ContainerNoPartitionStream
            | Self::ParasolidHeaderUnreadable => Severity::Error,
            Self::ContainerDeltasMissing
            | Self::ContainerStreamUntransferred
            | Self::ContainerSketchUntransferred
            | Self::ContainerBodyGroupUntransferred
            | Self::AppearanceFaceColorUntransferred
            | Self::SourceDialectUnverified
            | Self::KernelDialectUnverified
            | Self::DialectLayerCollision => Severity::Warning,
        }
    }

    const fn shared_taxonomy(self) -> LossTaxonomy {
        match self {
            Self::ContainerNoParasolidStorage
            | Self::ContainerUnreadable
            | Self::ContainerNotModelDocument
            | Self::ContainerNoBodyStream
            | Self::ContainerNoPartitionStream => LossTaxonomy::MissingGeometryStream,
            Self::ContainerDeltasMissing | Self::DialectLayerCollision => {
                LossTaxonomy::DecodeDiagnostic
            }
            Self::ContainerStreamUntransferred
            | Self::ContainerSketchUntransferred
            | Self::ContainerBodyGroupUntransferred => LossTaxonomy::ObjectRecordsUntransferred,
            Self::ParasolidHeaderUnreadable | Self::GeometryNotTransferred => {
                LossTaxonomy::GeometryNotTransferred
            }
            Self::AppearanceFaceColorUntransferred => LossTaxonomy::MaterialNotTransferred,
            Self::SourceDialectUnverified | Self::KernelDialectUnverified => {
                LossTaxonomy::SourceDialectUnverified
            }
        }
    }

    /// Namespaced [`LossKind`] for this local code, classified by taxonomy.
    #[must_use]
    pub(crate) fn kind(self) -> LossKind {
        LossKind::namespaced(NAMESPACE, self.code(), self.shared_taxonomy())
    }

    /// Build a [`LossNote`] for this code with the given per-instance message.
    ///
    /// The structured code is `solidedge/<local>`. Severity comes from the
    /// local code; the strict floor comes from the taxonomy.
    #[must_use]
    pub(crate) fn note(self, message: impl Into<String>) -> LossNote {
        LossNote::new(self.kind(), message).with_severity(self.severity())
    }
}

#[cfg(test)]
mod tests {
    use super::SolidEdgeLossCode;
    use std::collections::BTreeSet;

    /// Value-level golden: the stable string form of every code, pinned.
    #[test]
    fn code_strings_are_pinned() {
        let codes: Vec<&str> = SolidEdgeLossCode::ALL.iter().map(|c| c.code()).collect();
        assert_eq!(
            codes,
            [
                "container.no-parasolid-storage",
                "container.unreadable",
                "container.not-a-model-document",
                "container.no-body-stream",
                "container.no-partition-stream",
                "container.deltas-missing",
                "container.stream-untransferred",
                "container.sketch-untransferred",
                "container.body-group-untransferred",
                "parasolid.header-unreadable",
                "geometry.not-transferred",
                "appearance.face-color-untransferred",
                "source.dialect-unverified",
                "source.kernel-dialect-unverified",
                "source.dialect-layer-collision",
            ]
        );
    }

    /// Codes are unique and use the stable `family.detail` kebab shape.
    #[test]
    fn codes_are_unique_and_well_formed() {
        let mut seen = BTreeSet::new();
        for code in SolidEdgeLossCode::ALL {
            let text = code.code();
            assert!(seen.insert(text), "duplicate code {text}");
            let (family, detail) = text.split_once('.').expect("family.detail shape");
            assert!(!family.is_empty() && !detail.is_empty());
            assert!(
                text.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'-'),
                "code {text} is not lowercase kebab"
            );
        }
    }

    /// The note builder fixes severity from the codec-specific code.
    #[test]
    fn note_takes_severity_from_the_code() {
        for code in SolidEdgeLossCode::ALL {
            let note = code.note("x");
            assert_eq!(note.severity, code.severity());
            assert_eq!(note.message, "x");
            assert!(note.provenance.is_none());
        }
    }

    /// The declared list names every variant of the enum.
    #[test]
    fn all_covers_every_declared_variant() {
        let source = include_str!("loss.rs");
        let declaration = source
            .split_once("pub(crate) enum SolidEdgeLossCode {")
            .unwrap()
            .1;
        let body = declaration.split_once("\n}").unwrap().0;
        let variant_count = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//") && line.ends_with(','))
            .count();
        assert_eq!(SolidEdgeLossCode::ALL.len(), variant_count);
    }
}
