// SPDX-License-Identifier: Apache-2.0
//! Fast arithmetic preserves the version 0.5 recurrences and ordered sums.

use super::admission::EvaluationAdmission::Standard;
use super::*;
use crate::geometry::analytic::{
    CircleCurve, CylinderSurface, EllipseCurve, LineCurve, PlaneSurface, SphereSurface,
    TorusSurface,
};
use crate::geometry::nurbs::{NurbsSurfaceAxis, NurbsSurfaceLanes};
use crate::geometry::pcurve::{CirclePcurve, EllipsePcurve, HarmonicPcurve, PcurveNurbs};
use cadmpeg_core::decode::{DecodeArena, DecodePolicy, ResourceDimension};

macro_rules! exact_default {
    ($plain:path, $explicit:path, $($argument:expr),+ $(,)?) => {
        assert_eq!($plain($($argument),+), $explicit($($argument),+, EvalPrecision::Exact));
    };
}

fn bits<const N: usize>(values: [f64; N]) -> [u64; N] {
    values.map(f64::to_bits)
}
fn coordinates(point: Point3) -> [f64; 3] {
    [point.x, point.y, point.z]
}

// Frozen from v0.5.5 eval.rs. Only allocations and checked conversions differ.
fn legacy_span(knots: &[f64], degree: usize, count: usize, t: f64) -> usize {
    if t >= knots[count] {
        return count - 1;
    }
    if t <= knots[degree] {
        return degree;
    }
    let (mut lo, mut hi) = (degree, count);
    while lo < hi {
        let mid = usize::midpoint(lo, hi);
        if t < knots[mid] {
            hi = mid;
        } else if t >= knots[mid + 1] {
            lo = mid + 1;
        } else {
            return mid;
        }
    }
    lo
}
fn legacy_basis(knots: &[f64], degree: usize, span: usize, t: f64) -> Vec<f64> {
    let mut values = vec![1.0];
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
    for j in 1..=degree {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        let mut next = vec![0.0; j + 1];
        for (r, &value) in values.iter().enumerate().take(j) {
            let denominator = right[r + 1] + left[j - r];
            let factor = if denominator == 0.0 {
                0.0
            } else {
                value / denominator
            };
            next[r] = saved + right[r + 1] * factor;
            saved = left[j - r] * factor;
        }
        next[j] = saved;
        values = next;
    }
    values
}
fn legacy_derivative(knots: &[f64], degree: usize, span: usize, t: f64) -> Vec<f64> {
    if degree == 0 {
        return vec![0.0];
    }
    let lower = legacy_basis(knots, degree - 1, span, t);
    let lower_start = span - (degree - 1);
    (0..=degree)
        .map(|local| {
            let index = span - degree + local;
            let lower_at = |global: usize| {
                global
                    .checked_sub(lower_start)
                    .and_then(|at| lower.get(at))
                    .copied()
                    .unwrap_or(0.0)
            };
            let left_denominator = knots[index + degree] - knots[index];
            let right_denominator = knots[index + degree + 1] - knots[index + 1];
            let degree = f64_from_index(degree).expect("small reference degree");
            let left = if left_denominator == 0.0 {
                0.0
            } else {
                degree * lower_at(index) / left_denominator
            };
            let right = if right_denominator == 0.0 {
                0.0
            } else {
                degree * lower_at(index + 1) / right_denominator
            };
            left - right
        })
        .collect()
}
fn legacy_curve<const N: usize>(
    knots: &[f64],
    poles: &[[f64; N]],
    weights: &[f64],
    t: f64,
) -> [f64; N] {
    let degree = knots.len() - poles.len() - 1;
    let span = legacy_span(knots, degree, poles.len(), t);
    let basis = legacy_basis(knots, degree, span, t);
    let mut weighted = [0.0; N];
    let mut weight_sum = 0.0;
    for (i, value) in basis.iter().enumerate() {
        let index = span - degree + i;
        for (axis, coordinate) in poles[index].iter().enumerate() {
            weighted[axis] += value * weights[index] * coordinate;
        }
        weight_sum += value * weights[index];
    }
    weighted.map(|value| value / weight_sum)
}
fn legacy_surface(
    knots: &[f64],
    poles: &[Vec<Point3>],
    weights: &[Vec<f64>],
    u: f64,
    v: f64,
) -> [[f64; 3]; 3] {
    let degree = 2;
    let u_span = legacy_span(knots, degree, poles.len(), u);
    let v_span = legacy_span(knots, degree, poles[0].len(), v);
    let ub = legacy_basis(knots, degree, u_span, u);
    let vb = legacy_basis(knots, degree, v_span, v);
    let ud = legacy_derivative(knots, degree, u_span, u);
    let vd = legacy_derivative(knots, degree, v_span, v);
    let mut weighted = [[0.0; 3]; 3];
    let mut weight = [0.0; 3];
    for i in 0..=degree {
        for j in 0..=degree {
            let row = u_span - degree + i;
            let column = v_span - degree + j;
            let pole_weight = weights[row][column];
            let factors = [
                ub[i] * vb[j] * pole_weight,
                ud[i] * vb[j] * pole_weight,
                ub[i] * vd[j] * pole_weight,
            ];
            for (order, factor) in factors.into_iter().enumerate() {
                for (axis, coordinate) in coordinates(poles[row][column]).into_iter().enumerate() {
                    weighted[order][axis] += factor * coordinate;
                }
                weight[order] += factor;
            }
        }
    }
    let point = weighted[0].map(|value| value / weight[0]);
    [
        point,
        std::array::from_fn(|axis| (weighted[1][axis] - point[axis] * weight[1]) / weight[0]),
        std::array::from_fn(|axis| (weighted[2][axis] - point[axis] * weight[2]) / weight[0]),
    ]
}
fn legacy_offset(mut base: Point3, terms: &[(f64, Vector3)]) -> Point3 {
    for (factor, direction) in terms {
        base.x += factor * direction.x;
        base.y += factor * direction.y;
        base.z += factor * direction.z;
    }
    base
}

