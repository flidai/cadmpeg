// SPDX-License-Identifier: Apache-2.0
//! Codec-wide detection, inspection, and decode over synthetic documents.

use std::io::Cursor;

use cadmpeg_core::container::ContainerRole;
use cadmpeg_core::decode::InspectOptions;
use cadmpeg_ir::codec::{Codec, Confidence, DecodeFailure, DecodeOptions, DecodeResult};
use cadmpeg_ir::report::Severity;

use crate::loss::SolidEdgeLossCode;
use crate::test_support::cfb::{storage, stream};
use crate::test_support::{
    deltas_body, deltas_stream, document, moved_point_deltas_body, part_with_pair,
    partition_stream, triangle_body, with_decode_context,
};
use crate::SolidEdgeCodec;

fn decode_with(bytes: &[u8], options: &DecodeOptions) -> DecodeResult {
    SolidEdgeCodec
        .decode(&mut Cursor::new(bytes.to_vec()), options)
        .unwrap()
}

fn decode(bytes: &[u8]) -> DecodeResult {
    decode_with(bytes, &DecodeOptions::default())
}

fn has_loss(result: &DecodeResult, code: SolidEdgeLossCode) -> bool {
    result
        .report()
        .losses
        .iter()
        .any(|loss| loss.code == code.kind())
}

fn pair_without(streams: Vec<(String, crate::test_support::cfb::Node)>) -> Vec<u8> {
    document(vec![storage("PARASOLID", streams)])
}

#[test]
fn detects_a_parasolid_storage_by_content() {
    let bytes = part_with_pair();
    let found = with_decode_context(&bytes, |ctx| {
        SolidEdgeCodec
            .detect(ctx, cadmpeg_core::decode::View::over_retained(&bytes))
            .unwrap()
    });
    assert_eq!(found, Confidence::High);
}

#[test]
fn detection_ignores_a_compound_file_without_the_storage() {
    let bytes = document(vec![stream("Other", b"data")]);
    let found = with_decode_context(&bytes, |ctx| {
        SolidEdgeCodec
            .detect(ctx, cadmpeg_core::decode::View::over_retained(&bytes))
            .unwrap()
    });
    assert_eq!(found, Confidence::No);
    let found = with_decode_context(b"not a compound file", |ctx| {
        SolidEdgeCodec
            .detect(
                ctx,
                cadmpeg_core::decode::View::over_retained(b"not a compound file"),
            )
            .unwrap()
    });
    assert_eq!(found, Confidence::No);
}

#[test]
fn storage_and_stream_names_match_without_regard_to_case() {
    let bytes = document(vec![storage(
        "Parasolid",
        vec![stream("stream0.p_b", &partition_stream(&triangle_body()))],
    )]);
    let result = decode(&bytes);
    assert_eq!(result.ir().model.faces.len(), 1);
}

#[test]
fn decodes_the_pair_into_a_connected_body() {
    let result = decode(&part_with_pair());
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
fn the_deltas_stream_updates_point_records_of_the_partition() {
    let x_values = |result: &DecodeResult| {
        let mut values: Vec<f64> = result
            .ir()
            .model
            .points
            .iter()
            .map(|point| point.position().get().x)
            .collect();
        values.sort_by(f64::total_cmp);
        values
    };
    let partition = partition_stream(&triangle_body());
    let alone = decode(&pair_without(vec![stream("STREAM0.P_B", &partition)]));
    let merged = decode(&pair_without(vec![
        stream("STREAM0.P_B", &partition),
        stream("STREAM0.D_B", &deltas_stream(&moved_point_deltas_body())),
    ]));
    let alone = x_values(&alone);
    let merged = x_values(&merged);
    assert_eq!(alone.len(), 3);
    assert_eq!(merged.len(), 3);
    assert!((alone[0] - 0.0).abs() < 1e-9, "{alone:?}");
    assert!(
        (merged[0] - 0.0).abs() < 1e-9 && (merged[1] - 20.0).abs() < 1e-9,
        "{merged:?}"
    );
}

#[test]
fn reports_the_document_as_solid_edge_with_a_kernel_layer_per_stream() {
    let result = decode(&part_with_pair());
    let layers: Vec<_> = result
        .ir()
        .source
        .as_ref()
        .unwrap()
        .dialects()
        .unwrap()
        .iter()
        .map(|layer| layer.dialect().as_str().to_owned())
        .collect();
    assert_eq!(layers[0], "solidedge:cfb-parasolid");
    assert_eq!(layers.len(), 3);
    assert!(layers[1..].iter().all(|id| id == "parasolid:unknown"));
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::KernelDialectUnverified
    ));
}

#[test]
fn retains_both_streams_byte_exact() {
    let partition = partition_stream(&triangle_body());
    let deltas = deltas_stream(&deltas_body());
    let editable = cadmpeg_test_support::EditableDecodeResult::from(decode(&part_with_pair()));
    let retained: Vec<&[u8]> = editable
        .source_fidelity()
        .retained_records()
        .iter()
        .filter(|(id, _)| id.as_str().starts_with("solidedge:file:parasolid-stream#"))
        .map(|(_, record)| record.data().unwrap())
        .collect();
    assert_eq!(retained.len(), 2);
    assert!(retained.contains(&partition.as_slice()));
    assert!(retained.contains(&deltas.as_slice()));
}

