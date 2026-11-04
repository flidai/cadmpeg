// SPDX-License-Identifier: Apache-2.0
//! Directory scan tests.

use super::{is_parasolid_path, path_below, StreamName, StreamRole};

#[test]
fn stream_names_parse_by_pattern_without_regard_to_case() {
    let partition = StreamName::parse("STREAM0.P_B").unwrap();
    assert_eq!(partition.index, 0);
    assert_eq!(partition.role, StreamRole::Partition);
    let deltas = StreamName::parse("stream12.d_b").unwrap();
    assert_eq!(deltas.index, 12);
    assert_eq!(deltas.role, StreamRole::Deltas);
}

#[test]
fn other_names_do_not_parse() {
    for name in [
        "STREAM.P_B",
        "STREAM0.X_B",
        "STREAM0",
        "STREAM0P_B",
        "STREAMA.P_B",
        "STREAM-1.P_B",
        "XSTREAM0.P_B",
        "STREAM99999999999.P_B",
        "",
    ] {
        assert!(StreamName::parse(name).is_none(), "{name}");
    }
}

#[test]
fn a_leaf_is_below_the_storage_only_directly() {
    assert_eq!(
        path_below("PARASOLID/STREAM0.P_B", "PARASOLID"),
        Some("STREAM0.P_B")
    );
    assert_eq!(path_below("parasolid/x", "PARASOLID"), Some("x"));
    assert_eq!(path_below("PARASOLID/inner/x", "PARASOLID"), None);
    assert_eq!(path_below("PARASOLIDX/x", "PARASOLID"), None);
    assert_eq!(path_below("PARASOLID", "PARASOLID"), None);
    assert_eq!(path_below("é", "PARASOLID"), None);
}

#[test]
fn the_storage_is_recognized_by_its_first_path_component() {
    assert!(is_parasolid_path("PARASOLID"));
    assert!(is_parasolid_path("Parasolid/STREAM0.P_B"));
    assert!(!is_parasolid_path("X/PARASOLID"));
    assert!(!is_parasolid_path("PARASOLIDS"));
}
