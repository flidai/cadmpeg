// SPDX-License-Identifier: Apache-2.0
//! Caller admission for stored carrier evaluation scratch.

use cadmpeg_core::decode::{u64_from_index, DecodeContext};
use cadmpeg_core::CodecError;
use cadmpeg_ir::features::FinitePoint3;
use cadmpeg_ir::geometry::nurbs::NurbsCurve;
use cadmpeg_ir::geometry::CurveGeometry;

use crate::lane_refusal::SolverGate;

pub(super) fn curve_point(
    ctx: &DecodeContext<'_>,
    curve: &CurveGeometry,
    parameter: f64,
) -> Result<Option<FinitePoint3>, CodecError> {
    Ok(cadmpeg_ir::eval::finite_or_refusal(
        cadmpeg_ir::eval::decode::outer_refusal(cadmpeg_ir::eval::decode::curve_point(
            ctx, curve, parameter,
        ))?,
    )?)
}

pub(super) fn nurbs_curve_point(
    ctx: &DecodeContext<'_>,
    curve: &NurbsCurve,
    parameter: f64,
) -> Result<Option<FinitePoint3>, CodecError> {
    Ok(cadmpeg_ir::eval::finite_or_refusal(
        cadmpeg_ir::eval::decode::outer_refusal(cadmpeg_ir::eval::decode::nurbs_curve_point_at(
            ctx, curve, parameter,
        ))?,
    )?)
}

pub(crate) fn nurbs_surface_point(
    ctx: &DecodeContext<'_>,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    u: f64,
    v: f64,
) -> Result<Option<FinitePoint3>, CodecError> {
    Ok(cadmpeg_ir::eval::finite_or_refusal(
        cadmpeg_ir::eval::decode::outer_refusal(cadmpeg_ir::eval::decode::nurbs_surface_point(
            ctx, surface, u, v,
        ))?,
    )?)
}

pub(crate) fn nurbs_surface_parameter_near_point(
    ctx: &DecodeContext<'_>,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    point: cadmpeg_ir::math::Point3,
    seed: Option<cadmpeg_ir::math::Point2>,
) -> Result<Option<cadmpeg_ir::units::FinitePoint2>, CodecError> {
    Ok(cadmpeg_ir::eval::nurbs_surface_parameter_near_point(
        ctx, surface, point, seed,
    )?)
}

pub(crate) fn nurbs_surface_partials(
    ctx: &DecodeContext<'_>,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    u: f64,
    v: f64,
) -> Result<
    Option<cadmpeg_ir::eval::SurfacePartials<FinitePoint3, cadmpeg_ir::features::FiniteVector3>>,
    CodecError,
> {
    Ok(cadmpeg_ir::eval::finite_or_refusal(
        cadmpeg_ir::eval::nurbs_surface_partials(ctx, surface, u, v),
    )?)
}

pub(crate) fn surface_point(
    ctx: &DecodeContext<'_>,
    surface: &cadmpeg_ir::geometry::SurfaceGeometry,
    u: f64,
    v: f64,
) -> Result<Option<FinitePoint3>, CodecError> {
    Ok(cadmpeg_ir::eval::finite_or_refusal(
        cadmpeg_ir::eval::decode::outer_refusal(cadmpeg_ir::eval::decode::surface_point(
            ctx, surface, u, v,
        ))?,
    )?)
}

pub(super) fn charge_nurbs_isocurve_comparison(
    ctx: &DecodeContext<'_>,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    curve: &NurbsCurve,
) -> Result<(), CodecError> {
    const OPERATION: &str = "compare SLDPRT NURBS isocurve";
    let work = u64_from_index(surface.u_count())
        .checked_mul(u64_from_index(surface.v_count()))
        .and_then(|count| count.checked_add(u64_from_index(curve.pole_count())))
        .and_then(|count| count.checked_add(u64_from_index(surface.u_knots().len())))
        .and_then(|count| count.checked_add(u64_from_index(surface.v_knots().len())))
        .and_then(|count| count.checked_add(u64_from_index(curve.knots().len())))
        .and_then(|count| count.checked_mul(512))
        .ok_or_else(|| ctx.refuse_codec_limit(OPERATION, u64::MAX - 1, u64::MAX))?;
    ctx.charge_work(work, OPERATION)
}