fn legacy_cross(first: Vector3, second: Vector3) -> Vector3 {
    Vector3::new(
        first.y * second.z - first.z * second.y,
        first.z * second.x - first.x * second.z,
        first.x * second.y - first.y * second.x,
    )
}

#[test]
fn fast_analytic_cross_products_keep_v05_rounding() {
    use crate::scalar::{NonZeroLength, PositiveLength};
    use crate::units::{OrthonormalFrame3, UnitVector3};

    // Cancellation in this oblique frame triggers the default cross product's
    // exact-product fallback. Version 0.5 rounds the two products separately.
    let axis = Vector3::new(
        0.348_850_675_847_246_2,
        0.464_944_367_939_430_5,
        0.813_713_672_419_434_7,
    );
    let reference = Vector3::new(
        0.488_355_775_190_668_64,
        0.650_875_239_596_840_2,
        -0.581_265_911_023_239_8,
    );
    let transverse = legacy_cross(axis, reference);
    let exact_transverse = axis.cross(reference);
    assert_ne!(transverse.z.to_bits(), exact_transverse.z.to_bits());
    let frame = OrthonormalFrame3::from_units(
        UnitVector3::new(axis).unwrap(),
        UnitVector3::new(reference).unwrap(),
    )
    .unwrap();
    let origin = FinitePoint3::new(Point3::new(0.0, 0.0, 0.0)).unwrap();
    let radius = PositiveLength::new(1.0).unwrap();
    let circle = SolvedCurveGeometry::Circle(CircleCurve::new(origin, frame, radius));
    let sphere = SolvedSurfaceGeometry::Sphere(SphereSurface::new(
        origin,
        frame,
        NonZeroLength::new(1.0).unwrap(),
    ));
    let mut curve_differs_from_exact = false;
    for t in [-2.7_f64, 0.13, 0.71, std::f64::consts::FRAC_PI_2, 3.9] {
        let expected = legacy_offset(origin.get(), &[(t.cos(), reference), (t.sin(), transverse)]);
        let fast =
            decode::curve_point_solved_with_precision(Standard, &circle, t, EvalPrecision::Fast)
                .unwrap();
        let exact = decode::curve_point_solved(Standard, &circle, t).unwrap();
        assert_eq!(bits(coordinates(fast.get())), bits(coordinates(expected)));
        curve_differs_from_exact |= bits(coordinates(fast.get())) != bits(coordinates(exact.get()));
    }
    assert!(curve_differs_from_exact);

    let SolvedSurfaceGeometry::Sphere(carrier) = &sphere else {
        unreachable!()
    };
    let axis = *carrier.frame().axis().as_raw();
    let reference = *carrier.frame().reference().as_raw();
    let transverse = legacy_cross(axis, reference);
    let exact_transverse = axis.cross(reference);
    let mut inverse_detects_cross_rounding = false;
    let mut inverse_differs_from_exact = false;
    for point in [
        Point3::new(1.0, 2.0, 3.0),
        Point3::new(4.68, -0.162_462_015_215_154_2, 1_000_000_000_036.0),
        Point3::new(-1.3, 0.9, -0.2),
    ] {
        let project = |direction: Vector3| {
            point.x * direction.x + point.y * direction.y + point.z * direction.z
        };
        let (x, y, z) = (project(reference), project(transverse), project(axis));
        let expected = [y.atan2(x), z.atan2(x.hypot(y))];
        let wrong_y = project(exact_transverse);
        let wrong_cross = [wrong_y.atan2(x), z.atan2(x.hypot(wrong_y))];
        let fast =
            analytic_surface_parameters_solved_with_precision(&sphere, point, EvalPrecision::Fast)
                .unwrap();
        let exact = analytic_surface_parameters_solved(&sphere, point).unwrap();
        assert_eq!(bits([fast.u, fast.v]), bits(expected));
        inverse_detects_cross_rounding |= bits(expected) != bits(wrong_cross);
        inverse_differs_from_exact |= bits([fast.u, fast.v]) != bits([exact.u, exact.v]);
    }
    assert!(inverse_detects_cross_rounding);
    assert!(inverse_differs_from_exact);
}

