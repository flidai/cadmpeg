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
fn a_loop_that_its_faces_chain_does_not_reach_joins_the_face_that_owns_it() {
    use super::topology::{Bridge, Loop, Tables};
    use super::{attach_orphan_loops, Stats, WalkStop, WalkedFace};

    let arena = cadmpeg_core::decode::DecodeArena::new();
    let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
        &[],
        &arena,
        &cadmpeg_core::decode::DecodePolicy::service(),
    )
    .expect("context");
    let mut tables = Tables::default();
    tables
        .insert_bridge(
            &ctx,
            Bridge {
                attr: 10,
                refs: [0; 5],
                sequence: 1,
                sense: Sense::Forward,
                owner: None,
                offset: 0,
            },
        )
        .unwrap();
    tables
        .insert_loop(
            &ctx,
            Loop {
                attr: 20,
                refs: [0, 40, 10, 0],
                offset: 0,
            },
        )
        .unwrap();
    tables
        .insert_coedge(
            &ctx,
            Coedge {
                attr: 40,
                refs: [0, 20, 40, 40, 0, 0, 0, 0, 0],
                sense: Sense::Forward,
                offset: 0,
            },
        )
        .unwrap();
    let mut faces = vec![WalkedFace {
        bridge_attr: 10,
        surface_attr: 0,
        sense: Sense::Forward,
        loops: Vec::new(),
        stop: WalkStop::NoHead,
    }];
    let mut stats = Stats::default();
    attach_orphan_loops(&ctx, &tables, &mut faces, &mut stats).unwrap();
    assert_eq!(faces[0].loops, vec![(20, vec![40])]);
    assert_eq!(stats.orphan_loops_attached, 1);
    attach_orphan_loops(&ctx, &tables, &mut faces, &mut stats).unwrap();
    assert_eq!(faces[0].loops.len(), 1);
}
