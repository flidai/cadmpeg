// SPDX-License-Identifier: Apache-2.0
//! Dialect classification tests.

use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy};
use cadmpeg_core::dialect::Admission;
use cadmpeg_parasolid::{Carrier, OwnedSchemaToken};

use super::{classify_layers, SolidEdgeDialect};
use crate::loss::SolidEdgeLossCode;

fn with_ctx<T>(f: impl FnOnce(&DecodeContext<'_>) -> T) -> T {
    let arena = DecodeArena::new();
    let (ctx, _) = DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::service())
        .expect("empty root fits service policy");
    f(&ctx)
}

fn kernel(path: &str) -> (OwnedSchemaToken, Carrier) {
    (
        OwnedSchemaToken::try_from("SCH_1234567_12345_13006").expect("schema token"),
        Carrier::new(path.to_owned()),
    )
}

#[test]
fn every_row_has_a_distinct_registry_id() {
    let ids: Vec<_> = SolidEdgeDialect::ALL.iter().map(|row| row.id()).collect();
    assert_eq!(ids[0].as_str(), "solidedge:cfb-parasolid");
    assert_eq!(ids[1].as_str(), "solidedge:unknown");
}

#[test]
fn a_parasolid_storage_is_admitted_and_declares_its_name() {
    with_ctx(|ctx| {
        let (layers, losses) = classify_layers(ctx, Some("PARASOLID"), Vec::new())
            .expect("classification")
            .into_parts();
        let primary = layers.primary();
        assert_eq!(primary.dialect(), &SolidEdgeDialect::CfbParasolid.id());
        assert!(matches!(primary.admission(), Admission::Admitted));
        assert_eq!(
            primary.declared().get("storage").map(String::as_str),
            Some("PARASOLID")
        );
        assert!(losses.is_empty());
    });
}

#[test]
fn no_storage_is_residual_and_charges_the_source_dialect_loss() {
    with_ctx(|ctx| {
        let (layers, losses) = classify_layers(ctx, None, Vec::new())
            .expect("classification")
            .into_parts();
        let primary = layers.primary();
        assert_eq!(primary.dialect(), &SolidEdgeDialect::Unknown.id());
        assert!(matches!(primary.admission(), Admission::Residual));
        assert_eq!(losses.len(), 1);
        assert_eq!(
            losses[0].code,
            SolidEdgeLossCode::SourceDialectUnverified.kind()
        );
    });
}

#[test]
fn each_kernel_stream_adds_a_residual_layer_and_a_loss() {
    with_ctx(|ctx| {
        let kernels = vec![
            kernel("PARASOLID/STREAM0.P_B"),
            kernel("PARASOLID/STREAM0.D_B"),
        ];
        let (layers, losses) = classify_layers(ctx, Some("PARASOLID"), kernels)
            .expect("classification")
            .into_parts();
        assert_eq!(layers.iter().count(), 3);
        assert_eq!(losses.len(), 2);
        assert!(losses
            .iter()
            .all(|loss| loss.code == SolidEdgeLossCode::KernelDialectUnverified.kind()));
    });
}