/// Local work floor of one surface solver call, in normalized evaluator units.
const SURFACE_SOLVER_LOCAL_WORK: u64 = 1_000_000;

/// Local work one surface span adds to the floor.
const SURFACE_SOLVER_WORK_PER_SPAN: u64 = 1_024;

/// Local work ceiling of one surface solver call. The ceiling bounds the wall
/// time of one call, which the session total does not.
const SURFACE_SOLVER_LOCAL_CEILING: u64 = 8_000_000;

/// What a budgeted surface solver call produced.
///
/// The solvers are optional: a surface that they cannot serve loses the
/// parameter or the chord bound that they derive, and the geometry stays
/// otherwise intact. `Withheld` states that the call did not run, or ran out of
/// its slice. It leaves the session unfused, so the decode goes on and the
/// caller reports the loss.
#[derive(Debug)]
pub(super) enum Solved<T> {
    /// The solver ran and produced this result.
    Done(T),
    /// The solver did not run or ran out of its work.
    Withheld,
}

/// Spans of a NURBS surface: control points beyond the degree on each axis.
fn surface_spans(
    ctx: &DecodeContext<'_>,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    operation: &'static str,
) -> Result<u64, CodecError> {
    let spans =
        |count: usize, degree: u32| match u64_from_index(count).checked_sub(u64::from(degree)) {
            Some(spans) if spans > 0 => spans,
            _ => 1,
        };
    spans(surface.u_count(), surface.u_degree())
        .checked_mul(spans(surface.v_count(), surface.v_degree()))
        .ok_or_else(|| ctx.refuse_codec_limit(operation, u64::MAX - 1, u64::MAX))
}

/// The local work slice of one solver call, or `None` when the session has no
/// work left to pay for even one unit.
///
/// The local ceiling grows with the number of spans of the surface, because the
/// solver searches every span. One normalized evaluator unit also pays for
/// fixed arithmetic and a complete scan of both knot vectors, so the session
/// cost of a unit grows with the knot count. The slice never exceeds the work
/// that the session has left, so a call that starts cannot fuse the session. A
/// call that ran to completion under the former fixed slice still runs to
/// completion: the slice is never smaller than that slice unless the session
/// itself has less left.
fn surface_solver_budget<'ctx>(
    ctx: &'ctx DecodeContext<'_>,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    operation: &'static str,
) -> Result<Option<cadmpeg_core::decode::WorkBudget<'ctx>>, CodecError> {
    let scale = 64_u64
        .checked_add(u64_from_index(surface.u_knots().len()))
        .and_then(|scale| scale.checked_add(u64_from_index(surface.v_knots().len())))
        .and_then(std::num::NonZeroU64::new)
        .ok_or_else(|| ctx.refuse_codec_limit(operation, u64::MAX - 1, u64::MAX))?;
    let wanted = match surface_spans(ctx, surface, operation)?
        .checked_mul(SURFACE_SOLVER_WORK_PER_SPAN)
    {
        Some(work) if work < SURFACE_SOLVER_LOCAL_CEILING => work.max(SURFACE_SOLVER_LOCAL_WORK),
        _ => SURFACE_SOLVER_LOCAL_CEILING,
    };
    let affordable = ctx.work_remaining() / scale.get();
    if affordable == 0 {
        return Ok(None);
    }
    Ok(Some(
        ctx.work_budget(wanted.min(affordable))
            .with_session_work_scale(scale),
    ))
}

pub(super) fn nurbs_surface_parameter_within_tolerance(
    ctx: &DecodeContext<'_>,
    gate: &SolverGate,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    point: cadmpeg_ir::math::Point3,
    seed: Option<cadmpeg_ir::math::Point2>,
    tolerance: f64,
) -> Result<Solved<Option<cadmpeg_ir::units::FinitePoint2>>, CodecError> {
    const OPERATION: &str = "invert SLDPRT NURBS surface globally";
    if !gate.is_open() {
        return Ok(Solved::Withheld);
    }
    let Some(budget) = surface_solver_budget(ctx, surface, OPERATION)? else {
        return Ok(Solved::Withheld);
    };
    let result = cadmpeg_ir::eval::nurbs_surface_parameter_within_tolerance_with_budget(
        ctx, surface, point, seed, tolerance, &budget,
    );
    // A zero charge observes the session's original sticky refusal.
    ctx.charge_work(0, OPERATION)?;
    if budget.exhausted() {
        gate.note_exhausted();
        return Ok(Solved::Withheld);
    }
    Ok(Solved::Done(result?))
}

