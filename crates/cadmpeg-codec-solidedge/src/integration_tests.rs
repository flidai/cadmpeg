// SPDX-License-Identifier: Apache-2.0
//! Codec-wide detection, inspection, and decode over synthetic documents.

use std::io::Cursor;

use cadmpeg_core::container::ContainerRole;
use cadmpeg_core::decode::InspectOptions;
use cadmpeg_ir::codec::{Codec, Confidence, DecodeFailure, DecodeOptions, DecodeResult};
use cadmpeg_ir::report::Severity;

use crate::loss::SolidEdgeLossCode;
use crate::test_support::cfb::{storage, stream};
use crate::test_support::parasolid::text_header;
use crate::test_support::{
    binary_deltas, binary_partition, deltas_body, deltas_stream, document, moved_point_deltas_body,
    part_with_pair, partition_stream, partition_stream_of, transmit_stream, triangle_body,
    two_triangle_body, with_decode_context,
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
fn a_group_without_a_base_state_and_other_entries_are_listed_and_not_decoded() {
    let mut streams = vec![
        stream("STREAM0.P_B", &partition_stream(&triangle_body())),
        stream("STREAM0.D_B", &deltas_stream(&deltas_body())),
        stream("STREAM1.D_B", &deltas_stream(&deltas_body())),
        stream("Extra", b"payload"),
    ];
    streams.reverse();
    let result = decode(&pair_without(streams));
    let count = |code: SolidEdgeLossCode| {
        result
            .report()
            .losses
            .iter()
            .filter(|loss| loss.code == code.kind())
            .count()
    };
    assert_eq!(count(SolidEdgeLossCode::ContainerStreamUntransferred), 1);
    assert_eq!(count(SolidEdgeLossCode::ContainerBodyGroupUntransferred), 1);
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

fn assert_one_face(result: &DecodeResult) {
    assert!(result.report().geometry_transferred());
    assert_eq!(result.ir().model.faces.len(), 1);
}

#[test]
fn a_binary_stream_without_a_text_header_decodes() {
    let result = decode(&pair_without(vec![
        stream("STREAM0.P_B", &binary_partition(&triangle_body())),
        stream("STREAM0.D_B", &binary_deltas(&deltas_body())),
    ]));
    assert_one_face(&result);
}

#[test]
fn the_text_header_schema_classifies_the_stream_layers() {
    for schema in [
        "SCH_1400184_14000",
        "SCH_1700272_16100",
        "SCH_2201216_20000",
        "SCH_3001278_30100",
        "SCH_3500239_35002",
        "SCH_3501256_35102",
    ] {
        let result = decode(&pair_without(vec![stream(
            "STREAM0.P_B",
            &partition_stream_of(schema, &triangle_body()),
        )]));
        assert_one_face(&result);
        let source = result.ir().source.as_ref().unwrap();
        assert_eq!(
            source
                .attributes
                .get("parasolid_schema")
                .map(String::as_str),
            Some(schema)
        );
        assert!(has_loss(
            &result,
            SolidEdgeLossCode::KernelDialectUnverified
        ));
    }
}

#[test]
fn any_stream_index_and_case_names_the_pair() {
    for (partition, deltas) in [
        ("STREAM7.P_B", "STREAM7.D_B"),
        ("Stream3.P_B", "Stream3.D_B"),
        ("stream12.p_b", "stream12.d_b"),
    ] {
        let result = decode(&pair_without(vec![
            stream(partition, &partition_stream(&triangle_body())),
            stream(deltas, &deltas_stream(&deltas_body())),
        ]));
        assert_one_face(&result);
        assert!(!has_loss(
            &result,
            SolidEdgeLossCode::ContainerDeltasMissing
        ));
    }
}

#[test]
fn the_first_body_group_is_the_design_body_and_later_groups_stay_native() {
    let result = decode(&pair_without(vec![
        stream("STREAM9.P_B", &partition_stream(&triangle_body())),
        stream("STREAM2.P_B", &partition_stream(&triangle_body())),
        stream("STREAM2.D_B", &deltas_stream(&deltas_body())),
        stream("STREAM0FLAT0.P_B", &partition_stream(&triangle_body())),
    ]));
    assert!(result.report().geometry_transferred());
    let model = &result.ir().model;
    assert_eq!((model.bodies.len(), model.faces.len()), (1, 1));
    assert!(model.bodies.iter().all(|body| body.visible.is_none()));
    let source = result.ir().source.as_ref().unwrap();
    let attribute = |name: &str| source.attributes.get(name).map(String::as_str);
    assert_eq!(attribute("parasolid_stream_group"), Some("STREAM2"));
    assert_eq!(
        attribute("parasolid_body_groups"),
        Some("STREAM2,STREAM9,STREAM0FLAT0")
    );
    let secondary = result
        .report()
        .losses
        .iter()
        .filter(|loss| loss.code == SolidEdgeLossCode::ContainerBodyGroupUntransferred.kind())
        .count();
    assert_eq!(secondary, 2);
    assert_eq!(model.faces[0].id.as_str(), "sldprt:brep:face#10");
    let report = cadmpeg_ir::validate_neutral(result.ir(), Vec::new()).unwrap();
    assert!(report.is_ok(), "{report:?}");
    // Every stream of every group stays retained byte-exact.
    let editable = cadmpeg_test_support::EditableDecodeResult::from(result);
    assert!(editable.source_fidelity().retained_records().len() >= 4);
}

#[test]
fn a_single_group_has_no_secondary_group_loss() {
    let result = decode(&pair_without(vec![stream(
        "STREAM0.P_B",
        &partition_stream(&triangle_body()),
    )]));
    assert!(!has_loss(
        &result,
        SolidEdgeLossCode::ContainerBodyGroupUntransferred
    ));
}

#[test]
fn a_header_that_counts_more_fat_sectors_than_the_difat_lists_is_read_with_a_loss() {
    let mut bytes = pair_without(vec![stream(
        "STREAM0.P_B",
        &partition_stream(&triangle_body()),
    )]);
    let declared = u32::from_le_bytes(bytes[44..48].try_into().unwrap());
    bytes[44..48].copy_from_slice(&(declared + 1).to_le_bytes());
    let result = decode(&bytes);
    assert!(result.report().geometry_transferred());
    assert!(has_loss(&result, SolidEdgeLossCode::ContainerCfbTolerated));
    assert!(!has_loss(&result, SolidEdgeLossCode::ContainerUnreadable));
}

#[test]
fn a_directory_the_reader_cannot_list_is_a_typed_result_that_names_the_check() {
    let bytes = document(vec![
        stream("A/B", b"data"),
        storage(
            "PARASOLID",
            vec![stream("STREAM0.P_B", &partition_stream(&triangle_body()))],
        ),
    ]);
    let result = decode(&bytes);
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(&result, SolidEdgeLossCode::ContainerUnreadable));
    let loss = result
        .report()
        .losses
        .iter()
        .find(|loss| loss.code == SolidEdgeLossCode::ContainerUnreadable.kind())
        .unwrap();
    assert!(loss.message.contains("`/`"), "{}", loss.message);
    let found = with_decode_context(&bytes, |ctx| {
        SolidEdgeCodec
            .detect(ctx, cadmpeg_core::decode::View::over_retained(&bytes))
            .unwrap()
    });
    assert_eq!(found, Confidence::Low);
}

#[test]
fn a_standalone_transmit_stream_decodes_as_a_base_state() {
    let result = decode(&pair_without(vec![stream(
        "Body.X_B",
        &transmit_stream(&triangle_body()),
    )]));
    assert_one_face(&result);
    assert!(!has_loss(
        &result,
        SolidEdgeLossCode::ContainerDeltasMissing
    ));
}

fn face_senses_of(body: &[u8]) -> Vec<cadmpeg_ir::topology::Sense> {
    let result = decode(&pair_without(vec![stream(
        "Body.X_B",
        &transmit_stream(body),
    )]));
    assert!(result.report().geometry_transferred());
    let faces = &result.ir().model.faces;
    assert_eq!(faces.len(), 2);
    let mut senses: Vec<_> = faces
        .iter()
        .map(|face| (face.id.as_str().to_owned(), face.sense))
        .collect();
    senses.sort_by(|a, b| a.0.cmp(&b.0));
    senses.into_iter().map(|(_, sense)| sense).collect()
}

#[test]
fn a_hosted_stream_keeps_the_face_senses_of_its_records() {
    use cadmpeg_ir::topology::Sense;
    // The coedge senses place the two faces on the same side. The SolidWorks rule rewrites the
    // senses to agree with that, which would make both forward; a hosted stream keeps what its
    // face records say.
    assert_eq!(
        face_senses_of(&two_triangle_body(true)),
        [Sense::Forward, Sense::Reversed]
    );
    assert_eq!(
        face_senses_of(&two_triangle_body(false)),
        [Sense::Forward, Sense::Forward]
    );
}

#[test]
fn an_assembly_model_pair_decodes_as_a_body_group() {
    let result = decode(&pair_without(vec![
        stream("SVSKETCH.P_B", &partition_stream(&triangle_body())),
        stream("ASSEMBLYMODEL.P_B", &partition_stream(&triangle_body())),
        stream("ASSEMBLYMODEL.D_B", &deltas_stream(&deltas_body())),
    ]));
    assert_one_face(&result);
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ContainerSketchUntransferred
    ));
}

