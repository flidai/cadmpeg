// SPDX-License-Identifier: Apache-2.0
//! The outer bound of a face whose curve-bounded surface leaves the outer
//! boundary implicit.
//!
//! A STEP face is the finite region to the left of each of its bounds, so the
//! natural boundary of the support must be written as a bound or the face
//! changes its domain.

use std::collections::HashMap;

use cadmpeg_core::decode::ResourceLimit;
use cadmpeg_ir::eval::admission::EvaluationAdmission;
use cadmpeg_ir::eval::{
    finite_or_refusal, model_surface_point, nurbs_surface_isoline, IsolineDirection,
};
use cadmpeg_ir::geometry::nurbs::{NurbsCurve, NurbsSurface};
use cadmpeg_ir::geometry::pcurve::{LinePcurve, PcurveGeometry};
use cadmpeg_ir::geometry::{
    ProceduralSurfaceDefinition, SolvedCurveGeometry, SolvedSurfaceGeometry,
};
use cadmpeg_ir::math::{Point2, Point3, Vector3};
use cadmpeg_ir::topology::{Face, LoopBoundaryRole, Sense};
use cadmpeg_ir::units::UnitVector3;

use super::Builder;
use crate::geometry;
use crate::writer::{refs, Ref};

/// What a face's implicit outer boundary needs in STEP.
enum ImplicitOuter<'a> {
    /// The face's loops state its whole boundary.
    Stated,
    /// The natural boundary is the parameter rectangle of a NURBS support.
    Rectangle(&'a NurbsSurface),
    /// The face has no loop on a complete sphere or torus.
    Closed(ClosedSurface),
    /// STEP has no bound for the natural boundary, for the stated reason.
    Unwritable(&'static str),
}

/// The STEP outer bound of a face whose outer boundary is implicit.
#[derive(Clone)]
pub(super) enum OuterBound {
    /// The parameter rectangle of the support.
    Rectangle(Box<ParameterRectangle>),
    /// The seam edges of a complete sphere or torus.
    Closed(ClosedSurface),
    /// STEP has no bound for the natural boundary, for the stated reason.
    Unwritable(&'static str),
}

/// A complete sphere or torus carrier and the frame its seam edges lie in.
#[derive(Clone, Copy)]
pub(super) struct ClosedSurface {
    /// Center of the sphere or torus.
    center: Point3,
    /// Polar axis.
    axis: UnitVector3,
    /// Direction of the `u = 0` meridian from the axis.
    reference: UnitVector3,
    /// Sphere radius, or torus major radius.
    radius: f64,
    /// Torus tube radius, absent for a sphere.
    tube_radius: Option<f64>,
    /// Whether the face runs with the carrier's `(u, v)` orientation.
    forward: bool,
}

/// The four sides of a NURBS parameter rectangle, ready to emit.
#[derive(Clone)]
pub(super) struct ParameterRectangle {
    /// The corners at `(u0, v0)`, `(u1, v0)`, `(u1, v1)` and `(u0, v1)`.
    corners: [Point3; 4],
    /// `[[u0, u1], [v0, v1]]`.
    domain: [[f64; 2]; 2],
    /// The isolines at `v0`, `u1`, `v1` and `u0`, each on the free parameter.
    sides: [NurbsCurve; 4],
    /// Whether the loop runs counterclockwise in `(u, v)`.
    counterclockwise: bool,
}

/// Corners of each side, as indices into [`ParameterRectangle::corners`], in
/// the direction of the side's free parameter.
const SIDE_CORNERS: [[usize; 2]; 4] = [[0, 1], [1, 2], [3, 2], [0, 3]];

impl<'a> Builder<'a> {
    /// The outer bound of every face whose outer boundary is implicit.
    pub(super) fn implicit_outer_bounds(
        &self,
    ) -> Result<HashMap<&'a str, OuterBound>, ResourceLimit> {
        let mut bounds = HashMap::new();
        for (id, face) in &self.faces {
            let bound = match self.implicit_outer(face) {
                ImplicitOuter::Stated => continue,
                ImplicitOuter::Closed(closed) => OuterBound::Closed(closed),
                ImplicitOuter::Rectangle(nurbs) => match self.parameter_rectangle(face, nurbs)? {
                    Ok(rectangle) => OuterBound::Rectangle(Box::new(rectangle)),
                    Err(reason) => OuterBound::Unwritable(reason),
                },
                ImplicitOuter::Unwritable(reason) => OuterBound::Unwritable(reason),
            };
            bounds.insert(*id, bound);
        }
        Ok(bounds)
    }

    /// The implicit outer boundary of `face`, from the curve-bounded
    /// construction of its surface.
    fn implicit_outer(&self, face: &Face) -> ImplicitOuter<'a> {
        if let Some(closed) = self.closed_surface(face) {
            return ImplicitOuter::Closed(closed);
        }
        let Some(procedural) = self.procedural_surfaces.get(face.surface.as_str()) else {
            return ImplicitOuter::Stated;
        };
        let ProceduralSurfaceDefinition::CurveBounded {
            support,
            implicit_outer: true,
            ..
        } = procedural.definition()
        else {
            return ImplicitOuter::Stated;
        };
        if face
            .loops
            .iter()
            .any(|id| face.loop_role(id) == LoopBoundaryRole::Outer)
        {
            return ImplicitOuter::Stated;
        }
        let support = self
            .surfaces
            .get(support.as_str())
            .and_then(|surface| surface.geometry.solved());
        let carrier = self
            .surfaces
            .get(face.surface.as_str())
            .and_then(|surface| surface.geometry.solved());
        let Some(support) = support else {
            return ImplicitOuter::Unwritable("the support has no solved carrier");
        };
        match geometry::emitted_basis(support) {
            SolvedSurfaceGeometry::Plane(_)
            | SolvedSurfaceGeometry::Cylinder(_)
            | SolvedSurfaceGeometry::Cone(_) => {
                ImplicitOuter::Unwritable("the support is unbounded")
            }
            SolvedSurfaceGeometry::Sphere(_) | SolvedSurfaceGeometry::Torus(_) => {
                ImplicitOuter::Stated
            }
            SolvedSurfaceGeometry::Nurbs(nurbs) if nurbs.u_periodic() && nurbs.v_periodic() => {
                ImplicitOuter::Stated
            }
            SolvedSurfaceGeometry::Nurbs(nurbs) if nurbs.u_periodic() || nurbs.v_periodic() => {
                ImplicitOuter::Unwritable("the support's natural boundary is two closed isolines")
            }
            SolvedSurfaceGeometry::Nurbs(nurbs) => match support {
                SolvedSurfaceGeometry::Nurbs(_) if carrier == Some(support) => {
                    ImplicitOuter::Rectangle(nurbs)
                }
                SolvedSurfaceGeometry::Nurbs(_) => {
                    ImplicitOuter::Unwritable("the face carrier is not the support")
                }
                _ => ImplicitOuter::Unwritable("the support is a placed NURBS surface"),
            },
            SolvedSurfaceGeometry::Polygonal(_)
            | SolvedSurfaceGeometry::Transformed(_)
            | SolvedSurfaceGeometry::Unknown { .. } => {
                ImplicitOuter::Unwritable("the support has no stated natural boundary")
            }
        }
    }

    /// The sphere or torus of a face that has no loop: the face is the whole
    /// closed surface. A tube radius not smaller than the major radius leaves
    /// the torus without a seam.
    fn closed_surface(&self, face: &Face) -> Option<ClosedSurface> {
        if !face.loops.is_empty() || self.procedural_surfaces.contains_key(face.surface.as_str()) {
            return None;
        }
        let forward = matches!(face.sense, Sense::Forward);
        match self
            .surfaces
            .get(face.surface.as_str())?
            .geometry
            .solved()?
        {
            SolvedSurfaceGeometry::Sphere(sphere) => Some(ClosedSurface {
                center: sphere.center().get(),
                axis: *sphere.frame().axis(),
                reference: *sphere.frame().reference(),
                radius: sphere.radius().get().abs(),
                tube_radius: None,
                forward,
            }),
            SolvedSurfaceGeometry::Torus(torus) => {
                let radius = torus.major_radius().get().abs();
                let tube_radius = torus.minor_radius().get().abs();
                (tube_radius < radius).then(|| ClosedSurface {
                    center: torus.center().get(),
                    axis: *torus.frame().axis(),
                    reference: *torus.frame().reference(),
                    radius,
                    tube_radius: Some(tube_radius),
                    forward,
                })
            }
            _ => None,
        }
    }

    /// Whether `face` is a complete closed surface, which bounds itself.
    pub(super) fn is_closed_surface(&self, face: &Face) -> bool {
        self.closed_surface(face).is_some()
    }

    /// Write the seam edges of a complete sphere or torus as a
    /// `FACE_OUTER_BOUND`.
    ///
    /// A sphere is bounded by its `u = 0` meridian from pole to pole, used once
    /// in each direction. A torus is bounded by its `u = 0` meridian circle and
    /// its outer equator, each closed at the vertex where they meet.
    pub(super) fn emit_closed_surface(
        &mut self,
        closed: ClosedSurface,
        bound_flag: &str,
    ) -> Option<Ref> {
        let ClosedSurface {
            center,
            axis,
            reference,
            radius,
            tube_radius,
            forward,
        } = closed;
        let at = |along_reference: f64, along_axis: f64, origin: Point3| {
            let (x, z) = (Vector3::from(reference), Vector3::from(axis));
            Point3::new(
                origin.x + along_reference * x.x + along_axis * z.x,
                origin.y + along_reference * x.y + along_axis * z.y,
                origin.z + along_reference * x.z + along_axis * z.z,
            )
        };
        // The meridian circle runs through the reference direction and the
        // axis, so its normal is the reference crossed with the axis.
        let meridian_axis = UnitVector3::new(Vector3::from(reference).cross(Vector3::from(axis)))?;
        let oriented = |emitter: &mut crate::writer::Emitter, edge: Ref, forward: bool| {
            let flag = if forward { ".T." } else { ".F." };
            emitter.emit("ORIENTED_EDGE", &format!("'',*,*,{edge},{flag}"))
        };
        let loop_edges = if let Some(tube_radius) = tube_radius {
            let vertex_point =
                geometry::point(&mut self.emitter, at(radius + tube_radius, 0.0, center));
            let vertex = self
                .emitter
                .emit("VERTEX_POINT", &format!("'',{vertex_point}"));
            let meridian_frame = geometry::placement(
                &mut self.emitter,
                at(radius, 0.0, center),
                meridian_axis,
                reference,
            );
            let meridian = self.emitter.emit(
                "CIRCLE",
                &format!("'',{meridian_frame},{}", self.emitter.real(tube_radius)),
            );
            let equator_frame = geometry::placement(&mut self.emitter, center, axis, reference);
            let equator = self.emitter.emit(
                "CIRCLE",
                &format!(
                    "'',{equator_frame},{}",
                    self.emitter.real(radius + tube_radius)
                ),
            );
            let meridian_edge = self.emitter.emit(
                "EDGE_CURVE",
                &format!("'',{vertex},{vertex},{meridian},.T."),
            );
            let equator_edge = self
                .emitter
                .emit("EDGE_CURVE", &format!("'',{vertex},{vertex},{equator},.T."));
            let sequence = [
                (meridian_edge, false),
                (equator_edge, true),
                (meridian_edge, true),
                (equator_edge, false),
            ];
            let sequence: Vec<(Ref, bool)> = if forward {
                sequence.to_vec()
            } else {
                [
                    (equator_edge, true),
                    (meridian_edge, false),
                    (equator_edge, false),
                    (meridian_edge, true),
                ]
                .to_vec()
            };
            sequence
                .into_iter()
                .map(|(edge, direction)| oriented(&mut self.emitter, edge, direction))
                .collect::<Vec<_>>()
        } else {
            let south = geometry::point(&mut self.emitter, at(0.0, -radius, center));
            let north = geometry::point(&mut self.emitter, at(0.0, radius, center));
            let south = self.emitter.emit("VERTEX_POINT", &format!("'',{south}"));
            let north = self.emitter.emit("VERTEX_POINT", &format!("'',{north}"));
            let frame = geometry::placement(&mut self.emitter, center, meridian_axis, reference);
            let meridian = self.emitter.emit(
                "CIRCLE",
                &format!("'',{frame},{}", self.emitter.real(radius)),
            );
            let edge = self
                .emitter
                .emit("EDGE_CURVE", &format!("'',{south},{north},{meridian},.T."));
            vec![
                oriented(&mut self.emitter, edge, true),
                oriented(&mut self.emitter, edge, false),
            ]
        };
        let edge_loop = self
            .emitter
            .emit("EDGE_LOOP", &format!("'',{}", refs(&loop_edges)));
        Some(
            self.emitter
                .emit("FACE_OUTER_BOUND", &format!("'',{edge_loop},{bound_flag}")),
        )
    }

    /// The sides of the parameter rectangle of `nurbs`, wound so the face
    /// lies on their left.
    fn parameter_rectangle(
        &self,
        face: &Face,
        nurbs: &NurbsSurface,
    ) -> Result<Result<ParameterRectangle, &'static str>, ResourceLimit> {
        let (Some(u), Some(v)) = (
            knot_domain(nurbs.u_knots(), nurbs.u_degree(), nurbs.u_count()),
            knot_domain(nurbs.v_knots(), nurbs.v_degree(), nurbs.v_count()),
        ) else {
            return Ok(Err("the support has no knot domain"));
        };
        let domain = [u, v];
        let [[u0, u1], [v0, v1]] = domain;
        let Some(surface) = self.surfaces.get(face.surface.as_str()) else {
            return Ok(Err("the face carrier is missing"));
        };
        let mut corners = [Point3::new(0.0, 0.0, 0.0); 4];
        for (corner, (u, v)) in corners
            .iter_mut()
            .zip([(u0, v0), (u1, v0), (u1, v1), (u0, v1)])
        {
            let Some(point) = finite_or_refusal(model_surface_point(
                EvaluationAdmission::Standard,
                self.ir,
                &surface.geometry,
                u,
                v,
            ))?
            else {
                return Ok(Err("a parameter rectangle corner does not evaluate"));
            };
            *corner = point.get();
        }
        let tolerance = self.ir.tolerances.linear.get();
        for (index, corner) in corners.iter().enumerate() {
            if corners[index + 1..]
                .iter()
                .any(|other| other.distance(*corner) <= tolerance)
            {
                return Ok(Err("two corners of the parameter rectangle coincide"));
            }
        }
        let mut sides = Vec::with_capacity(4);
        for (direction, at) in [
            (IsolineDirection::ConstantV, v0),
            (IsolineDirection::ConstantU, u1),
            (IsolineDirection::ConstantV, v1),
            (IsolineDirection::ConstantU, u0),
        ] {
            let Some(side) =
                nurbs_surface_isoline(EvaluationAdmission::Standard, nurbs, direction, at)?
            else {
                return Ok(Err("a side of the parameter rectangle does not evaluate"));
            };
            sides.push(side);
        }
        let Ok(sides) = <[NurbsCurve; 4]>::try_from(sides) else {
            return Ok(Err("a side of the parameter rectangle does not evaluate"));
        };
        Ok(Ok(ParameterRectangle {
            corners,
            domain,
            sides,
            counterclockwise: matches!(face.sense, Sense::Forward) != nurbs.normal_reversed(),
        }))
    }

    /// Write the rectangle as a `FACE_OUTER_BOUND` on `surface`.
    pub(super) fn emit_parameter_rectangle(
        &mut self,
        rectangle: ParameterRectangle,
        surface: Ref,
        bound_flag: &str,
    ) -> Option<Ref> {
        let ParameterRectangle {
            corners,
            domain: [[u0, u1], [v0, v1]],
            sides,
            counterclockwise,
        } = rectangle;
        let vertices = corners.map(|corner| {
            let point = geometry::point(&mut self.emitter, corner);
            self.emitter.emit("VERTEX_POINT", &format!("'',{point}"))
        });
        let traces = [
            ([0.0, v0], [1.0, 0.0]),
            ([u1, 0.0], [0.0, 1.0]),
            ([0.0, v1], [1.0, 0.0]),
            ([u0, 0.0], [0.0, 1.0]),
        ];
        let context = self.pcurve_context();
        let mut edges = Vec::with_capacity(sides.len());
        for ((side, [start, end]), ([ou, ov], [du, dv])) in
            sides.into_iter().zip(SIDE_CORNERS).zip(traces)
        {
            let curve = geometry::curve(&mut self.emitter, &SolvedCurveGeometry::Nurbs(side))?;
            let trace = LinePcurve::try_new(Point2::new(ou, ov), Point2::new(du, dv)).ok()?;
            let trace = geometry::pcurve(&mut self.emitter, &PcurveGeometry::Line(trace))?;
            let representation = self.emitter.emit(
                "DEFINITIONAL_REPRESENTATION",
                &format!("'',({trace}),{context}"),
            );
            let pcurve = self
                .emitter
                .emit("PCURVE", &format!("'',{surface},{representation}"));
            let surface_curve = self.emitter.emit(
                "SURFACE_CURVE",
                &format!("'',{curve},({pcurve}),.CURVE_3D."),
            );
            edges.push(self.emitter.emit(
                "EDGE_CURVE",
                &format!(
                    "'',{},{},{surface_curve},.T.",
                    vertices[start], vertices[end]
                ),
            ));
        }
        let order: [(usize, bool); 4] = if counterclockwise {
            [(0, true), (1, true), (2, false), (3, false)]
        } else {
            [(3, true), (2, true), (1, false), (0, false)]
        };
        let oriented = order
            .into_iter()
            .map(|(side, forward)| {
                let flag = if forward { ".T." } else { ".F." };
                self.emitter
                    .emit("ORIENTED_EDGE", &format!("'',*,*,{},{flag}", edges[side]))
            })
            .collect::<Vec<_>>();
        let edge_loop = self
            .emitter
            .emit("EDGE_LOOP", &format!("'',{}", refs(&oriented)));
        Some(
            self.emitter
                .emit("FACE_OUTER_BOUND", &format!("'',{edge_loop},{bound_flag}")),
        )
    }
}

/// The parameter interval `[knots[degree], knots[count]]` of a B-spline axis.
fn knot_domain(knots: &[f64], degree: u32, count: usize) -> Option<[f64; 2]> {
    let degree = usize::try_from(degree).ok()?;
    let start = *knots.get(degree)?;
    let end = *knots.get(count)?;
    (start < end).then_some([start, end])
}

#[cfg(test)]
mod tests;