pub(super) fn nurbs_surface_parameter_segment_chord_bound(
    ctx: &DecodeContext<'_>,
    gate: &SolverGate,
    surface: &cadmpeg_ir::geometry::nurbs::NurbsSurface,
    parameters: [cadmpeg_ir::math::Point2; 2],
    chord: [cadmpeg_ir::math::Point3; 2],
) -> Result<Solved<Option<f64>>, CodecError> {
    const OPERATION: &str = "certify SLDPRT NURBS surface chord";
    if !gate.is_open() {
        return Ok(Solved::Withheld);
    }
    let u = u64::from(surface.u_degree())
        .checked_add(1)
        .ok_or_else(|| ctx.refuse_codec_limit(OPERATION, u64::MAX - 1, u64::MAX))?;
    let v = u64::from(surface.v_degree())
        .checked_add(1)
        .ok_or_else(|| ctx.refuse_codec_limit(OPERATION, u64::MAX - 1, u64::MAX))?;
    let patches = u64_from_index(surface.u_count())
        .checked_mul(u64_from_index(surface.v_count()))
        .ok_or_else(|| ctx.refuse_codec_limit(OPERATION, u64::MAX - 1, u64::MAX))?;
    let splits = patches
        .checked_mul(4)
        .and_then(|count| count.checked_add(2))
        .ok_or_else(|| ctx.refuse_codec_limit(OPERATION, u64::MAX - 1, u64::MAX))?;
    // Restricting each tensor-product line costs at most its squared support
    // times the combined degrees. IR admits split scans and sorting.
    let work = u
        .checked_mul(v)
        .and_then(|support| support.checked_mul(support))
        .and_then(|work| work.checked_mul(u.checked_add(v)?))
        .and_then(|work| work.checked_mul(64))
        .and_then(|work| work.checked_mul(splits))
        .ok_or_else(|| ctx.refuse_codec_limit(OPERATION, u64::MAX - 1, u64::MAX))?;
    if work > ctx.work_remaining() {
        return Ok(Solved::Withheld);
    }
    ctx.charge_work(work, OPERATION)?;
    let Some(budget) = surface_solver_budget(ctx, surface, OPERATION)? else {
        return Ok(Solved::Withheld);
    };
    let result = cadmpeg_ir::eval::nurbs_surface_parameter_segment_chord_bound_with_budget(
        ctx, surface, parameters, chord, &budget,
    );
    ctx.charge_work(0, OPERATION)?;
    if budget.exhausted() {
        gate.note_exhausted();
        return Ok(Solved::Withheld);
    }
    result.map(Solved::Done)
}

