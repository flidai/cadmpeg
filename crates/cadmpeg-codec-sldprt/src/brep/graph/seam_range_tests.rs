// SPDX-License-Identifier: Apache-2.0
//! Seam points of ring edges and edges defined by one reversed coedge.

use super::topology::Coedge;
use cadmpeg_ir::topology::Sense;

#[test]
fn a_sole_reversed_coedge_defines_an_edge_without_a_forward_one() {
    use std::collections::HashMap;

    let record = |attr, sense| Coedge {
        attr,
        refs: [0, 0, 0, 0, 101, 10, 7, 0, 0],
        sense,
        offset: 0,
    };
    let reversed = HashMap::from([(10, record(10, Sense::Reversed))]);
    let forward = HashMap::from([(10, record(10, Sense::Forward))]);
    let one = HashMap::from([(7, 1_usize)]);
    let two = HashMap::from([(7, 2_usize)]);
    let incidences = [(10_u16, 0_u16, 0_u16)];
    let pick = |counts, coedges| super::sole_reversed_coedge(7, None, &incidences, counts, coedges);
    assert_eq!(pick(&one, &reversed), Some((10, true)));
    assert_eq!(pick(&one, &forward), None);
    assert_eq!(pick(&two, &reversed), None);
}

#[test]
fn a_ring_edge_on_a_circle_starts_at_the_start_of_its_range() {
    use crate::brep::CurveCarrier;
    use cadmpeg_ir::geometry::{CurveGeometry, SolvedCurveGeometry};
    use cadmpeg_ir::math::{Point3, Vector3};
    let geometry = CurveGeometry::Solved(SolvedCurveGeometry::Circle(
        cadmpeg_ir::geometry::analytic::CircleCurve::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            5.0,
        )
        .unwrap(),
    ));
    let ctx = cadmpeg_test_support::service_decode_context();
    let carrier = |range| CurveCarrier {
        attr: 1,
        offset: 0,
        end: 0,
        geometry: geometry.clone(),
        parameter_range: cadmpeg_ir::units::FiniteVector::new(range),
    };
    let at_zero = super::closed_seam_point(&ctx, &carrier([0.0, std::f64::consts::TAU]))
        .unwrap()
        .unwrap();
    assert!((at_zero.x - 5.0).abs() < 1e-9 && at_zero.y.abs() < 1e-9);
    let quarter = std::f64::consts::FRAC_PI_2;
    let shifted =
        super::closed_seam_point(&ctx, &carrier([quarter, quarter + std::f64::consts::TAU]))
            .unwrap()
            .unwrap();
    assert!(shifted.x.abs() < 1e-9 && (shifted.y - 5.0).abs() < 1e-9);
}

#[test]
fn an_explicit_canonical_reference_to_the_sole_coedge_keeps_the_ends() {
    use super::topology::{EdgeReferences, EdgeUse};
    use std::collections::HashMap;

    let coedge = Coedge {
        attr: 10,
        refs: [0, 0, 0, 0, 101, 10, 7, 0, 0],
        sense: Sense::Reversed,
        offset: 0,
    };
    let coedges = HashMap::from([(10, coedge)]);
    let counts = HashMap::from([(7, 1_usize)]);
    let incidences = [(10_u16, 0_u16, 0_u16)];
    let edge_use = |canonical| EdgeUse {
        attr: 7,
        references: EdgeReferences::Bare([canonical, 0, 0, 0, 0, 0]),
        sequence: 0,
        offset: 0,
    };
    let pick = |canonical| {
        super::sole_reversed_coedge(
            7,
            Some(&edge_use(canonical)),
            &incidences,
            &counts,
            &coedges,
        )
    };
    assert_eq!(pick(10), Some((10, false)));
    assert_eq!(pick(11), None);
}

#[test]
fn a_face_of_no_body_record_joins_the_shell_of_its_neighbours() {
    use super::{adopt_unclaimed_faces, WalkStop, WalkedFace};
    use std::collections::HashMap;

    let coedge = |attr, edge| Coedge {
        attr,
        refs: [0, 0, 0, 0, 101, 0, edge, 0, 0],
        sense: Sense::Forward,
        offset: 0,
    };
    let coedges: HashMap<u16, Coedge> = [(1, 7), (2, 8), (3, 7), (4, 9), (5, 9), (6, 11)]
        .into_iter()
        .map(|(attr, edge)| (attr, coedge(attr, edge)))
        .collect();
    let face = |bridge_attr, ring: Vec<u16>| WalkedFace {
        bridge_attr,
        surface_attr: 0,
        sense: Sense::Forward,
        loops: vec![(bridge_attr, ring)],
        stop: WalkStop::End,
        ring_stop: None,
    };
    // Face 10 is bound; 20 shares edge 7 with it; 30 shares edge 9 with 20;
    // 40 shares nothing.
    let faces = [
        face(10, vec![1, 2]),
        face(20, vec![3, 4]),
        face(30, vec![5]),
        face(40, vec![6]),
    ];
    let bytes = [0_u8; 8];
    let arena = cadmpeg_core::decode::DecodeArena::new();
    let ctx = cadmpeg_core::decode::DecodeContext::from_root_bytes(
        &bytes,
        &arena,
        &cadmpeg_core::decode::DecodePolicy::service(),
    )
    .expect("root")
    .0;
    let mut group = HashMap::from([(10_u16, 3_usize)]);
    let mut shell = HashMap::from([(10_u16, 5_u16)]);
    assert_eq!(
        adopt_unclaimed_faces(&ctx, &coedges, &faces, &mut group, &mut shell).expect("adopt"),
        2
    );
    assert_eq!(group.get(&20), Some(&3));
    assert_eq!(group.get(&30), Some(&3));
    assert_eq!(shell.get(&30), Some(&5));
    assert_eq!(group.get(&40), None);

    // A face between two shells stays unclaimed.
    let mut group = HashMap::from([(10_u16, 3_usize), (30_u16, 4_usize)]);
    let mut shell = HashMap::from([(10_u16, 5_u16), (30_u16, 6_u16)]);
    adopt_unclaimed_faces(&ctx, &coedges, &faces, &mut group, &mut shell).expect("adopt");
    assert_eq!(group.get(&20), None);
}
