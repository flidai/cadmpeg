// SPDX-License-Identifier: Apache-2.0

#[test]
fn a_ring_edge_on_a_closed_ellipse_has_a_seam_point() {
    use crate::brep::CurveCarrier;
    use cadmpeg_ir::geometry::{CurveGeometry, SolvedCurveGeometry};
    use cadmpeg_ir::math::{Point3, Vector3};
    let geometry = CurveGeometry::Solved(SolvedCurveGeometry::Ellipse(
        cadmpeg_ir::geometry::analytic::EllipseCurve::try_new(
            Point3::new(3.0, -2.0, 7.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            5.0,
            2.0,
        )
        .unwrap(),
    ));
    let carrier = CurveCarrier {
        marker: None,
        attr: 1,
        offset: 0,
        end: 0,
        geometry,
        parameter_range: None,
    };
    let ctx = cadmpeg_test_support::service_decode_context();
    let seam = super::closed_seam_point(&ctx, &carrier).unwrap().unwrap();
    assert!((seam.x - 8.0).abs() < 1e-9 && (seam.y + 2.0).abs() < 1e-9);
    let open = CurveCarrier {
        parameter_range: cadmpeg_ir::units::FiniteVector::new([0.0, 1.0]),
        ..carrier
    };
    assert!(super::closed_seam_point(&ctx, &open).unwrap().is_none());
}

#[test]
fn angular_edge_range_starts_within_one_turn_from_zero() {
    use std::f64::consts::TAU;
    // A start a rounding error below zero is read as zero.
    assert_eq!(super::canonical_angular_range([-5.0e-15, 1.5]), [0.0, 1.5]);
    // A start inside the turn stays as stored.
    assert_eq!(super::canonical_angular_range([0.5, 1.5]), [0.5, 1.5]);
    // A start further below zero moves up by whole turns with its sweep.
    let [start, end] = super::canonical_angular_range([-1.0, 0.5]);
    assert!((start - (TAU - 1.0)).abs() < 1.0e-12);
    assert!((end - (TAU + 0.5)).abs() < 1.0e-12);
}