#[cfg(test)]
mod tests {
    use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy, ResourceDimension};
    use cadmpeg_core::CodecError;
    use cadmpeg_ir::geometry::nurbs::{NurbsSurface, NurbsSurfaceAxis, NurbsSurfaceLanes};
    use cadmpeg_ir::math::Point3;

    fn bilinear_plane() -> NurbsSurface {
        let knots = vec![0.0, 0.0, 1.0, 1.0];
        NurbsSurface::from_lanes(
            &cadmpeg_test_support::service_decode_context(),
            NurbsSurfaceAxis::new(1, knots.clone(), false),
            NurbsSurfaceAxis::new(1, knots, false),
            NurbsSurfaceLanes::new(
                (0..2)
                    .map(|u| {
                        (0..2)
                            .map(|v| Point3::new(f64::from(u), f64::from(v), 0.0))
                            .collect()
                    })
                    .collect(),
                None,
            ),
            false,
        )
        .expect("fixture admission")
        .expect("bilinear plane")
    }

    #[test]
    fn solvers_run_when_the_session_can_pay_for_them() {
        let surface = bilinear_plane();
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::service()).unwrap();
        let point = Point3::new(0.25, 0.75, 0.0);
        let Ok(super::Solved::Done(Some(parameters))) =
            super::nurbs_surface_parameter_within_tolerance(
                &ctx,
                &crate::lane_refusal::SolverGate::default(),
                &surface,
                point,
                None,
                1e-6,
            )
        else {
            panic!("the inversion must run")
        };
        assert!((parameters.get().u - 0.25).abs() < 1e-6);
        ctx.finish_session().unwrap();
    }

    #[test]
    fn solvers_that_the_session_cannot_pay_for_are_withheld_without_fusing_it() {
        let surface = bilinear_plane();
        let arena = DecodeArena::new();
        let mut policy = DecodePolicy::service();
        policy.limits.max_work_units = 50;
        let (ctx, _) = DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
        let before = ctx.work_remaining();
        let point = Point3::new(0.25, 0.75, 0.0);
        assert!(matches!(
            super::nurbs_surface_parameter_within_tolerance(
                &ctx,
                &crate::lane_refusal::SolverGate::default(),
                &surface,
                point,
                None,
                1e-6,
            ),
            Ok(super::Solved::Withheld)
        ));
        let chord = [point, Point3::new(0.5, 0.5, 0.0)];
        let ends = [
            cadmpeg_ir::math::Point2::new(0.25, 0.75),
            cadmpeg_ir::math::Point2::new(0.5, 0.5),
        ];
        assert!(matches!(
            super::nurbs_surface_parameter_segment_chord_bound(
                &ctx,
                &crate::lane_refusal::SolverGate::default(),
                &surface,
                ends,
                chord,
            ),
            Ok(super::Solved::Withheld)
        ));
        assert_eq!(
            ctx.work_remaining(),
            before,
            "a withheld call charges nothing"
        );
        ctx.charge_work(1, "later work still runs").unwrap();
        ctx.finish_session().unwrap();
    }

    #[test]
    fn stored_surface_helpers_preserve_actual_admission_refusal() {
        // Degree 16 needs a heap basis; lower degrees evaluate inline.
        let mut knots = vec![0.0; 17];
        knots.extend(vec![1.0; 17]);
        let surface = NurbsSurface::from_lanes(
            &cadmpeg_test_support::service_decode_context(),
            NurbsSurfaceAxis::new(16, knots.clone(), false),
            NurbsSurfaceAxis::new(16, knots, false),
            NurbsSurfaceLanes::new(
                (0..17)
                    .map(|u| {
                        (0..17)
                            .map(|v| Point3::new(f64::from(u) / 16.0, f64::from(v) / 16.0, 0.0))
                            .collect()
                    })
                    .collect(),
                None,
            ),
            false,
        )
        .expect("fixture admission")
        .expect("degree-16 plane");
        for dimension in [
            ResourceDimension::MaterializedBytes,
            ResourceDimension::CollectionItems,
            ResourceDimension::WorkUnits,
        ] {
            let arena = DecodeArena::new();
            let mut policy = DecodePolicy::service();
            match dimension {
                ResourceDimension::MaterializedBytes => policy.limits.max_materialized_bytes = 0,
                ResourceDimension::CollectionItems => policy.limits.max_collection_items = 0,
                ResourceDimension::WorkUnits => policy.limits.max_work_units = 0,
                _ => unreachable!("three scratch dimensions"),
            }
            let (ctx, _) = DecodeContext::from_root_bytes(&[], &arena, &policy).unwrap();
            let CodecError::ResourceLimit(original) =
                super::nurbs_surface_partials(&ctx, &surface, 0.5, 0.5).unwrap_err()
            else {
                panic!("the partial helper must preserve its scratch refusal")
            };
            assert_eq!(original.dimension, dimension);
            assert_eq!((original.limit, original.used), (0, 0));
            assert!(original.additional > 0);
            assert_eq!(ctx.resource_refusal(), Some(original));
            assert!(
                matches!(super::nurbs_surface_parameter_near_point(&ctx, &surface, Point3::new(f64::NAN, 0.0, 0.0), None), Err(CodecError::ResourceLimit(limit)) if limit == original)
            );
        }
    }
}
