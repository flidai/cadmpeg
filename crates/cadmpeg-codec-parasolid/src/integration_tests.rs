// SPDX-License-Identifier: Apache-2.0
//! Codec-wide detection, inspection, and decode over synthetic transmit files.

use std::io::Cursor;

use cadmpeg_core::container::ContainerRole;
use cadmpeg_core::decode::InspectOptions;
use cadmpeg_core::CodecError;
use cadmpeg_ir::codec::{Codec, Confidence, DecodeFailure, DecodeOptions, DecodeResult};
use cadmpeg_ir::report::Severity;

use crate::loss::ParasolidLossCode;
use crate::test_support::parasolid::text_header;
use crate::test_support::{
    binary_transmit, transmit_file, transmit_file_of, triangle_body, with_decode_context,
};
use crate::ParasolidCodec;

fn decode_with(bytes: &[u8], options: &DecodeOptions) -> DecodeResult {
    ParasolidCodec
        .decode(&mut Cursor::new(bytes.to_vec()), options)
        .unwrap()
}

fn decode(bytes: &[u8]) -> DecodeResult {
    decode_with(bytes, &DecodeOptions::default())
}

fn has_loss(result: &DecodeResult, code: ParasolidLossCode) -> bool {
    result
        .report()
        .losses
        .iter()
        .any(|loss| loss.code == code.kind())
}

#[test]
fn detects_a_transmit_header_by_content() {
    let bytes = transmit_file(&triangle_body());
    let found = with_decode_context(&bytes, |ctx| {
        ParasolidCodec
            .detect(ctx, cadmpeg_core::decode::View::over_retained(&bytes))
            .unwrap()
    });
    assert_eq!(found, Confidence::High);
    let other = b"ISO-10303-21;\nHEADER;".to_vec();
    let found = with_decode_context(&other, |ctx| {
        ParasolidCodec
            .detect(ctx, cadmpeg_core::decode::View::over_retained(&other))
            .unwrap()
    });
    assert_eq!(found, Confidence::No);
}

#[test]
fn decodes_the_transmit_data_into_a_connected_body() {
    let result = decode(&transmit_file(&triangle_body()));
    let model = &result.ir().model;
    assert!(result.report().geometry_transferred());
    assert_eq!(model.bodies.len(), 1);
    assert_eq!(model.faces.len(), 1);
    assert_eq!(model.loops.len(), 1);
    assert_eq!(model.coedges.len(), 3);
    assert_eq!(model.edges.len(), 3);
    assert_eq!(model.vertices.len(), 3);
    assert_eq!(model.curves.len(), 3);
    assert_eq!(model.surfaces.len(), 1);
    // Parasolid metres become millimetres: the 0.1 m side spans 100 mm.
    let extent = model
        .points
        .iter()
        .map(|point| point.position().get().x)
        .fold(0.0_f64, f64::max);
    assert!((extent - 100.0).abs() < 1e-9, "{extent}");
}

