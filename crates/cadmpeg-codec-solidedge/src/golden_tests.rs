// SPDX-License-Identifier: Apache-2.0
//! Golden inspect and decode snapshots over code-built Solid Edge documents.
//!
//! Inputs come from [`crate::test_support`] builders. Shared harness:
//! [`cadmpeg_test_support::golden`]. `UPDATE_GOLDEN=1` rewrites goldens only.

use std::io::Cursor;

use cadmpeg_core::decode::InspectOptions;
use cadmpeg_ir::codec::{Codec, DecodeOptions};
use cadmpeg_test_support::golden::{snapshot_text, Branch, Harness};
use cadmpeg_test_support::EditableDecodeResult;

use crate::test_support::cfb::{storage, stream};
use crate::test_support::{
    binary_partition, deltas_body, deltas_stream, document, part_with_pair, partition_stream,
    transmit_stream, triangle_body,
};
use crate::SolidEdgeCodec;

const REGENERATE: &str = "UPDATE_GOLDEN=1 cargo test -p cadmpeg-codec-solidedge golden";

fn harness() -> Harness {
    Harness::new(env!("CARGO_MANIFEST_DIR"), "par", REGENERATE)
}

fn branches() -> [Branch; 2] {
    [
        Branch::named("inspect", inspect_snapshot),
        Branch::named("decode", decode_snapshot),
    ]
}

fn inputs() -> Vec<(String, Vec<u8>)> {
    let partition = partition_stream(&triangle_body());
    vec![
        ("partition_and_deltas".to_string(), part_with_pair()),
        (
            "partition_only".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![stream("STREAM0.P_B", &partition)],
            )]),
        ),
        (
            "deltas_without_partition".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![stream("STREAM0.D_B", &deltas_stream(&deltas_body()))],
            )]),
        ),
        (
            "additional_stream".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![
                    stream("STREAM0.P_B", &partition),
                    stream("STREAM0.D_B", &deltas_stream(&deltas_body())),
                    stream("STREAM1.P_B", &partition),
                ],
            )]),
        ),
        (
            "no_parasolid_storage".to_string(),
            document(vec![stream("Other", b"payload")]),
        ),
        (
            "numbered_group_mixed_case".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![
                    stream("Stream7.P_B", &partition),
                    stream("Stream7.D_B", &deltas_stream(&deltas_body())),
                ],
            )]),
        ),
        (
            "standalone_transmit".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![stream("Body.X_B", &transmit_stream(&triangle_body()))],
            )]),
        ),
        (
            "binary_without_text_header".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![stream("STREAM0.P_B", &binary_partition(&triangle_body()))],
            )]),
        ),
        (
            "assembly_model_with_sketch".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![
                    stream("SVSKETCH.P_B", &partition),
                    stream("ASSEMBLYMODEL.P_B", &partition),
                    stream("ASSEMBLYMODEL.D_B", &deltas_stream(&deltas_body())),
                ],
            )]),
        ),
        (
            "sketch_only".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![stream("SVSKETCH.P_B", &partition)],
            )]),
        ),
        (
            "unreadable_directory".to_string(),
            document(vec![
                stream("A/B", b"data"),
                storage("PARASOLID", vec![stream("STREAM0.P_B", &partition)]),
            ]),
        ),
        (
            "two_body_groups".to_string(),
            document(vec![storage(
                "PARASOLID",
                vec![
                    stream("STREAM0.P_B", &partition),
                    stream("STREAM1.P_B", &partition),
                    stream("STREAM1.D_B", &deltas_stream(&deltas_body())),
                ],
            )]),
        ),
        (
            "family_master".to_string(),
            document(vec![
                stream("FamilyInfo", b"info"),
                stream("FamilyPropertyFlags", b"flags"),
                storage("S:Master", vec![stream("Data", b"data")]),
            ]),
        ),
    ]
}

fn inspect_snapshot(bytes: &[u8]) -> String {
    let value = match SolidEdgeCodec.inspect(&mut Cursor::new(bytes), &InspectOptions::default()) {
        Ok(summary) => serde_json::to_value(summary).expect("serialize inspect summary"),
        Err(error) => serde_json::json!({ "inspect_error": error.to_string() }),
    };
    snapshot_text(&value)
}

fn decode_snapshot(bytes: &[u8]) -> String {
    let value = match SolidEdgeCodec.decode(&mut Cursor::new(bytes), &DecodeOptions::default()) {
        Ok(result) => {
            let result = EditableDecodeResult::from(result);
            serde_json::json!({
                "ir": result.ir(),
                "report": result.report(),
                "source_fidelity": result.source_fidelity(),
            })
        }
        Err(error) => serde_json::json!({ "decode_error": error.to_string() }),
    };
    snapshot_text(&value)
}

#[test]
fn golden_snapshots_hold() {
    harness().check_inputs(&inputs(), &branches());
}

#[test]
fn golden_output_is_deterministic() {
    harness().check_determinism_inputs(&inputs(), &branches());
}