#[test]
fn the_decoded_model_passes_neutral_validation() {
    let result = decode(&part_with_pair());
    let report = cadmpeg_ir::validate_neutral(result.ir(), Vec::new()).unwrap();
    assert!(report.is_ok(), "{report:?}");
}

#[test]
fn a_missing_deltas_stream_decodes_the_partition_and_charges_a_loss() {
    let result = decode(&pair_without(vec![stream(
        "STREAM0.P_B",
        &partition_stream(&triangle_body()),
    )]));
    assert!(result.report().geometry_transferred());
    assert!(has_loss(&result, SolidEdgeLossCode::ContainerDeltasMissing));
}

#[test]
fn a_forced_decode_without_a_parasolid_storage_is_a_typed_container_only_result() {
    let result = decode(&document(vec![stream("Other", b"data")]));
    assert!(!result.report().geometry_transferred());
    assert!(result.ir().model.faces.is_empty());
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ContainerNoParasolidStorage
    ));
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::SourceDialectUnverified
    ));
    assert!(has_loss(&result, SolidEdgeLossCode::GeometryNotTransferred));
    assert_eq!(
        result
            .ir()
            .source
            .as_ref()
            .unwrap()
            .dialects()
            .unwrap()
            .primary()
            .dialect()
            .as_str(),
        "solidedge:unknown"
    );
}

#[test]
fn a_storage_without_a_partition_stream_reports_no_model() {
    let result = decode(&pair_without(vec![stream(
        "STREAM0.D_B",
        &deltas_stream(&deltas_body()),
    )]));
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ContainerNoPartitionStream
    ));
    assert!(has_loss(&result, SolidEdgeLossCode::GeometryNotTransferred));
}

#[test]
fn a_partition_without_a_transmit_header_reports_no_model() {
    let result = decode(&pair_without(vec![stream("STREAM0.P_B", &[0x5a; 64])]));
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ParasolidHeaderUnreadable
    ));
    let blocking = result
        .report()
        .losses
        .iter()
        .any(|loss| loss.severity == Severity::Blocking);
    assert!(blocking);
}

#[test]
fn entries_outside_the_stream0_pair_are_listed_and_not_decoded() {
    let mut streams = vec![
        stream("STREAM0.P_B", &partition_stream(&triangle_body())),
        stream("STREAM0.D_B", &deltas_stream(&deltas_body())),
        stream("STREAM1.P_B", &partition_stream(&triangle_body())),
        stream("Extra", b"payload"),
    ];
    streams.reverse();
    let result = decode(&pair_without(streams));
    let untransferred = result
        .report()
        .losses
        .iter()
        .filter(|loss| loss.code == SolidEdgeLossCode::ContainerStreamUntransferred.kind())
        .count();
    assert_eq!(untransferred, 2);
    assert_eq!(result.ir().model.faces.len(), 1);
}

#[test]
fn container_only_decode_skips_the_model() {
    let result = decode_with(
        &part_with_pair(),
        &DecodeOptions {
            container_only: true,
            ..DecodeOptions::default()
        },
    );
    assert!(result.ir().model.faces.is_empty());
    assert!(!has_loss(
        &result,
        SolidEdgeLossCode::GeometryNotTransferred
    ));
}

#[test]
fn inspect_classifies_the_directory() {
    let summary = SolidEdgeCodec
        .inspect(
            &mut Cursor::new(part_with_pair()),
            &InspectOptions::default(),
        )
        .unwrap();
    let roles: Vec<_> = summary
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.role))
        .collect();
    assert!(roles.contains(&("PARASOLID", ContainerRole::Storage)));
    assert!(roles.contains(&("PARASOLID/STREAM0.P_B", ContainerRole::ParasolidStream)));
    assert!(roles.contains(&("PARASOLID/STREAM0.D_B", ContainerRole::ParasolidStream)));
    let partition = summary
        .entries
        .iter()
        .find(|entry| entry.name == "PARASOLID/STREAM0.P_B")
        .unwrap();
    assert_eq!(partition.attributes["parasolid_role"], "partition");
}

#[test]
fn a_file_that_is_not_a_compound_file_is_refused_as_the_wrong_format() {
    let error = SolidEdgeCodec
        .decode(
            &mut Cursor::new(b"PK\x03\x04 zip".to_vec()),
            &DecodeOptions::default(),
        )
        .unwrap_err();
    assert!(
        matches!(
            &error,
            DecodeFailure::Codec(cadmpeg_core::CodecError::WrongFormat(_))
        ),
        "{error:?}"
    );
}

#[test]
fn a_larger_stream_reads_through_regular_sectors() {
    let mut body = triangle_body();
    body.resize(6000, 0);
    let bytes = pair_without(vec![stream("STREAM0.P_B", &partition_stream(&body))]);
    let result = decode(&bytes);
    assert_eq!(result.ir().model.faces.len(), 1);
}