#[test]
fn a_sketch_only_storage_is_a_typed_refusal() {
    for name in ["SVSKETCH.P_B", "SKETCH.P_B"] {
        let result = decode(&pair_without(vec![stream(
            name,
            &partition_stream(&triangle_body()),
        )]));
        assert!(!result.report().geometry_transferred());
        assert!(result.ir().model.faces.is_empty());
        assert!(has_loss(&result, SolidEdgeLossCode::ContainerNoBodyStream));
        assert!(has_loss(
            &result,
            SolidEdgeLossCode::ContainerSketchUntransferred
        ));
        assert!(!has_loss(
            &result,
            SolidEdgeLossCode::ContainerNoPartitionStream
        ));
    }
}

#[test]
fn an_empty_storage_is_a_typed_refusal() {
    let result = decode(&pair_without(Vec::new()));
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(&result, SolidEdgeLossCode::ContainerNoBodyStream));
}

#[test]
fn a_family_master_is_identified_and_refused() {
    let bytes = document(vec![
        stream("FamilyInfo", b"info"),
        stream("FamilyPropertyFlags", b"flags"),
        storage("S:Master", vec![stream("Data", b"data")]),
    ]);
    let found = with_decode_context(&bytes, |ctx| {
        SolidEdgeCodec
            .detect(ctx, cadmpeg_core::decode::View::over_retained(&bytes))
            .unwrap()
    });
    assert_eq!(found, Confidence::High);
    let result = decode(&bytes);
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ContainerNotModelDocument
    ));
    assert!(!has_loss(
        &result,
        SolidEdgeLossCode::ContainerNoParasolidStorage
    ));
}

