//! Sketch entity projection from B-rep edges.

use cadmpeg_ir::annotations::Annotations;
use cadmpeg_ir::geometry::{CurveGeometry, SolvedCurveGeometry};
use cadmpeg_ir::math::{Point2, Point3, Vector3};
use cadmpeg_ir::sketches::{
    SketchConstraint, SketchConstraintDefinitionInput, SketchConstraintId, SketchEntity,
    SketchGeometry, SketchGeometryDefinition, SketchId, SketchLocus,
};
use cadmpeg_ir::Exactness;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

struct FormattedByteCount(usize);

impl fmt::Write for FormattedByteCount {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 = self.0.checked_add(text.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

fn retained_curve_debug(
    ctx: &cadmpeg_core::decode::DecodeContext<'_>,
    curve: &CurveGeometry,
) -> Result<String, cadmpeg_core::CodecError> {
    let mut count = FormattedByteCount(0);
    fmt::write(&mut count, format_args!("{curve:?}")).map_err(|_| {
        ctx.refuse_codec_limit("retain SLDPRT opaque sketch curve", u64::MAX - 1, u64::MAX)
    })?;
    let mut text = String::new();
    ctx.try_reserve_retained_text(&mut text, count.0, "retain SLDPRT opaque sketch curve")?;
    fmt::write(&mut text, format_args!("{curve:?}")).map_err(|_| {
        cadmpeg_core::CodecError::malformed("cannot format SLDPRT opaque sketch curve")
    })?;
    Ok(text)
}

fn retained_id_text(
    ctx: &cadmpeg_core::decode::DecodeContext<'_>,
    value: &str,
    operation: &'static str,
) -> Result<String, cadmpeg_core::CodecError> {
    let mut text = String::new();
    ctx.try_reserve_retained_text(&mut text, value.len(), operation)?;
    text.push_str(value);
    Ok(text)
}

const EPS_SKETCH_EDGES_PROJECT_EDGE_E9: f64 = 1.0e-9;
/// Largest distance, in millimetres, between a B-spline end pole and the
/// projected edge vertex for the carrier to end at the vertex.
const EPS_SKETCH_EDGES_NURBS_ENDPOINT_MM: f64 = 1.0e-6;
const EPS_SKETCH_EDGES_CIRCLE_CONTAINS_POINT_E9: f64 = 1.0e-9;
const EPS_SKETCH_EDGES_ELLIPSE_CONTAINS_POINT_E9: f64 = 1.0e-9;

/// The projection tolerance of one edge projected onto a sketch plane.
///
/// The projection resolves to [`EPS_SKETCH_EDGES_PROJECT_EDGE_E9`], so an edge
/// stating a finer tolerance states a resolution the projection does not
/// carry. [`EdgeProjectionTolerance::of`] is the only constructor: a stated
/// tolerance under that bound is refused, so no stated value is floored, and an
/// edge that states no tolerance projects at the bound.
#[derive(Debug, Clone, Copy)]
struct EdgeProjectionTolerance(f64);

impl EdgeProjectionTolerance {
    /// The projection tolerance of `edge`, or `None` when the edge states a
    /// tolerance finer than the projection resolves.
    fn of(edge: &cadmpeg_ir::topology::Edge) -> Option<Self> {
        let Some(stated) = edge.tolerance else {
            return Some(Self(EPS_SKETCH_EDGES_PROJECT_EDGE_E9));
        };
        (stated.get() >= EPS_SKETCH_EDGES_PROJECT_EDGE_E9).then_some(Self(stated.get()))
    }

    /// The tolerance value.
    fn get(self) -> f64 {
        self.0
    }
}

/// The sketch and the source position whose shared endpoints become constraints.
#[derive(Clone, Copy)]
pub(super) struct EndpointConstraintSource<'a> {
    pub(super) sketch: &'a SketchId,
    pub(super) entities: &'a [SketchEntity],
    pub(super) block_offset: usize,
    pub(super) stream_ordinal: usize,
    pub(super) face_ordinal: usize,
    pub(super) stream: &'a cadmpeg_ir::StreamName,
}

pub(super) fn project_endpoint_constraints(
    ctx: &cadmpeg_core::decode::DecodeContext<'_>,
    source: EndpointConstraintSource<'_>,
    annotations: &mut Annotations,
    constraints: &mut Vec<SketchConstraint>,
) -> Result<(), cadmpeg_core::CodecError> {
    let EndpointConstraintSource {
        sketch,
        entities,
        block_offset,
        stream_ordinal,
        face_ordinal,
        stream,
    } = source;
    let mut loci_by_endpoint =
        BTreeMap::<&str, Vec<(bool, &cadmpeg_ir::sketches::SketchEntityId)>>::new();
    for entity in entities {
        if entity.endpoint_refs.len() != 2 {
            continue;
        }
        for (index, endpoint) in entity.endpoint_refs.iter().enumerate() {
            ctx.admit_btree_entry(
                &loci_by_endpoint,
                &endpoint.as_str(),
                "index SLDPRT shared sketch endpoints",
            )?;
            let loci = loci_by_endpoint.entry(endpoint).or_default();
            ctx.reserve_vec(loci, 1, "collect SLDPRT shared sketch endpoint loci")?;
            loci.push((index == 0, entity.id()));
        }
    }
    for (_endpoint, sources) in loci_by_endpoint {
        let Some((_, first)) = sources.first() else {
            continue;
        };
        ctx.charge_work(
            u64::try_from(sources.len()).map_err(|_| {
                ctx.refuse_codec_limit(
                    "compare SLDPRT shared sketch endpoints",
                    u64::MAX - 1,
                    u64::MAX,
                )
            })?,
            "compare SLDPRT shared sketch endpoints",
        )?;
        if !sources.iter().any(|(_, entity)| entity != first) {
            continue;
        }
        let digits = |value: usize| {
            if value == 0 {
                1
            } else {
                cadmpeg_core::decode::index_from_u32(value.ilog10()) + 1
            }
        };
        let id_length = "sldprt:model:sketch-constraint#:::".len()
            + digits(block_offset)
            + digits(stream_ordinal)
            + digits(face_ordinal)
            + digits(constraints.len());
        let mut id_text = String::new();
        ctx.try_reserve_retained_text(
            &mut id_text,
            id_length,
            "retain SLDPRT shared endpoint constraint ID",
        )?;
        std::fmt::Write::write_fmt(
            &mut id_text,
            format_args!(
                "sldprt:model:sketch-constraint#{block_offset}:{stream_ordinal}:{face_ordinal}:{}",
                constraints.len()
            ),
        )
        .map_err(|_| {
            cadmpeg_core::CodecError::malformed(
                "cannot format SLDPRT shared endpoint constraint ID",
            )
        })?;
        let Ok(id) = SketchConstraintId::mint(id_text) else {
            continue;
        };
        let mut loci = Vec::new();
        ctx.reserve_vec(
            &mut loci,
            sources.len(),
            "collect SLDPRT shared endpoint constraint loci",
        )?;
        for (start, source) in sources {
            let source = cadmpeg_ir::sketches::SketchEntityId::mint(retained_id_text(
                ctx,
                source.as_str(),
                "retain SLDPRT shared endpoint entity ID",
            )?)
            .map_err(|_| {
                cadmpeg_core::CodecError::malformed("invalid admitted SLDPRT sketch entity ID")
            })?;
            loci.push(if start {
                SketchLocus::Start(source)
            } else {
                SketchLocus::End(source)
            });
        }
        let Ok(definition) = cadmpeg_ir::sketches::SketchConstraintDefinition::try_from(
            SketchConstraintDefinitionInput::CoincidentLoci { loci },
        ) else {
            continue;
        };
        crate::annotations::note(
            ctx,
            annotations,
            retained_id_text(
                ctx,
                id.as_str(),
                "retain SLDPRT shared endpoint annotation ID",
            )?,
            stream,
            0,
            "feature_input_shared_endpoint",
            Exactness::Derived,
        )?;
        let sketch = SketchId::mint(retained_id_text(
            ctx,
            sketch.as_str(),
            "retain SLDPRT constraint sketch ID",
        )?)
        .map_err(|_| cadmpeg_core::CodecError::malformed("invalid admitted SLDPRT sketch ID"))?;
        ctx.reserve_vec(constraints, 1, "collect SLDPRT shared endpoint constraints")?;
        constraints.push(SketchConstraint {
            id,
            sketch,
            definition,
            name: None,
            driving: None,
            active: None,
            virtual_space: None,
            visible: None,
            orientation: None,
            label_distance: None,
            label_position: None,
            metadata: None,
            native_ref: None,
        });
    }
    Ok(())
}

/// The sketch plane a projection maps model space onto.
///
/// The origin and the two in-plane axes are one frame: a projection reads
/// all three together or none of them.
#[derive(Debug, Clone, Copy)]
pub(super) struct SketchPlaneFrame {
    /// Plane origin in model space.
    pub(super) origin: Point3,
    /// In-plane u direction.
    pub(super) u_axis: Vector3,
    /// In-plane v direction.
    pub(super) v_axis: Vector3,
}

/// Knots, projected poles and weights of a projected NURBS edge.
type ProjectedNurbsLanes = (Vec<f64>, Vec<Point2>, Option<Vec<f64>>);

/// One edge projected onto a sketch plane.
#[derive(Debug)]
pub(super) struct ProjectedEdge {
    pub(super) geometry: SketchGeometry,
    /// The sketch entity runs from the end vertex of the edge to its start
    /// vertex. A sketch arc runs counter-clockwise, and a circle or ellipse
    /// whose axis opposes the plane normal runs clockwise in the plane.
    pub(super) reversed_endpoints: bool,
}

impl ProjectedEdge {
    fn forward(geometry: SketchGeometry) -> Self {
        Self {
            geometry,
            reversed_endpoints: false,
        }
    }
}

/// Largest distance, in millimetres, of a chart pole from the chord for the
/// chart to count as straight.
const STRAIGHT_CHART_TOLERANCE_MM: f64 = 1.0e-3;

/// Whether every pole of a degree-1 chart lies on the chord through `start` and
/// `end`, and the chart spans both end points along that chord. The vertices of
/// an intersection edge lie within the chart error of the polyline, so the
/// end points are compared with the extent of the chart, not with its poles.
fn straight_chart_contains(poles: &[Point2], start: Point2, end: Point2) -> bool {
    let (du, dv) = (end.u - start.u, end.v - start.v);
    let length = du.hypot(dv);
    if length <= EPS_SKETCH_EDGES_NURBS_ENDPOINT_MM || poles.len() < 2 {
        return false;
    }
    let along = |point: Point2| ((point.u - start.u) * du + (point.v - start.v) * dv) / length;
    let across =
        |point: Point2| ((point.u - start.u) * dv - (point.v - start.v) * du).abs() / length;
    if poles
        .iter()
        .any(|pole| across(*pole) > STRAIGHT_CHART_TOLERANCE_MM)
    {
        return false;
    }
    let lower = poles
        .iter()
        .map(|pole| along(*pole))
        .fold(f64::INFINITY, f64::min);
    let upper = poles
        .iter()
        .map(|pole| along(*pole))
        .fold(f64::NEG_INFINITY, f64::max);
    lower <= STRAIGHT_CHART_TOLERANCE_MM && upper >= length - STRAIGHT_CHART_TOLERANCE_MM
}

/// Project an edge onto a sketch plane.
///
/// A sketch arc and a sketch ellipse run counter-clockwise in the plane. The
/// stored curve runs counter-clockwise about its own axis, so a curve whose
/// axis opposes the plane normal projects to the same arc with the parameter
/// sign negated and the endpoints exchanged.
pub(super) fn project_edge(
    ctx: &cadmpeg_core::decode::DecodeContext<'_>,
    edge: &cadmpeg_ir::topology::Edge,
    vertices: &HashMap<&cadmpeg_ir::ids::VertexId, &cadmpeg_ir::ids::PointId>,
    points: &HashMap<&cadmpeg_ir::ids::PointId, Point3>,
    curves: &HashMap<&cadmpeg_ir::ids::CurveId, &CurveGeometry>,
    frame: SketchPlaneFrame,
    refusal: &mut crate::lane_refusal::LaneRefusals,
) -> Result<Option<ProjectedEdge>, cadmpeg_core::CodecError> {
    let SketchPlaneFrame {
        origin,
        u_axis,
        v_axis,
    } = frame;
    let Some(start_id) = vertices.get(&edge.start) else {
        return Ok(None);
    };
    let Some(start_point) = points.get(start_id) else {
        return Ok(None);
    };
    let start = project_point(*start_point, origin, u_axis, v_axis);
    let Some(end_id) = vertices.get(&edge.end) else {
        return Ok(None);
    };
    let Some(end_point) = points.get(end_id) else {
        return Ok(None);
    };
    let end = project_point(*end_point, origin, u_axis, v_axis);
    let line = || SketchGeometry::try_from(SketchGeometryDefinition::Line { start, end }).ok();
    let Some(tolerance) = EdgeProjectionTolerance::of(edge) else {
        return Ok(None);
    };
    let tolerance = tolerance.get();
    if let Some(CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(nurbs))) =
        edge.curve().and_then(|id| curves.get(id).copied())
    {
        let operation = "project SLDPRT sketch NURBS edge";
        let project = |point: Point3| project_point(point, origin, u_axis, v_axis);
        // A carrier that runs past the edge is cut to the span between the end
        // points of the edge. A carrier that already ends at them stays whole.
        let whole_lanes = |ctx: &cadmpeg_core::decode::DecodeContext<'_>| -> Result<
            ProjectedNurbsLanes,
            cadmpeg_core::CodecError,
        > {
            let count = nurbs.pole_count();
            let mut knots = Vec::new();
            ctx.reserve_vec(&mut knots, nurbs.knots().len(), operation)?;
            knots.extend_from_slice(nurbs.knots());
            let mut projected = Vec::new();
            ctx.reserve_vec(&mut projected, count, operation)?;
            let weights = match nurbs.pole_rows() {
                cadmpeg_ir::geometry::nurbs::NurbsPoles3::Polynomial { points } => {
                    projected.extend(points.iter().map(|point| project(point.get())));
                    None
                }
                cadmpeg_ir::geometry::nurbs::NurbsPoles3::Rational { points } => {
                    projected.extend(points.iter().map(|point| project(point.point.get())));
                    let mut weights = Vec::new();
                    ctx.reserve_vec(&mut weights, count, operation)?;
                    weights.extend(points.iter().map(|point| point.weight.get()));
                    Some(weights)
                }
            };
            Ok((knots, projected, weights))
        };
        let (mut knots, mut projected, mut weights) = whole_lanes(ctx)?;
        let mut against_edge = false;
        let gap = |left: Point2, right: Point2| (left.u - right.u).hypot(left.v - right.v);
        let tolerance = tolerance.max(EPS_SKETCH_EDGES_NURBS_ENDPOINT_MM);
        let ends_match = |poles: &[Point2], first: Point2, last: Point2| {
            poles.first().zip(poles.last()).is_some_and(|(head, tail)| {
                gap(*head, first) <= tolerance && gap(*tail, last) <= tolerance
            })
        };
        if !ends_match(&projected, start, end) {
            if ends_match(&projected, end, start) {
                against_edge = true;
            } else if let Some(span) =
                crate::brep::graph::nurbs_edge_span(ctx, nurbs, *start_point, *end_point)?
            {
                let mut cut = Vec::new();
                ctx.reserve_vec(&mut cut, span.poles.len(), operation)?;
                cut.extend(span.poles.iter().map(|point| project(*point)));
                let (head_end, tail_end) = if span.against_edge {
                    (end, start)
                } else {
                    (start, end)
                };
                if ends_match(&cut, head_end, tail_end) {
                    knots = span.knots;
                    projected = cut;
                    weights = span.weights;
                    against_edge = span.against_edge;
                }
            }
        }
        if nurbs.degree() == 1
            && !ends_match(&projected, start, end)
            && !ends_match(&projected, end, start)
            && straight_chart_contains(&projected, start, end)
        {
            // A straight chart polyline that contains both end points, with a
            // vertex a chart error off the polyline: the edge is the chord.
            return Ok(line().map(ProjectedEdge::forward));
        }
        return match cadmpeg_ir::geometry::pcurve::PcurveNurbs::from_lanes(
            ctx,
            nurbs.degree(),
            knots,
            projected,
            weights,
            nurbs.periodic(),
        )? {
            Ok(nurbs) => Ok(Some(ProjectedEdge {
                geometry: SketchGeometry::nurbs(nurbs),
                reversed_endpoints: against_edge,
            })),
            Err(error) => {
                refusal.note(
                    ctx,
                    format_args!("sldprt projected sketch edge {}", edge.id),
                    &error,
                )?;
                Ok(None)
            }
        };
    }
    let curve = edge.curve().and_then(|id| curves.get(id).copied());
    let native = match curve {
        Some(CurveGeometry::Solved(
            SolvedCurveGeometry::Circle(_)
            | SolvedCurveGeometry::Ellipse(_)
            | SolvedCurveGeometry::Line(_)
            | SolvedCurveGeometry::Nurbs(_),
        ))
        | None => None,
        Some(other) => cadmpeg_core::text::NonBlankString::new(retained_curve_debug(ctx, other)?)
            .map(SketchGeometry::native),
    };
    let normal = u_axis.cross(v_axis);
    let projected = (|| match curve {
        Some(CurveGeometry::Solved(SolvedCurveGeometry::Circle(circle_curve))) => {
            let center = circle_curve.center().get();
            let radius = circle_curve.radius();
            let center = project_point(center, origin, u_axis, v_axis);
            if !circle_contains_point(center, radius, start, tolerance)
                || !circle_contains_point(center, radius, end, tolerance)
            {
                return line().map(ProjectedEdge::forward);
            }
            if (start.u - end.u).hypot(start.v - end.v) <= EPS_SKETCH_EDGES_PROJECT_EDGE_E9 {
                Some(ProjectedEdge::forward(
                    SketchGeometry::try_from(SketchGeometryDefinition::Circle {
                        center,
                        radius: cadmpeg_ir::scalar::Length::from(circle_curve.radius()),
                    })
                    .ok()?,
                ))
            } else {
                let parameters = edge
                    .param_range()
                    .map(cadmpeg_ir::units::FiniteVector::get)
                    .filter(|[start, end]| start != end);
                let clockwise = circle_curve.frame().axis().as_raw().dot(normal) < 0.0;
                // The stored parameter counts from the reference direction.
                let reference = circle_curve.frame().reference().as_raw();
                let reference_angle = reference.dot(v_axis).atan2(reference.dot(u_axis));
                let angle_at = |point: Point2| (point.v - center.v).atan2(point.u - center.u);
                let (start_angle, end_angle) = match (parameters, clockwise) {
                    (Some([first, last]), false) => {
                        (reference_angle + first, reference_angle + last)
                    }
                    (Some([first, last]), true) => {
                        (reference_angle - last, reference_angle - first)
                    }
                    (None, false) => (angle_at(start), angle_at(end)),
                    (None, true) => (angle_at(end), angle_at(start)),
                };
                Some(ProjectedEdge {
                    geometry: SketchGeometry::try_from(SketchGeometryDefinition::Arc {
                        center,
                        radius: cadmpeg_ir::scalar::Length::from(circle_curve.radius()),
                        start_angle: cadmpeg_ir::scalar::Angle::new(start_angle)?,
                        end_angle: cadmpeg_ir::scalar::Angle::new(end_angle)?,
                    })
                    .ok()?,
                    reversed_endpoints: clockwise,
                })
            }
        }
        Some(CurveGeometry::Solved(SolvedCurveGeometry::Ellipse(ellipse_curve))) => {
            let center = ellipse_curve.center().get();
            let major_direction = ellipse_curve.frame().reference().as_raw();
            let major_radius = ellipse_curve.major_radius().get();
            let minor_radius = ellipse_curve.minor_radius().get();
            let center = project_point(center, origin, u_axis, v_axis);
            let major_u = major_direction.dot(u_axis);
            let major_v = major_direction.dot(v_axis);
            let major_angle = major_v.atan2(major_u);
            if !ellipse_contains_point(
                center,
                major_angle,
                ellipse_curve.major_radius(),
                ellipse_curve.minor_radius(),
                start,
                tolerance,
            ) || !ellipse_contains_point(
                center,
                major_angle,
                ellipse_curve.major_radius(),
                ellipse_curve.minor_radius(),
                end,
                tolerance,
            ) {
                return line().map(ProjectedEdge::forward);
            }
            let full = (start.u - end.u).hypot(start.v - end.v) <= EPS_SKETCH_EDGES_PROJECT_EDGE_E9;
            let parameter = |point: Point2| {
                let du = point.u - center.u;
                let dv = point.v - center.v;
                let major_component = du * major_angle.cos() + dv * major_angle.sin();
                let minor_component = -du * major_angle.sin() + dv * major_angle.cos();
                (minor_component / minor_radius).atan2(major_component / major_radius)
            };
            let parameters = edge
                .param_range()
                .map(cadmpeg_ir::units::FiniteVector::get)
                .filter(|[start, end]| start != end);
            let clockwise = ellipse_curve.frame().axis().as_raw().dot(normal) < 0.0;
            let bounds = if full {
                None
            } else {
                let (first, last) = match (parameters, clockwise) {
                    (Some([first, last]), false) => (first, last),
                    (Some([first, last]), true) => (-last, -first),
                    (None, false) => (parameter(start), parameter(end)),
                    (None, true) => (parameter(end), parameter(start)),
                };
                Some([
                    cadmpeg_ir::scalar::Angle::new(first)?,
                    cadmpeg_ir::scalar::Angle::new(last)?,
                ])
            };
            Some(ProjectedEdge {
                geometry: SketchGeometry::try_from(SketchGeometryDefinition::Ellipse {
                    center,
                    major_angle: cadmpeg_ir::scalar::Angle::new(major_angle)?,
                    radii: cadmpeg_ir::sketches::EllipseRadii {
                        major_radius: cadmpeg_ir::scalar::Length::from(
                            ellipse_curve.major_radius(),
                        ),
                        minor_radius: cadmpeg_ir::scalar::Length::from(
                            ellipse_curve.minor_radius(),
                        ),
                    },
                    bounds,
                })
                .ok()?,
                reversed_endpoints: clockwise && !full,
            })
        }
        Some(CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(_))) => None,
        None if edge.start == edge.end => Some(ProjectedEdge::forward(
            SketchGeometry::try_from(SketchGeometryDefinition::Point { position: start }).ok()?,
        )),
        Some(CurveGeometry::Solved(SolvedCurveGeometry::Line(_))) | None => {
            line().map(ProjectedEdge::forward)
        }
        Some(_) => native.map(ProjectedEdge::forward),
    })();
    Ok(projected)
}

