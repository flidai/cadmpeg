// SPDX-License-Identifier: Apache-2.0
//! The band of a torus face between two full circles.
//!
//! A face on a torus whose boundary is two full circles round the torus axis
//! can be either of two bands of the tube: the arc from one circle to the other
//! or the rest of the tube. The loops and the face sense choose one. A fillet
//! spans less than half a turn of the tube, so a face that the loops and sense
//! make longer than half a turn takes the other side: its sense is flipped.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use cadmpeg_core::decode::DecodeContext;
use cadmpeg_core::CodecError;
use cadmpeg_ir::document::CadIr;
use cadmpeg_ir::geometry::{CurveGeometry, SolvedCurveGeometry, SolvedSurfaceGeometry};
use cadmpeg_ir::topology::{LoopBoundary, Sense};

/// Relative tolerance of the coaxial test.
const COAXIAL: f64 = 1.0e-6;

/// The tube angle of a circle round the torus axis and the direction in which
/// the coedge runs round that axis (`1.0` anticlockwise).
struct Ring {
    angle: f64,
    direction: f64,
}

/// Flip the sense of every torus face whose two circle loops select the band
/// of more than half a turn. Returns the number of faces flipped.
pub(crate) fn choose_torus_bands(
    ctx: &DecodeContext<'_>,
    ir: &mut CadIr,
) -> Result<usize, CodecError> {
    let model = &ir.model;
    let surfaces: HashMap<_, _> = model.surfaces.iter().map(|s| (&s.id, s)).collect();
    let loops: HashMap<_, _> = model.loops.iter().map(|l| (&l.id, l)).collect();
    let coedges: HashMap<_, _> = model.coedges.iter().map(|c| (&c.id, c)).collect();
    let edges: HashMap<_, _> = model.edges.iter().map(|e| (&e.id, e)).collect();
    let curves: HashMap<_, _> = model.curves.iter().map(|c| (&c.id, c)).collect();
    let mut flip = Vec::new();
    for (index, face) in model.faces.iter().enumerate() {
        ctx.charge_work(1, "choose Solid Edge torus bands")?;
        let Some(SolvedSurfaceGeometry::Torus(torus)) = surfaces
            .get(&face.surface)
            .and_then(|s| s.geometry.solved())
        else {
            continue;
        };
        let center = torus.center().get();
        let axis = *torus.frame().axis().as_raw();
        let major = torus.major_radius().get();
        let minor = torus.minor_radius().get();
        if !(minor > 0.0 && major > minor) {
            continue;
        }
        let mut rings = Vec::new();
        for loop_id in &face.loops {
            let Some(LoopBoundary::Ring(ring)) = loops.get(loop_id).map(|l| &l.boundary) else {
                rings.clear();
                break;
            };
            let [coedge_id] = ring.coedges() else {
                rings.clear();
                break;
            };
            let Some(coedge) = coedges.get(coedge_id) else {
                rings.clear();
                break;
            };
            let Some(edge) = edges.get(&coedge.edge) else {
                rings.clear();
                break;
            };
            let circle = edge
                .curve()
                .and_then(|id| curves.get(id))
                .and_then(|curve| match &curve.geometry {
                    CurveGeometry::Solved(SolvedCurveGeometry::Circle(circle)) => Some(circle),
                    _ => None,
                });
            let Some(circle) = circle.filter(|_| edge.start == edge.end) else {
                rings.clear();
                break;
            };
            let c = circle.center().get();
            let d = [c.x - center.x, c.y - center.y, c.z - center.z];
            let height = d[0] * axis.x + d[1] * axis.y + d[2] * axis.z;
            let off_axis = [
                d[0] - height * axis.x,
                d[1] - height * axis.y,
                d[2] - height * axis.z,
            ];
            let off = (off_axis[0].powi(2) + off_axis[1].powi(2) + off_axis[2].powi(2)).sqrt();
            let circle_axis = circle.frame().axis().as_raw();
            let along = circle_axis.x * axis.x + circle_axis.y * axis.y + circle_axis.z * axis.z;
            if off > COAXIAL * major || along.abs() < 1.0 - COAXIAL {
                rings.clear();
                break;
            }
            let forward = matches!(coedge.sense, Sense::Forward);
            rings.push(Ring {
                angle: height.atan2(circle.radius().get() - major),
                direction: along.signum() * if forward { 1.0 } else { -1.0 },
            });
        }
        let [first, second] = rings.as_slice() else {
            continue;
        };
        // Natural torus parameters: u round the axis, v round the tube, normal
        // out of the tube. The face lies left of each loop seen from its normal.
        let sense = if matches!(face.sense, Sense::Forward) {
            1.0
        } else {
            -1.0
        };
        let (from, to) = match (
            first.direction * sense > 0.0,
            second.direction * sense > 0.0,
        ) {
            (true, false) => (first.angle, second.angle),
            (false, true) => (second.angle, first.angle),
            _ => continue,
        };
        if (to - from).rem_euclid(TAU) > PI {
            flip.push(index);
        }
    }
    for index in &flip {
        let face = &mut ir.model.faces[*index];
        face.sense = match face.sense {
            Sense::Forward => Sense::Reversed,
            Sense::Reversed => Sense::Forward,
        };
    }
    Ok(flip.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::with_decode_context;
    use cadmpeg_ir::geometry::analytic::{CircleCurve, TorusSurface};
    use cadmpeg_ir::geometry::{Curve, Surface, SurfaceGeometry};
    use cadmpeg_ir::ids::{
        CoedgeId, CurveId, EdgeId, FaceId, LoopId, PointId, ShellId, SurfaceId, VertexId,
    };
    use cadmpeg_ir::math::{Point3, Vector3};
    use cadmpeg_ir::topology::{
        Coedge, Edge, EdgeCarrier, Face, FaceLoops, Loop, LoopRing, Point, Vertex,
    };
    use cadmpeg_ir::{identity_key, identity_namespace};

    /// A torus of major radius 10 and minor radius 2 about +z, and one face on it
    /// bounded by the circle at the outer equator (tube angle 0, radius 12) and the
    /// circle on top (tube angle a quarter turn, radius 10, height 2). `outer_forward`
    /// and `top_forward` are the coedge senses on circles that run anticlockwise about +z.
    fn fillet(outer_forward: bool, top_forward: bool, sense: Sense) -> CadIr {
        let mut ir = CadIr::empty();
        let m = &mut ir.model;
        m.surfaces.push(Surface {
            id: SurfaceId::compose(
                &identity_namespace!("solidedge", "test", "surface"),
                identity_key!("torus"),
            ),
            source_object: None,
            geometry: SurfaceGeometry::Solved(SolvedSurfaceGeometry::Torus(
                TorusSurface::try_new(
                    Point3::new(0.0, 0.0, 0.0),
                    Vector3::new(0.0, 0.0, 1.0),
                    Vector3::new(1.0, 0.0, 0.0),
                    10.0,
                    2.0,
                )
                .unwrap(),
            )),
        });
        let face = FaceId::compose(
            &identity_namespace!("solidedge", "test", "face"),
            identity_key!("band"),
        );
        let mut loops = Vec::new();
        for (key, radius, height, forward) in [
            (0_u16, 12.0, 0.0, outer_forward),
            (1_u16, 10.0, 2.0, top_forward),
        ] {
            let k = || identity_key!("ring").colon(key);
            let point = PointId::compose(&identity_namespace!("solidedge", "test", "point"), k());
            let vertex =
                VertexId::compose(&identity_namespace!("solidedge", "test", "vertex"), k());
            let curve = CurveId::compose(&identity_namespace!("solidedge", "test", "curve"), k());
            let edge = EdgeId::compose(&identity_namespace!("solidedge", "test", "edge"), k());
            let coedge =
                CoedgeId::compose(&identity_namespace!("solidedge", "test", "coedge"), k());
            let lp = LoopId::compose(&identity_namespace!("solidedge", "test", "loop"), k());
            m.points.push(Point::new(
                point.clone(),
                cadmpeg_ir::features::FinitePoint3::new(Point3::new(radius, 0.0, height)).unwrap(),
                None,
            ));
            m.vertices.push(Vertex {
                id: vertex.clone(),
                point,
                tolerance: None,
            });
            m.curves.push(Curve {
                parameter_range: None,
                id: curve.clone(),
                source_object: None,
                geometry: CurveGeometry::Solved(SolvedCurveGeometry::Circle(
                    CircleCurve::try_new(
                        Point3::new(0.0, 0.0, height),
                        Vector3::new(0.0, 0.0, 1.0),
                        Vector3::new(1.0, 0.0, 0.0),
                        radius,
                    )
                    .unwrap(),
                )),
            });
            m.edges.push(Edge {
                id: edge.clone(),
                carrier: EdgeCarrier::new(Some(curve), None).unwrap(),
                start: vertex.clone(),
                end: vertex,
                tolerance: None,
            });
            m.coedges.push(Coedge {
                id: coedge.clone(),
                owner_loop: lp.clone(),
                edge,
                radial_next: coedge.clone(),
                sense: if forward {
                    Sense::Forward
                } else {
                    Sense::Reversed
                },
                use_curve: None,
                pcurves: Vec::new(),
            });
            m.loops.push(Loop {
                id: lp.clone(),
                face: face.clone(),
                boundary: LoopBoundary::Ring(LoopRing::single(coedge)),
            });
            loops.push(lp);
        }
        m.faces.push(Face {
            id: face,
            shell: ShellId::compose(
                &identity_namespace!("solidedge", "test", "shell"),
                identity_key!("shell"),
            ),
            surface: m.surfaces[0].id.clone(),
            sense,
            loops: FaceLoops::Unspecified { loops },
            name: None,
            color: None,
            tolerance: None,
        });
        ir
    }

    fn run(ir: &mut CadIr) -> usize {
        with_decode_context(&[], |ctx| choose_torus_bands(ctx, ir).expect("admitted"))
    }

    #[test]
    fn a_quarter_band_keeps_its_sense() {
        // Outer circle anticlockwise, top circle clockwise: seen from the outward
        // tube normal the face lies between them, the quarter turn of a fillet.
        let mut ir = fillet(true, false, Sense::Forward);
        assert_eq!(run(&mut ir), 0);
        assert_eq!(ir.model.faces[0].sense, Sense::Forward);
    }

    #[test]
    fn a_band_of_three_quarters_takes_the_other_side() {
        let mut ir = fillet(false, true, Sense::Forward);
        assert_eq!(run(&mut ir), 1);
        assert_eq!(ir.model.faces[0].sense, Sense::Reversed);
        // The flipped face now selects the quarter band and is left alone.
        assert_eq!(run(&mut ir), 0);
    }

    #[test]
    fn the_face_sense_takes_part_in_the_choice() {
        // The same loops seen from the inward normal select the three-quarter band.
        let mut ir = fillet(true, false, Sense::Reversed);
        assert_eq!(run(&mut ir), 1);
        assert_eq!(ir.model.faces[0].sense, Sense::Forward);
    }

    #[test]
    fn loops_that_run_the_same_way_are_left_alone() {
        let mut ir = fillet(true, true, Sense::Forward);
        assert_eq!(run(&mut ir), 0);
        assert_eq!(ir.model.faces[0].sense, Sense::Forward);
    }
}
