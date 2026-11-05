// SPDX-License-Identifier: Apache-2.0
//! Stable loss vocabulary for standalone Parasolid transmit decoding.
//!
//! Every fallback, approximation, and drop the decoder reports carries a
//! stable machine-readable code from [`ParasolidLossCode`]. Codes are the
//! gating surface: harness oracles and downstream tooling key on them, never
//! on the human-readable message text, so a reworded message is not a contract
//! change and a new drop path without a code does not compile.
//!
//! [`ParasolidLossCode::note`] is the single construction path for a
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

const NAMESPACE: LossNamespace<'static> = match LossNamespace::new("parasolid") {
    Ok(namespace) => namespace,
    Err(_) => panic!("reserved codec namespace"),
};

/// A stable, machine-readable identifier for one transmit-file transfer loss.
///
/// The string form (via [`ParasolidLossCode::code`]) is the stable contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ParasolidLossCode {
    /// The header declares a text encoding of the transmit data, which the
    /// decoder does not read.
    TextTransmitUnsupported,
    /// The binary part of the file has no readable stream header.
    HeaderUnreadable,
    /// The transmit data decoded no surfaces, points, or faces.
    GeometryNotTransferred,
    /// The transmit data carries face color records the model does not take.
    AppearanceFaceColorUntransferred,
    /// The embedded Parasolid schema has no declared grammar and was recovered
    /// as residual.
    KernelDialectUnverified,
}

impl ParasolidLossCode {
    /// Every code, in declaration order.
    #[cfg(test)]
    const ALL: &'static [ParasolidLossCode] = &[
        Self::TextTransmitUnsupported,
        Self::HeaderUnreadable,
        Self::GeometryNotTransferred,
        Self::AppearanceFaceColorUntransferred,
        Self::KernelDialectUnverified,
    ];

    /// The stable string identifier. This is the gating contract.
    #[must_use]
    const fn code(self) -> &'static str {
        match self {
            Self::TextTransmitUnsupported => "parasolid.text-transmit-unsupported",
            Self::HeaderUnreadable => "parasolid.header-unreadable",
            Self::GeometryNotTransferred => "geometry.not-transferred",
            Self::AppearanceFaceColorUntransferred => "appearance.face-color-untransferred",
            Self::KernelDialectUnverified => "source.kernel-dialect-unverified",
        }
    }

    /// The severity of this loss.
    #[must_use]
    const fn severity(self) -> Severity {
        match self {
            Self::GeometryNotTransferred => Severity::Blocking,
            Self::TextTransmitUnsupported | Self::HeaderUnreadable => Severity::Error,
            Self::AppearanceFaceColorUntransferred | Self::KernelDialectUnverified => {
                Severity::Warning
            }
        }
    }

    const fn shared_taxonomy(self) -> LossTaxonomy {
        match self {
            Self::TextTransmitUnsupported
            | Self::HeaderUnreadable
            | Self::GeometryNotTransferred => LossTaxonomy::GeometryNotTransferred,
            Self::AppearanceFaceColorUntransferred => LossTaxonomy::MaterialNotTransferred,
            Self::KernelDialectUnverified => LossTaxonomy::SourceDialectUnverified,
        }
    }

    /// Namespaced [`LossKind`] for this local code, classified by taxonomy.
    #[must_use]
    pub(crate) fn kind(self) -> LossKind {
        LossKind::namespaced(NAMESPACE, self.code(), self.shared_taxonomy())
    }

    /// Build a [`LossNote`] for this code with the given per-instance message.
    ///
    /// The structured code is `parasolid/<local>`. Severity comes from the
    /// local code; the strict floor comes from the taxonomy.
    #[must_use]
    pub(crate) fn note(self, message: impl Into<String>) -> LossNote {
        LossNote::new(self.kind(), message).with_severity(self.severity())
    }
}

#[cfg(test)]
mod tests {
    use super::ParasolidLossCode;
    use std::collections::BTreeSet;

    /// Value-level golden: the stable string form of every code, pinned.
    #[test]
    fn code_strings_are_pinned() {
        let codes: Vec<&str> = ParasolidLossCode::ALL.iter().map(|c| c.code()).collect();
        assert_eq!(
            codes,
            [
                "parasolid.text-transmit-unsupported",
                "parasolid.header-unreadable",
                "geometry.not-transferred",
                "appearance.face-color-untransferred",
                "source.kernel-dialect-unverified",
            ]
        );
    }

    /// Codes are unique and use the stable `family.detail` kebab shape.
    #[test]
    fn codes_are_unique_and_well_formed() {
        let mut seen = BTreeSet::new();
        for code in ParasolidLossCode::ALL {
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
        for code in ParasolidLossCode::ALL {
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
            .split_once("pub(crate) enum ParasolidLossCode {")
            .unwrap()
            .1;
        let body = declaration.split_once("\n}").unwrap().0;
        let variant_count = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with("//") && line.ends_with(','))
            .count();
        assert_eq!(ParasolidLossCode::ALL.len(), variant_count);
    }
}