#[test]
fn a_model_document_without_geometry_is_a_typed_refusal_with_its_version() {
    let bytes = document(vec![
        stream("DocVersion2", &[1, 0, 223, 0]),
        stream("PSMroots", b"roots"),
    ]);
    let found = with_decode_context(&bytes, |ctx| {
        SolidEdgeCodec
            .detect(ctx, cadmpeg_core::decode::View::over_retained(&bytes))
            .unwrap()
    });
    assert_eq!(found, Confidence::High);
    let result = decode(&bytes);
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ContainerNoParasolidStorage
    ));
    assert!(!has_loss(
        &result,
        SolidEdgeLossCode::ContainerNotModelDocument
    ));
    assert!(result
        .report()
        .notes
        .iter()
        .any(|note| note.contains("DocVersion2") && note.contains("223")));
}

#[test]
fn directory_names_with_a_colon_are_read() {
    let bytes = document(vec![
        storage("S:JSite1", vec![stream("S:Data", b"data")]),
        storage(
            "PARASOLID",
            vec![stream("STREAM0.P_B", &partition_stream(&triangle_body()))],
        ),
    ]);
    assert_one_face(&decode(&bytes));
}

#[test]
fn a_text_encoded_transmit_is_not_decoded() {
    let mut text = text_header("SCH_3201255_32001", "text");
    text.extend(binary_partition(&triangle_body()));
    let result = decode(&pair_without(vec![stream("STREAM0.P_B", &text)]));
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ParasolidHeaderUnreadable
    ));
}

#[test]
fn a_text_header_without_binary_data_is_not_decoded() {
    let text = text_header("SCH_3201255_32001", "binary");
    let result = decode(&pair_without(vec![stream("STREAM0.P_B", &text)]));
    assert!(!result.report().geometry_transferred());
    assert!(has_loss(
        &result,
        SolidEdgeLossCode::ParasolidHeaderUnreadable
    ));
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
fn a_model_whose_ranges_pass_the_checks_keeps_every_range() {
    let mut ir = cadmpeg_ir::examples::unit_cube().unwrap();
    let before = ir.clone();
    let withheld = with_decode_context(&[], |ctx| {
        crate::decode::withhold_rejected_ranges(ctx, &mut ir).unwrap()
    });
    assert_eq!(withheld, crate::decode::RangeOutcome::default());
    assert_eq!(ir, before);
}

#[test]
fn a_circle_range_that_starts_past_one_turn_moves_to_the_same_points() {
    use cadmpeg_ir::geometry::{CurveGeometry, SolvedCurveGeometry};
    use cadmpeg_ir::math::{Point3, Vector3};
    let circle = CurveGeometry::Solved(SolvedCurveGeometry::Circle(
        cadmpeg_ir::geometry::analytic::CircleCurve::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            5.0,
        )
        .unwrap(),
    ));
    let turn = std::f64::consts::TAU;
    let moved = crate::decode::equivalent_range(&circle, [turn + 0.5, turn + 1.5]);
    assert!((moved[0] - 0.5).abs() < 1e-12 && (moved[1] - 1.5).abs() < 1e-12);
    let negative = crate::decode::equivalent_range(&circle, [-0.5, 0.5]);
    assert!((negative[0] - (turn - 0.5)).abs() < 1e-12);
    assert_eq!(
        crate::decode::equivalent_range(&circle, [0.5, 1.5]),
        [0.5, 1.5]
    );
}