fn circle_contains_point(
    center: Point2,
    radius: cadmpeg_ir::scalar::PositiveLength,
    point: Point2,
    tolerance: f64,
) -> bool {
    let radius = radius.get();
    let distance = (point.u - center.u).hypot(point.v - center.v);
    distance.is_finite()
        && (distance - radius.abs()).abs()
            <= tolerance.max(radius.abs() * EPS_SKETCH_EDGES_CIRCLE_CONTAINS_POINT_E9)
}

fn ellipse_contains_point(
    center: Point2,
    major_angle: f64,
    major_radius: cadmpeg_ir::scalar::PositiveLength,
    minor_radius: cadmpeg_ir::scalar::PositiveLength,
    point: Point2,
    tolerance: f64,
) -> bool {
    let major_radius = major_radius.get();
    let minor_radius = minor_radius.get();
    if major_radius.abs() <= tolerance || minor_radius.abs() <= tolerance {
        return false;
    }
    let du = point.u - center.u;
    let dv = point.v - center.v;
    let major = du * major_angle.cos() + dv * major_angle.sin();
    let minor = -du * major_angle.sin() + dv * major_angle.cos();
    let parameter = (minor / minor_radius).atan2(major / major_radius);
    let reconstructed = Point2::new(
        center.u + major_radius * parameter.cos() * major_angle.cos()
            - minor_radius * parameter.sin() * major_angle.sin(),
        center.v
            + major_radius * parameter.cos() * major_angle.sin()
            + minor_radius * parameter.sin() * major_angle.cos(),
    );
    let distance = (point.u - reconstructed.u).hypot(point.v - reconstructed.v);
    distance.is_finite()
        && distance
            <= tolerance.max(
                major_radius.abs().max(minor_radius.abs())
                    * EPS_SKETCH_EDGES_ELLIPSE_CONTAINS_POINT_E9,
            )
}

pub(super) fn project_point(
    point: Point3,
    origin: Point3,
    u_axis: Vector3,
    v_axis: Vector3,
) -> Point2 {
    let delta = Vector3::new(point.x - origin.x, point.y - origin.y, point.z - origin.z);
    Point2::new(delta.dot(u_axis), delta.dot(v_axis))
}

#[cfg(test)]
mod sketch_edges_tests;
