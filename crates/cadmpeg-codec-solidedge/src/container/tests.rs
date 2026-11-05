// SPDX-License-Identifier: Apache-2.0
//! Directory scan tests.

use super::{
    group_number, is_marker_path, kind_of, path_below, DocumentKind, StreamName, StreamRole,
};

#[test]
fn stream_names_parse_by_pattern_without_regard_to_case() {
    let partition = StreamName::parse("STREAM0.P_B").unwrap();
    assert_eq!(partition.group, "STREAM0");
    assert_eq!(group_number(&partition.group), Some(0));
    assert_eq!(partition.role, StreamRole::Partition);
    let deltas = StreamName::parse("stream12.d_b").unwrap();
    assert_eq!(deltas.group, "STREAM12");
    assert_eq!(group_number(&deltas.group), Some(12));
    assert_eq!(deltas.role, StreamRole::Deltas);
    let mixed = StreamName::parse("Stream7.P_B").unwrap();
    assert_eq!(mixed.group, "STREAM7");
}

#[test]
fn other_stems_form_their_own_groups() {
    let assembly = StreamName::parse("ASSEMBLYMODEL.D_B").unwrap();
    assert_eq!(assembly.group, "ASSEMBLYMODEL");
    assert_eq!(group_number(&assembly.group), None);
    assert_eq!(assembly.role, StreamRole::Deltas);
    let standalone = StreamName::parse("Part.X_B").unwrap();
    assert_eq!(standalone.role, StreamRole::Transmit);
}

#[test]
fn sketch_stems_are_sketches_whatever_the_extension() {
    for leaf in ["SKETCH.P_B", "SVSKETCH.P_B", "svsketch.d_b", "Sketch.X_B"] {
        assert_eq!(
            StreamName::parse(leaf).unwrap().role,
            StreamRole::Sketch,
            "{leaf}"
        );
    }
}

#[test]
fn names_without_a_stream_extension_do_not_parse() {
    for name in [
        "STREAM0",
        "STREAM0P_B",
        ".P_B",
        "STREAM0.TXT",
        "STREAM0.P_B.bak",
        "",
    ] {
        assert!(StreamName::parse(name).is_none(), "{name}");
    }
}

#[test]
fn group_numbers_need_digits_only() {
    assert_eq!(group_number("STREAM3"), Some(3));
    assert_eq!(group_number("STREAM"), None);
    assert_eq!(group_number("STREAMA"), None);
    assert_eq!(group_number("STREAM-1"), None);
    assert_eq!(group_number("STREAM99999999999"), None);
    assert_eq!(group_number("XSTREAM0"), None);
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
fn markers_are_recognized_by_their_first_path_component() {
    assert!(is_marker_path("PARASOLID"));
    assert!(is_marker_path("Parasolid/STREAM0.P_B"));
    assert!(is_marker_path("DocVersion2"));
    assert!(is_marker_path("PSMroots"));
    assert!(is_marker_path("FamilyInfo"));
    assert!(!is_marker_path("X/PARASOLID"));
    assert!(!is_marker_path("PARASOLIDS"));
    assert!(!is_marker_path("S:Master"));
}

#[test]
fn the_document_kind_follows_the_root_members() {
    let kind = |paths: &[&str]| kind_of(paths.iter().copied());
    assert_eq!(
        kind(&["PARASOLID", "PARASOLID/STREAM0.P_B"]),
        DocumentKind::Model
    );
    assert_eq!(
        kind(&["DocVersion2", "PSMroots"]),
        DocumentKind::ModelWithoutGeometry
    );
    assert_eq!(
        kind(&["FamilyInfo", "FamilyPropertyFlags", "S:Master"]),
        DocumentKind::FamilyMaster
    );
    assert_eq!(
        kind(&["FamilyInfo", "DocVersion2"]),
        DocumentKind::ModelWithoutGeometry
    );
    assert_eq!(
        kind(&["WordDocument", "1Table"]),
        DocumentKind::Unrecognized
    );
}