#[test]
fn fast_basis_matches_v05_at_knots_and_above_inline_support() {
    for degree in [0, 1, 2, 3, 8, 15, 16, 23] {
        let support = degree + 1;
        let knots: Vec<f64> = std::iter::repeat_n(0.0, support)
            .chain([0.4, 0.4, 0.7])
            .chain(std::iter::repeat_n(1.0, support))
            .collect();
        for t in [0.0, 0.13, 0.4, 0.63, 0.7, 0.91, 1.0] {
            let span = legacy_span(&knots, degree, support + 3, t);
            let scratch = decode::Scratch::new(Standard).with_precision(EvalPrecision::Fast);
            let actual = basis::bspline_basis(&scratch, &knots, degree, span, t).unwrap();
            let expected = legacy_basis(&knots, degree, span, t);
            assert_eq!(
                actual
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                "degree={degree}, t={t}"
            );
        }
    }
}

#[test]
fn fast_rational_curve_and_pcurve_match_v05() {
    let ctx = cadmpeg_test_support::service_decode_context();
    let knots = vec![0.0, 0.0, 0.0, 0.4, 0.7, 1.0, 1.0, 1.0];
    let poles = vec![
        Point3::new(1.2, -0.7, 3.1),
        Point3::new(2.3, 5.1, -1.4),
        Point3::new(-3.7, 2.1, 0.6),
        Point3::new(8.2, -9.1, 4.3),
        Point3::new(2.7, 1.9, -5.2),
    ];
    let weights = vec![1.0, 0.73, 1.4, 0.91, 1.2];
    let uv: Vec<_> = poles
        .iter()
        .map(|point| Point2::new(point.x, point.y))
        .collect();
    let curve = NurbsCurve::from_lanes(
        &ctx,
        2,
        knots.clone(),
        poles.clone(),
        Some(weights.clone()),
        false,
    )
    .unwrap()
    .unwrap();
    let pcurve = PcurveGeometry::Nurbs {
        nurbs: PcurveNurbs::from_lanes(
            &ctx,
            2,
            knots.clone(),
            uv.clone(),
            Some(weights.clone()),
            false,
        )
        .unwrap()
        .unwrap(),
    };
    let raw: Vec<_> = poles.iter().copied().map(coordinates).collect();
    for t in [0.0, 0.13, 0.4, 0.63, 0.7, 0.91, 1.0] {
        let expected = legacy_curve(&knots, &raw, &weights, t);
        let actual =
            decode::nurbs_curve_point_at_with_precision(Standard, &curve, t, EvalPrecision::Fast)
                .unwrap();
        assert_eq!(bits(coordinates(actual.get())), bits(expected));
        let solved = SolvedCurveGeometry::Nurbs(curve.clone());
        assert_eq!(
            decode::curve_point_solved_with_precision(Standard, &solved, t, EvalPrecision::Fast)
                .unwrap(),
            actual
        );
        let actual_uv =
            decode::pcurve_uv_with_precision(Standard, &pcurve, t, EvalPrecision::Fast).unwrap();
        assert_eq!(
            bits([actual_uv.u, actual_uv.v]),
            bits([expected[0], expected[1]])
        );
        assert_eq!(
            nurbs_pcurve_uv_with_precision(
                &ctx,
                2,
                &knots,
                &uv,
                Some(&weights),
                t,
                EvalPrecision::Fast
            )
            .unwrap(),
            actual_uv
        );
        exact_default!(
            decode::nurbs_curve_point_at,
            decode::nurbs_curve_point_at_with_precision,
            Standard,
            &curve,
            t
        );
        exact_default!(
            decode::curve_point_solved,
            decode::curve_point_solved_with_precision,
            Standard,
            &solved,
            t
        );
        exact_default!(
            decode::pcurve_uv,
            decode::pcurve_uv_with_precision,
            Standard,
            &pcurve,
            t
        );
        exact_default!(
            nurbs_pcurve_uv,
            nurbs_pcurve_uv_with_precision,
            &ctx,
            2,
            &knots,
            &uv,
            Some(&weights),
            t
        );
    }
    let periodic_knots: Vec<_> = knots.iter().map(|knot| 0.3 + 1.7 * knot).collect();
    let periodic = NurbsCurve::from_lanes(
        &ctx,
        2,
        periodic_knots.clone(),
        poles,
        Some(weights.clone()),
        true,
    )
    .unwrap()
    .unwrap();
    let solved = SolvedCurveGeometry::Nurbs(periodic.clone());
    let lower = periodic_knots[2];
    let upper = periodic_knots[raw.len()];
    for t in [lower, upper, lower - 0.13, upper + 0.27, -7.9, 6.2] {
        let mapped = lower + (t - lower).rem_euclid(upper - lower);
        let expected = legacy_curve(&periodic_knots, &raw, &weights, mapped);
        let actual =
            decode::curve_point_solved_with_precision(Standard, &solved, t, EvalPrecision::Fast)
                .unwrap();
        assert_eq!(
            bits(coordinates(actual.get())),
            bits(expected),
            "periodic t={t}"
        );
    }
    // Raw evaluation reads the endpoint directly instead of wrapping its phase.
    let endpoint = decode::nurbs_curve_point_at_with_precision(
        Standard,
        &periodic,
        upper,
        EvalPrecision::Fast,
    )
    .unwrap();
    let expected = legacy_curve(&periodic_knots, &raw, &weights, upper);
    assert_eq!(bits(coordinates(endpoint.get())), bits(expected));
    assert_ne!(
        bits(expected),
        bits(legacy_curve(&periodic_knots, &raw, &weights, lower))
    );
}