#[test]
fn reports_the_kernel_row_of_the_schema_as_the_only_layer() {
    let result = decode(&transmit_file(&triangle_body()));
    let layers = result
        .ir()
        .source
        .as_ref()
        .unwrap()
        .dialects()
        .unwrap()
        .iter()
        .map(|layer| layer.dialect().as_str().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(layers, ["parasolid:unknown"]);
    assert!(has_loss(
        &result,
        ParasolidLossCode::KernelDialectUnverified
    ));
}

#[test]
fn retains_the_file_byte_exact() {
    let file = transmit_file(&triangle_body());
    let editable = cadmpeg_test_support::EditableDecodeResult::from(decode(&file));
    let retained: Vec<&[u8]> = editable
        .source_fidelity()
        .retained_records()
        .iter()
        .filter(|(id, _)| id.as_str().starts_with("parasolid:file:transmit#"))
        .map(|(_, record)| record.data().unwrap())
        .collect();
    assert_eq!(retained, [file.as_slice()]);
}

#[test]
fn the_decoded_model_passes_neutral_validation() {
    let result = decode(&transmit_file(&triangle_body()));
    let report = cadmpeg_ir::validate_neutral(result.ir(), Vec::new()).unwrap();
    assert!(report.is_ok(), "{report:?}");
}

#[test]
fn container_only_decode_skips_the_model() {
    let options = DecodeOptions {
        container_only: true,
        ..DecodeOptions::default()
    };
    let result = decode_with(&transmit_file(&triangle_body()), &options);
    assert!(result.ir().model.faces.is_empty());
    assert!(!has_loss(
        &result,
        ParasolidLossCode::GeometryNotTransferred
    ));
}

#[test]
fn the_text_header_schema_classifies_the_layer() {
    for (schema, row) in [
        ("SCH_1400184_14000", "parasolid:unknown"),
        ("SCH_3201255_32001", "parasolid:unknown"),
        ("SCH_SW_33103_11000", "parasolid:sch-sw-33103"),
    ] {
        let result = decode(&transmit_file_of(schema, &triangle_body()));
        assert!(result.report().geometry_transferred(), "{schema}");
        let source = result.ir().source.as_ref().unwrap();
        assert_eq!(
            source
                .attributes
                .get("parasolid_schema")
                .map(String::as_str),
            Some(schema)
        );
        assert_eq!(source.dialects().unwrap().primary().dialect().as_str(), row);
    }
}

#[test]
fn inspect_reports_the_stream_entry_and_the_layer() {
    let bytes = transmit_file(&triangle_body());
    let summary = ParasolidCodec
        .inspect(&mut Cursor::new(bytes.clone()), &InspectOptions::default())
        .unwrap();
    assert_eq!(summary.format(), "parasolid");
    assert_eq!(summary.entries.len(), 1);
    assert_eq!(summary.entries[0].role, ContainerRole::ParasolidStream);
    assert!(summary
        .losses
        .iter()
        .all(|loss| loss.code == ParasolidLossCode::KernelDialectUnverified.kind()));
    assert!(summary.notes[0].contains("binary schema SCH_3201255_32001"));
}

#[test]
fn a_text_encoded_transmit_is_a_typed_refusal_with_the_layers_inspect_reports() {
    let mut file = text_header("SCH_1100129_11004", "text");
    file.extend(b"\nT51 : TRANSMIT FILE created by modeller version 110012917\n 1 2993 0 0\n");

    let summary = ParasolidCodec
        .inspect(&mut Cursor::new(file.clone()), &InspectOptions::default())
        .unwrap();
    let inspected = summary.dialects().unwrap().primary().clone();
    assert_eq!(inspected.dialect().as_str(), "parasolid:unknown");
    assert_eq!(inspected.declared()["encoding"], "text");
    assert!(summary.losses.iter().any(|loss| loss.code
        == ParasolidLossCode::TextTransmitUnsupported.kind()
        && loss.severity == Severity::Error));

    let error = ParasolidCodec
        .decode(&mut Cursor::new(file), &DecodeOptions::default())
        .unwrap_err();
    let DecodeFailure::Codec(CodecError::UnsupportedDialect { dialects, message }) = error else {
        panic!("expected an identified dialect refusal, got {error:?}");
    };
    assert_eq!(dialects.primary(), &inspected);
    assert!(message.starts_with("parasolid.text-transmit-unsupported"));
}

#[test]
fn a_text_header_without_binary_data_is_not_decoded() {
    let text = text_header("SCH_3201255_32001", "binary");
    let result = decode(&text);
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(&result, ParasolidLossCode::HeaderUnreadable));
    assert!(has_loss(&result, ParasolidLossCode::GeometryNotTransferred));
    let source = result.ir().source.as_ref().unwrap();
    assert_eq!(
        source
            .attributes
            .get("parasolid_schema")
            .map(String::as_str),
        Some("SCH_3201255_32001")
    );
}

#[test]
fn a_header_without_a_schema_is_the_wrong_format() {
    let header = String::from_utf8(text_header("SCH_3201255_32001", "binary")).unwrap();
    let bytes = header.replace("SCH=SCH_3201255_32001", "SCH=").into_bytes();
    let error = ParasolidCodec
        .decode(&mut Cursor::new(bytes), &DecodeOptions::default())
        .unwrap_err();
    assert!(matches!(
        error,
        DecodeFailure::Codec(CodecError::WrongFormat(_))
    ));
}

#[test]
fn input_without_a_transmit_header_is_the_wrong_format() {
    for bytes in [binary_transmit(&triangle_body()), b"ISO-10303-21;".to_vec()] {
        let error = ParasolidCodec
            .decode(&mut Cursor::new(bytes), &DecodeOptions::default())
            .unwrap_err();
        assert!(matches!(
            error,
            DecodeFailure::Codec(CodecError::WrongFormat(_))
        ));
    }
}
