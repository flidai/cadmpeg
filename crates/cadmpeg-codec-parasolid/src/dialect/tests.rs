// SPDX-License-Identifier: Apache-2.0
//! Dialect classification tests.

use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy};
use cadmpeg_core::dialect::Admission;
use cadmpeg_parasolid::OwnedSchemaToken;

use super::{classify_layers, Encoding};
use crate::loss::ParasolidLossCode;

fn with_ctx<T>(f: impl FnOnce(&DecodeContext<'_>) -> T) -> T {
    let arena = DecodeArena::new();
    let (ctx, _) = DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::service())
        .expect("empty root fits service policy");
    f(&ctx)
}

fn token(text: &str) -> OwnedSchemaToken {
    OwnedSchemaToken::try_from(text).expect("schema token")
}

#[test]
fn the_named_schema_row_is_the_only_layer_and_declares_encoding_and_carrier() {
    with_ctx(|ctx| {
        let (layers, losses) =
            classify_layers(ctx, token("SCH_SW_33103_11000"), Encoding::Binary).expect("layers");
        let primary = layers.primary();
        assert_eq!(primary.dialect().as_str(), "parasolid:sch-sw-33103");
        assert_eq!(layers.iter().count(), 1);
        assert_eq!(
            primary.declared().get("schema").map(String::as_str),
            Some("SCH_SW_33103_11000")
        );
        assert_eq!(
            primary.declared().get("carrier").map(String::as_str),
            Some("transmit")
        );
        assert_eq!(
            primary.declared().get("encoding").map(String::as_str),
            Some("binary")
        );
        assert_eq!(losses.len(), 1);
        assert_eq!(
            losses[0].code,
            ParasolidLossCode::KernelDialectUnverified.kind()
        );
    });
}

#[test]
fn an_unnamed_schema_is_residual_and_a_text_header_declares_text() {
    with_ctx(|ctx| {
        let (layers, losses) =
            classify_layers(ctx, token("SCH_1100129_11004"), Encoding::Text).expect("layers");
        let primary = layers.primary();
        assert_eq!(primary.dialect().as_str(), "parasolid:unknown");
        assert!(matches!(primary.admission(), Admission::Residual));
        assert_eq!(
            primary.declared().get("encoding").map(String::as_str),
            Some("text")
        );
        assert_eq!(losses.len(), 1);
    });
}

#[test]
fn a_format_13006_schema_names_its_row() {
    with_ctx(|ctx| {
        let (layers, _) = classify_layers(ctx, token("SCH_3201255_32001_13006"), Encoding::Binary)
            .expect("layers");
        assert_eq!(
            layers.primary().dialect().as_str(),
            "parasolid:format-13006"
        );
    });
}