#[test]
fn fast_surface_point_and_partials_match_v05() {
    let ctx = cadmpeg_test_support::service_decode_context();
    let knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let poles: Vec<Vec<_>> = (0..3)
        .map(|u| {
            (0..3)
                .map(|v| {
                    Point3::new(
                        0.3 + f64::from(u) * 1.7,
                        -0.8 + f64::from(v) * 2.3,
                        f64::from(u * v) * 0.7,
                    )
                })
                .collect()
        })
        .collect();
    let weights: Vec<Vec<_>> = (0..3)
        .map(|u| (0..3).map(|v| 0.7 + f64::from(u + 2 * v) * 0.13).collect())
        .collect();
    for rational in [false, true] {
        let weights = if rational {
            weights.clone()
        } else {
            vec![vec![1.0; 3]; 3]
        };
        let surface = NurbsSurface::from_lanes(
            &ctx,
            NurbsSurfaceAxis::new(2, knots.clone(), false),
            NurbsSurfaceAxis::new(2, knots.clone(), false),
            NurbsSurfaceLanes::new(poles.clone(), rational.then(|| weights.clone())),
            false,
        )
        .unwrap()
        .unwrap();
        for (u, v) in [
            (0.0, 0.0),
            (0.13, 0.67),
            (0.5, 0.5),
            (0.91, 0.27),
            (1.0, 1.0),
        ] {
            let expected = legacy_surface(&knots, &poles, &weights, u, v);
            let point = decode::nurbs_surface_point_with_precision(
                Standard,
                &surface,
                u,
                v,
                EvalPrecision::Fast,
            )
            .unwrap();
            assert_eq!(bits(coordinates(point.get())), bits(expected[0]));
            let partials = nurbs_surface_partials_with_precision(
                Standard,
                &surface,
                u,
                v,
                EvalPrecision::Fast,
            )
            .unwrap();
            assert_eq!(bits(coordinates(partials.point.get())), bits(expected[0]));
            assert_eq!(
                bits([partials.du.x, partials.du.y, partials.du.z]),
                bits(expected[1])
            );
            assert_eq!(
                bits([partials.dv.x, partials.dv.y, partials.dv.z]),
                bits(expected[2])
            );
            let solved = SolvedSurfaceGeometry::Nurbs(surface.clone());
            assert_eq!(
                decode::surface_point_solved_with_precision(
                    Standard,
                    &solved,
                    u,
                    v,
                    EvalPrecision::Fast
                )
                .unwrap(),
                point
            );
            exact_default!(
                decode::nurbs_surface_point,
                decode::nurbs_surface_point_with_precision,
                Standard,
                &surface,
                u,
                v
            );
            exact_default!(
                decode::surface_point_solved,
                decode::surface_point_solved_with_precision,
                Standard,
                &solved,
                u,
                v
            );
            exact_default!(
                nurbs_surface_partials,
                nurbs_surface_partials_with_precision,
                Standard,
                &surface,
                u,
                v
            );
        }
    }
}

#[test]
fn fast_analytic_curves_match_v05_ordered_offsets() {
    let origin = Point3::new(1.2, -3.7, 0.1);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    let reference = Vector3::new(0.6, 0.8, 0.0);
    let curves = [
        SolvedCurveGeometry::Line(LineCurve::try_new(origin, reference).unwrap()),
        SolvedCurveGeometry::Circle(CircleCurve::try_new(origin, axis, reference, 3.7).unwrap()),
        SolvedCurveGeometry::Ellipse(
            EllipseCurve::try_new(origin, axis, reference, 3.7, 1.9).unwrap(),
        ),
    ];
    for curve in curves {
        for t in [-2.7_f64, 0.0, 0.13, 0.71, 3.9] {
            let expected = match &curve {
                SolvedCurveGeometry::Line(line) => {
                    legacy_offset(line.origin().get(), &[(t, *line.direction().as_raw())])
                }
                SolvedCurveGeometry::Circle(circle) => legacy_offset(
                    circle.center().get(),
                    &[
                        (
                            circle.radius().get() * t.cos(),
                            *circle.frame().reference().as_raw(),
                        ),
                        (
                            circle.radius().get() * t.sin(),
                            circle
                                .frame()
                                .axis()
                                .as_raw()
                                .cross(*circle.frame().reference().as_raw()),
                        ),
                    ],
                ),
                SolvedCurveGeometry::Ellipse(ellipse) => legacy_offset(
                    ellipse.center().get(),
                    &[
                        (
                            ellipse.major_radius().get() * t.cos(),
                            *ellipse.frame().reference().as_raw(),
                        ),
                        (
                            ellipse.minor_radius().get() * t.sin(),
                            ellipse
                                .frame()
                                .axis()
                                .as_raw()
                                .cross(*ellipse.frame().reference().as_raw()),
                        ),
                    ],
                ),
                _ => unreachable!(),
            };
            let point =
                decode::curve_point_solved_with_precision(Standard, &curve, t, EvalPrecision::Fast)
                    .unwrap();
            assert_eq!(bits(coordinates(point.get())), bits(coordinates(expected)));
            exact_default!(
                decode::curve_point_solved,
                decode::curve_point_solved_with_precision,
                Standard,
                &curve,
                t
            );
        }
    }
}

#[test]
fn fast_analytic_surface_lifts_and_inverses_match_v05() {
    let origin = Point3::new(1.2, -3.7, 0.1);
    let axis = Vector3::new(0.0, 0.0, 1.0);
    let reference = Vector3::new(0.6, 0.8, 0.0);
    let surfaces = [
        SolvedSurfaceGeometry::Plane(PlaneSurface::try_new(origin, axis, reference).unwrap()),
        SolvedSurfaceGeometry::Cylinder(
            CylinderSurface::try_new(origin, axis, reference, 3.7).unwrap(),
        ),
        SolvedSurfaceGeometry::Sphere(
            SphereSurface::try_new(origin, axis, reference, 3.7).unwrap(),
        ),
        SolvedSurfaceGeometry::Torus(
            TorusSurface::try_new(origin, axis, reference, 3.7, 1.9).unwrap(),
        ),
    ];
    for surface in surfaces {
        for (u, v) in [(0.13_f64, -0.27_f64), (0.71, 0.39), (-1.3, 0.91)] {
            let frame = match &surface {
                SolvedSurfaceGeometry::Plane(value) => value.frame(),
                SolvedSurfaceGeometry::Cylinder(value) => value.frame(),
                SolvedSurfaceGeometry::Sphere(value) => value.frame(),
                SolvedSurfaceGeometry::Torus(value) => value.frame(),
                _ => unreachable!(),
            };
            let axis = *frame.axis().as_raw();
            let reference = *frame.reference().as_raw();
            let transverse = axis.cross(reference);
            let terms = match &surface {
                SolvedSurfaceGeometry::Plane(_) => vec![(u, reference), (v, transverse)],
                SolvedSurfaceGeometry::Cylinder(_) => vec![
                    (3.7 * u.cos(), reference),
                    (3.7 * u.sin(), transverse),
                    (v, axis),
                ],
                SolvedSurfaceGeometry::Sphere(_) => vec![
                    (3.7 * v.cos() * u.cos(), reference),
                    (3.7 * v.cos() * u.sin(), transverse),
                    (3.7 * v.sin(), axis),
                ],
                SolvedSurfaceGeometry::Torus(_) => vec![
                    ((3.7 + 1.9 * v.cos()) * u.cos(), reference),
                    ((3.7 + 1.9 * v.cos()) * u.sin(), transverse),
                    (1.9 * v.sin(), axis),
                ],
                _ => unreachable!(),
            };
            let expected = legacy_offset(origin, &terms);
            let point = decode::surface_point_solved_with_precision(
                Standard,
                &surface,
                u,
                v,
                EvalPrecision::Fast,
            )
            .unwrap();
            assert_eq!(bits(coordinates(point.get())), bits(coordinates(expected)));
            let delta = Vector3::new(
                expected.x - origin.x,
                expected.y - origin.y,
                expected.z - origin.z,
            );
            let project = |direction: Vector3| {
                delta.x * direction.x + delta.y * direction.y + delta.z * direction.z
            };
            let (x, y, z) = (project(reference), project(transverse), project(axis));
            let expected_uv = match &surface {
                SolvedSurfaceGeometry::Plane(_) => [x, y],
                SolvedSurfaceGeometry::Cylinder(_) => [(y / 3.7).atan2(x / 3.7), z],
                SolvedSurfaceGeometry::Sphere(_) => [y.atan2(x), z.atan2(x.hypot(y))],
                SolvedSurfaceGeometry::Torus(_) => {
                    [y.atan2(x), (z / 1.9).atan2((x.hypot(y) - 3.7) / 1.9)]
                }
                _ => unreachable!(),
            };
            let uv = analytic_surface_parameters_solved_with_precision(
                &surface,
                expected,
                EvalPrecision::Fast,
            )
            .unwrap();
            assert_eq!(bits([uv.u, uv.v]), bits(expected_uv));
            exact_default!(
                analytic_surface_parameters_solved,
                analytic_surface_parameters_solved_with_precision,
                &surface,
                expected
            );
            exact_default!(
                decode::surface_point_solved,
                decode::surface_point_solved_with_precision,
                Standard,
                &surface,
                u,
                v
            );
        }
    }
}

#[test]
fn fast_keeps_work_and_allocation_refusals() {
    let surface = test_support::high_degree_plane();
    for dimension in [
        ResourceDimension::WorkUnits,
        ResourceDimension::CollectionItems,
    ] {
        let mut policy = DecodePolicy::service();
        match dimension {
            ResourceDimension::WorkUnits => policy.limits.max_work_units = 0,
            ResourceDimension::CollectionItems => policy.limits.max_collection_items = 0,
            _ => unreachable!(),
        }
        let arena = DecodeArena::new();
        let (ctx, _) = DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
        let failure = decode::nurbs_surface_point_with_precision(
            &ctx,
            &surface,
            0.37,
            0.63,
            EvalPrecision::Fast,
        )
        .unwrap_err();
        assert!(
            matches!(failure, EvaluationFailure::ResourceLimit(limit) if limit.dimension == dimension && limit.limit == 0)
        );
        assert!(
            matches!(ctx.finish_session(), Err(CodecError::ResourceLimit(limit)) if limit.dimension == dimension)
        );
    }
    assert_eq!(EvalPrecision::default(), EvalPrecision::Exact);
}

#[test]
fn fast_analytic_pcurves_match_v05_ordered_offsets() {
    let center = Point2::new(0.125, -0.125);
    let x = Point2::new(0.6, 0.8);
    let y = Point2::new(-0.8, 0.6);
    // Cancellation distinguishes the ordered binary64 additions from Exact.
    let cosine = Point2::new(9_007_199_254_740_992.0, -9_007_199_254_740_992.0);
    let sine = Point2::new(-9_007_199_254_740_992.0, 9_007_199_254_740_992.0);
    let fixtures = [
        (
            PcurveGeometry::Circle(CirclePcurve::try_new(center, x, y, 3.7).unwrap()),
            3.7,
            3.7,
            x,
            y,
        ),
        (
            PcurveGeometry::Ellipse(EllipsePcurve::try_new(center, x, y, 3.7, 1.9).unwrap()),
            3.7,
            1.9,
            x,
            y,
        ),
        (
            PcurveGeometry::Harmonic(HarmonicPcurve::try_new(center, cosine, sine).unwrap()),
            1.0,
            1.0,
            cosine,
            sine,
        ),
    ];
    let mut differs_from_exact = false;
    for (geometry, major, minor, x, y) in fixtures {
        for t in [-2.7_f64, 0.0, 0.13, std::f64::consts::FRAC_PI_4, 3.9] {
            // v0.5.5 offset2 folds terms in this order.
            let mut expected = center;
            for (factor, direction) in [(major * t.cos(), x), (minor * t.sin(), y)] {
                expected.u += factor * direction.u;
                expected.v += factor * direction.v;
            }
            let fast =
                decode::pcurve_uv_with_precision(Standard, &geometry, t, EvalPrecision::Fast)
                    .unwrap();
            assert_eq!(bits([fast.u, fast.v]), bits([expected.u, expected.v]));
            let exact =
                decode::pcurve_uv_with_precision(Standard, &geometry, t, EvalPrecision::Exact)
                    .unwrap();
            differs_from_exact |= bits([fast.u, fast.v]) != bits([exact.u, exact.v]);
            assert_eq!(decode::pcurve_uv(Standard, &geometry, t).unwrap(), exact);
        }
    }
    assert!(
        differs_from_exact,
        "fixtures must exercise the precision selection"
    );
}
