// SPDX-License-Identifier: Apache-2.0
//! Staging of a capped `ON_Extrusion` as one shell of caps and lateral faces.
//!
//! The profile boundaries are cut at their corners. Each piece owns one lateral
//! face, whose bottom and top edges are the edges of the cap faces, so the two
//! caps and the laterals meet in shared vertices and edges. A cap that the
//! object does not state leaves its edges on one face, and the shell stays
//! open.

use super::{annotate_derived, CandidateError, ExactnessTarget};
use crate::extrusion::{CapPcurve, DecodedExtrusion, ExtrusionBoundary, ExtrusionPiece};
use cadmpeg_core::decode::DecodeContext;
use cadmpeg_ir::document::CadIr;
use cadmpeg_ir::geometry::{
    nurbs::NurbsCurve,
    pcurve::{Pcurve, PcurveGeometry, PcurveNurbs},
    Curve, CurveGeometry, ProceduralSurface, ProceduralSurfaceDefinition, SolvedCurveGeometry,
    SolvedSurfaceGeometry, Surface, SurfaceGeometry,
};
use cadmpeg_ir::ids::{
    BodyId, CoedgeId, CurveId, EdgeId, FaceId, IdentityKey, LoopId, PcurveId, PointId,
    ProceduralSurfaceId, RegionId, ShellId, SurfaceId, VertexId,
};
use cadmpeg_ir::topology::{
    Body, BodyKind, Coedge, Edge, EdgeCarrier, Face, FaceLoops, Loop, LoopBoundary, LoopRing,
    PcurveUse, Point, Region, Sense, Shell, Vertex,
};
use cadmpeg_ir::units::{FiniteVector, UnitVector3};
use cadmpeg_ir::SourceObjectAssociation;

const STAGING: &str = "extrusion staging";

macro_rules! rhino_id {
    ($ty:ident, $kind:literal, $key:expr) => {
        $ty::compose(
            &cadmpeg_ir::identity_namespace!("rhino", "object", $kind),
            $key,
        )
    };
}

macro_rules! copy_id {
    ($ctx:expr, $id:expr) => {
        $id.try_clone_for_decode($ctx, "Rhino typed identity copy")?
    };
}

/// Stages the caps and lateral faces of a capped extrusion and returns the
/// identity of its body.
pub(super) fn stage_extrusion_body<T: ExactnessTarget>(
    ctx: &DecodeContext<'_>,
    ir: &mut CadIr,
    annotations: &mut T,
    key: &str,
    association: &SourceObjectAssociation,
    extrusion: &DecodedExtrusion,
) -> Result<String, CandidateError> {
    if !(extrusion.caps[0] || extrusion.caps[1]) {
        return Err(format!("{STAGING}: no enabled caps").into());
    }
    let key = IdentityKey::try_new(key.to_owned()).map_err(|error| error.to_string())?;
    let shell_id = rhino_id!(ShellId, "shell", key_with(ctx, &key, format_args!("body"))?);
    let mut stage = Stage {
        ctx,
        ir,
        annotations,
        key,
        association,
        extrusion,
        shell: shell_id,
    };
    let mut boundary_edges =
        ctx.collection_vec(extrusion.boundaries.len(), "Rhino extrusion edges")?;
    for (profile, boundary) in extrusion.boundaries.iter().enumerate() {
        boundary_edges.push(stage.boundary_edges(profile, boundary)?);
    }
    let mut face_ids = Vec::new();
    for (profile, boundary) in extrusion.boundaries.iter().enumerate() {
        stage.laterals(profile, boundary, &boundary_edges[profile], &mut face_ids)?;
    }
    for cap in 0..2 {
        if extrusion.caps[cap] {
            face_ids.push(stage.cap(cap, &boundary_edges)?);
        }
    }

    let body_key = key_with(ctx, &stage.key, format_args!("body"))?;
    let body_id = rhino_id!(
        BodyId,
        "body",
        body_key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
    );
    let region_id = rhino_id!(RegionId, "region", body_key);
    let ir = stage.ir;
    ir.model.shells.push(
        Shell::new(
            copy_id!(ctx, stage.shell),
            copy_id!(ctx, region_id),
            face_ids,
            Vec::new(),
            Vec::new(),
        )
        .map_err(|error| format!("{STAGING}: {error}"))?,
    );
    ir.model.regions.push(Region {
        id: copy_id!(ctx, region_id),
        body: copy_id!(ctx, body_id),
        shells: vec![copy_id!(ctx, stage.shell)],
    });
    ir.model.bodies.push(Body {
        id: copy_id!(ctx, body_id),
        kind: if extrusion.caps[0] && extrusion.caps[1] {
            BodyKind::Solid
        } else {
            BodyKind::Sheet
        },
        regions: vec![copy_id!(ctx, region_id)],
        transform: None,
        name: association
            .name
            .as_deref()
            .map(|name| ctx.copy_retained_text(name, "Rhino extrusion body name"))
            .transpose()?,
        color: association.color,
        visible: association.visible,
    });
    for id in [
        stage.shell.to_string(),
        region_id.to_string(),
        body_id.to_string(),
    ] {
        annotate_derived(ctx, stage.annotations, &id)?;
    }
    Ok(body_id.to_string())
}

fn key_with(
    ctx: &DecodeContext<'_>,
    key: &IdentityKey,
    tail: std::fmt::Arguments<'_>,
) -> Result<IdentityKey, CandidateError> {
    let tail = IdentityKey::try_new(format!(".{tail}")).map_err(|error| error.to_string())?;
    Ok(key
        .try_clone_for_decode(ctx, "Rhino temporary identity key")?
        .then(tail))
}

/// Entities that a boundary shares between its laterals and the caps.
struct BoundaryEdges {
    /// Curve edges at each cap end, one per piece.
    edges: [Vec<EdgeId>; 2],
    /// Seam edges from the start end to the finish end, one per piece.
    seams: Vec<EdgeId>,
}

struct Stage<'a, T: ExactnessTarget> {
    ctx: &'a DecodeContext<'a>,
    ir: &'a mut CadIr,
    annotations: &'a mut T,
    key: IdentityKey,
    association: &'a SourceObjectAssociation,
    extrusion: &'a DecodedExtrusion,
    shell: ShellId,
}

impl<T: ExactnessTarget> Stage<'_, T> {
    fn key(&self, tail: std::fmt::Arguments<'_>) -> Result<IdentityKey, CandidateError> {
        key_with(self.ctx, &self.key, tail)
    }

    fn derived(&mut self, id: &dyn std::fmt::Display) -> Result<(), CandidateError> {
        annotate_derived(self.ctx, self.annotations, &id.to_string())?;
        Ok(())
    }

    fn source(&self) -> Result<SourceObjectAssociation, CandidateError> {
        Ok(self
            .association
            .try_clone_for_decode(self.ctx, "Rhino source association copy")?)
    }

    fn lateral_coedge(
        &self,
        profile: usize,
        segment: usize,
        slot: usize,
    ) -> Result<CoedgeId, CandidateError> {
        Ok(rhino_id!(
            CoedgeId,
            "coedge",
            self.key(format_args!(
                "profile-{profile}.lateral-{segment}.slot-{slot}"
            ))?
        ))
    }

    fn cap_coedge(
        &self,
        cap: usize,
        profile: usize,
        segment: usize,
    ) -> Result<CoedgeId, CandidateError> {
        Ok(rhino_id!(
            CoedgeId,
            "coedge",
            self.key(format_args!("cap-{cap}.profile-{profile}.seg-{segment}"))?
        ))
    }

    /// Vertices, curve edges at both ends, and seam edges of one boundary.
    fn boundary_edges(
        &mut self,
        profile: usize,
        boundary: &ExtrusionBoundary,
    ) -> Result<BoundaryEdges, CandidateError> {
        let ctx = self.ctx;
        let count = boundary.pieces.len();
        if count == 0 {
            return Err(format!("{STAGING}: profile {profile} has no pieces").into());
        }
        let mut vertices: [Vec<VertexId>; 2] = [
            ctx.collection_vec(count, "Rhino extrusion vertices")?,
            ctx.collection_vec(count, "Rhino extrusion vertices")?,
        ];
        let mut edges: [Vec<EdgeId>; 2] = [
            ctx.collection_vec(count, "Rhino extrusion edges")?,
            ctx.collection_vec(count, "Rhino extrusion edges")?,
        ];
        let mut seams = ctx.collection_vec(count, "Rhino extrusion seams")?;
        for cap in 0..2 {
            for (segment, piece) in boundary.pieces.iter().enumerate() {
                let endpoint = piece_curve(piece, cap)
                    .control_points()
                    .first()
                    .copied()
                    .ok_or_else(|| {
                        format!("{STAGING}: profile {profile} piece {segment} has no pole")
                    })?;
                let key = self.key(format_args!("profile-{profile}.cap-{cap}.seg-{segment}"))?;
                let point_id = rhino_id!(
                    PointId,
                    "point",
                    key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
                );
                let vertex_id = rhino_id!(VertexId, "vertex", key);
                self.ir.model.points.push(Point::new(
                    copy_id!(ctx, point_id),
                    endpoint,
                    Some(self.source()?),
                ));
                self.ir.model.vertices.push(Vertex {
                    id: copy_id!(ctx, vertex_id),
                    point: point_id,
                    tolerance: None,
                });
                self.derived(&vertex_id)?;
                vertices[cap].push(vertex_id);
            }
            for (segment, piece) in boundary.pieces.iter().enumerate() {
                let key = self.key(format_args!("profile-{profile}.cap-{cap}.seg-{segment}"))?;
                let curve_id = rhino_id!(
                    CurveId,
                    "curve",
                    key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
                );
                let edge_id = rhino_id!(EdgeId, "edge", key);
                let nurbs = piece_curve(piece, cap);
                let range = nurbs_range(nurbs).ok_or_else(|| {
                    format!("{STAGING}: profile {profile} piece {segment} has no domain")
                })?;
                self.ir.model.curves.push(Curve {
                    parameter_range: None,
                    id: copy_id!(ctx, curve_id),
                    geometry: CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(
                        nurbs.try_clone_for_decode(ctx, "Rhino extrusion piece curve copy")?,
                    )),
                    source_object: Some(self.source()?),
                });
                self.ir.model.edges.push(Edge {
                    id: copy_id!(ctx, edge_id),
                    carrier: EdgeCarrier::new(Some(copy_id!(ctx, curve_id)), Some(range))
                        .map_err(|error| format!("{STAGING}: {error}"))?,
                    start: copy_id!(ctx, vertices[cap][segment]),
                    end: copy_id!(ctx, vertices[cap][(segment + 1) % count]),
                    tolerance: None,
                });
                self.derived(&curve_id)?;
                self.derived(&edge_id)?;
                edges[cap].push(edge_id);
            }
        }
        for (segment, (bottom, top)) in vertices[0].iter().zip(&vertices[1]).enumerate() {
            let key = self.key(format_args!("profile-{profile}.seam-{segment}"))?;
            let curve_id = rhino_id!(
                CurveId,
                "curve",
                key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
            );
            let edge_id = rhino_id!(EdgeId, "edge", key);
            let [start, end] = [0, 1].map(|cap| {
                piece_curve(&boundary.pieces[segment], cap)
                    .control_points()
                    .first()
                    .copied()
            });
            let (Some(start), Some(end)) = (start, end) else {
                return Err(
                    format!("{STAGING}: profile {profile} piece {segment} has no pole").into(),
                );
            };
            let delta = end.get().vector_from(start.get());
            let (direction, length) =
                UnitVector3::normalized_with_length(delta).ok_or_else(|| {
                    format!("{STAGING}: profile {profile} seam {segment} is degenerate")
                })?;
            self.ir.model.curves.push(Curve {
                parameter_range: None,
                id: copy_id!(ctx, curve_id),
                geometry: CurveGeometry::Solved(SolvedCurveGeometry::Line(
                    cadmpeg_ir::geometry::analytic::LineCurve::new(start, direction),
                )),
                source_object: Some(self.source()?),
            });
            self.ir.model.edges.push(Edge {
                id: copy_id!(ctx, edge_id),
                carrier: EdgeCarrier::new(Some(copy_id!(ctx, curve_id)), Some([0.0, length.get()]))
                    .map_err(|error| format!("{STAGING}: {error}"))?,
                start: copy_id!(ctx, bottom),
                end: copy_id!(ctx, top),
                tolerance: None,
            });
            self.derived(&curve_id)?;
            self.derived(&edge_id)?;
            seams.push(edge_id);
        }
        Ok(BoundaryEdges { edges, seams })
    }

    /// One lateral face per piece of a boundary.
    fn laterals(
        &mut self,
        profile: usize,
        boundary: &ExtrusionBoundary,
        edges: &BoundaryEdges,
        face_ids: &mut Vec<FaceId>,
    ) -> Result<(), CandidateError> {
        let ctx = self.ctx;
        let extrusion = self.extrusion;
        let count = boundary.pieces.len();
        let mut procedures = ctx.collection_vec(count, "Rhino extrusion lateral procedures")?;
        for (segment, piece) in boundary.pieces.iter().enumerate() {
            let key = self.key(format_args!("profile-{profile}.lateral-{segment}"))?;
            let surface_id = rhino_id!(
                SurfaceId,
                "surface",
                key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
            );
            let face_id = rhino_id!(
                FaceId,
                "face",
                key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
            );
            let loop_id = rhino_id!(
                LoopId,
                "loop",
                key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
            );
            let procedure_id = rhino_id!(ProceduralSurfaceId, "procedural-surface", key);
            let lateral = crate::surfaces::extrusion_nurbs(
                ctx,
                &piece.start_nurbs,
                &piece.end_nurbs,
                extrusion.path_domain,
                extrusion.transposed,
                0,
            )
            .map_err(|error| format!("{STAGING}: lateral surface: {error}"))?;
            self.ir.model.surfaces.push(Surface {
                id: copy_id!(ctx, surface_id),
                geometry: SurfaceGeometry::Solved(SolvedSurfaceGeometry::Nurbs(lateral)),
                source_object: Some(self.source()?),
            });
            let directrix = rhino_id!(
                CurveId,
                "curve",
                self.key(format_args!("profile-{profile}.cap-0.seg-{segment}"))?
            );
            let payload =
                cadmpeg_ir::geometry::surface_payloads::ExtrusionSurfaceConstruction::try_new(
                    directrix,
                    None,
                    extrusion.direction,
                    None,
                    cadmpeg_ir::geometry::CacheContract::from_form(None),
                )
                .map_err(|error| CandidateError::Admission(error.to_string()))?;
            procedures.push((
                copy_id!(ctx, surface_id),
                ProceduralSurface::new(
                    copy_id!(ctx, procedure_id),
                    ProceduralSurfaceDefinition::Extrusion(payload),
                    None,
                ),
            ));

            // The ring runs counterclockwise about the sweep normal: bottom
            // edge, finish seam up, top edge back, start seam down.
            let next = (segment + 1) % count;
            let previous = (segment + count - 1) % count;
            let members = [
                (&edges.edges[0][segment], Sense::Forward),
                (&edges.seams[next], Sense::Forward),
                (&edges.edges[1][segment], Sense::Reversed),
                (&edges.seams[segment], Sense::Reversed),
            ];
            let partners = [
                extrusion.caps[0]
                    .then(|| self.cap_coedge(0, profile, segment))
                    .transpose()?,
                Some(self.lateral_coedge(profile, next, 3)?),
                extrusion.caps[1]
                    .then(|| self.cap_coedge(1, profile, segment))
                    .transpose()?,
                Some(self.lateral_coedge(profile, previous, 1)?),
            ];
            let mut ring = ctx.collection_vec(members.len(), "Rhino extrusion lateral ring")?;
            for (slot, ((edge, sense), partner)) in members.into_iter().zip(partners).enumerate() {
                let coedge_id = self.lateral_coedge(profile, segment, slot)?;
                // A coedge alone on its edge names itself.
                let radial_next = match partner {
                    Some(partner) => partner,
                    None => copy_id!(ctx, coedge_id),
                };
                self.ir.model.coedges.push(Coedge {
                    id: copy_id!(ctx, coedge_id),
                    owner_loop: copy_id!(ctx, loop_id),
                    edge: copy_id!(ctx, edge),
                    radial_next,
                    sense,
                    pcurves: Vec::new(),
                    use_curve: None,
                });
                self.derived(&coedge_id)?;
                ring.push(coedge_id);
            }
            self.ir.model.loops.push(Loop {
                id: copy_id!(ctx, loop_id),
                face: copy_id!(ctx, face_id),
                boundary: LoopBoundary::Ring(
                    LoopRing::new(ctx, ring, Vec::new())
                        .map_err(cadmpeg_core::CodecError::from)?
                        .map_err(|error| format!("{STAGING}: {error}"))?,
                ),
            });
            self.ir.model.faces.push(Face {
                id: copy_id!(ctx, face_id),
                shell: copy_id!(ctx, self.shell),
                surface: copy_id!(ctx, surface_id),
                sense: if extrusion.transposed {
                    Sense::Reversed
                } else {
                    Sense::Forward
                },
                loops: FaceLoops::classified(copy_id!(ctx, loop_id), Vec::new()),
                name: None,
                color: self.association.color,
                tolerance: None,
            });
            self.derived(&surface_id)?;
            self.derived(&procedure_id)?;
            self.derived(&loop_id)?;
            self.derived(&face_id)?;
            face_ids.push(face_id);
        }
        self.ir
            .model
            .add_procedural_surfaces(ctx, procedures)?
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    /// One planar cap face with an outer loop and the hole loops.
    fn cap(
        &mut self,
        cap: usize,
        boundary_edges: &[BoundaryEdges],
    ) -> Result<FaceId, CandidateError> {
        let ctx = self.ctx;
        let extrusion = self.extrusion;
        let key = self.key(format_args!("cap-{cap}"))?;
        let surface_id = rhino_id!(
            SurfaceId,
            "surface",
            key.try_clone_for_decode(ctx, "Rhino temporary identity key")?
        );
        let face_id = rhino_id!(FaceId, "face", key);
        let frame = cadmpeg_ir::units::OrthonormalFrame3::from_units(
            extrusion.cap_normals[cap],
            extrusion.cap_u_axes[cap],
        )
        .ok_or_else(|| {
            "extrusion cap staging: PlaneSurface.normal/u_axis must form an orthonormal frame"
                .to_string()
        })?;
        let origin = cadmpeg_ir::features::FinitePoint3::new(extrusion.cap_origins[cap])
            .ok_or_else(|| {
                "extrusion cap staging: PlaneSurface.origin must be finite".to_string()
            })?;
        self.ir.model.surfaces.push(Surface {
            id: copy_id!(ctx, surface_id),
            geometry: SurfaceGeometry::Solved(SolvedSurfaceGeometry::Plane(
                cadmpeg_ir::geometry::analytic::PlaneSurface::new(origin, frame),
            )),
            source_object: Some(self.source()?),
        });
        let mut loop_ids =
            ctx.collection_vec(extrusion.boundaries.len(), "Rhino extrusion cap loop IDs")?;
        for (profile, boundary) in extrusion.boundaries.iter().enumerate() {
            let loop_id = rhino_id!(
                LoopId,
                "loop",
                self.key(format_args!("cap-{cap}.profile-{profile}"))?
            );
            let count = boundary.pieces.len();
            let mut ring = ctx.collection_vec(count, "Rhino extrusion cap ring")?;
            for step in 0..count {
                // The start cap faces against the path, so its ring runs backwards.
                let segment = if cap == 0 { count - 1 - step } else { step };
                let coedge_id = self.cap_coedge(cap, profile, segment)?;
                let pcurve_id = rhino_id!(
                    PcurveId,
                    "pcurve",
                    self.key(format_args!("cap-{cap}.profile-{profile}.seg-{segment}"))?
                );
                push_cap_pcurve(
                    ctx,
                    self.ir,
                    &pcurve_id,
                    piece_pcurve(&boundary.pieces[segment], cap),
                )?;
                self.derived(&pcurve_id)?;
                let radial_next =
                    self.lateral_coedge(profile, segment, if cap == 0 { 0 } else { 2 })?;
                self.ir.model.coedges.push(Coedge {
                    id: copy_id!(ctx, coedge_id),
                    owner_loop: copy_id!(ctx, loop_id),
                    edge: copy_id!(ctx, boundary_edges[profile].edges[cap][segment]),
                    radial_next,
                    sense: if cap == 0 {
                        Sense::Reversed
                    } else {
                        Sense::Forward
                    },
                    pcurves: vec![PcurveUse {
                        pcurve: pcurve_id,
                        isoparametric: None,
                        parameter_range: None,
                    }],
                    use_curve: None,
                });
                self.derived(&coedge_id)?;
                ring.push(coedge_id);
            }
            self.ir.model.loops.push(Loop {
                id: copy_id!(ctx, loop_id),
                face: copy_id!(ctx, face_id),
                boundary: LoopBoundary::Ring(
                    LoopRing::new(ctx, ring, Vec::new())
                        .map_err(cadmpeg_core::CodecError::from)?
                        .map_err(|error| format!("{STAGING}: {error}"))?,
                ),
            });
            self.derived(&loop_id)?;
            loop_ids.push(loop_id);
        }
        let mut loop_ids = loop_ids.into_iter();
        let outer = loop_ids
            .next()
            .ok_or_else(|| format!("{STAGING}: cap {cap} has no outer loop"))?;
        self.ir.model.faces.push(Face {
            id: copy_id!(ctx, face_id),
            shell: copy_id!(ctx, self.shell),
            surface: copy_id!(ctx, surface_id),
            sense: if cap == 0 {
                Sense::Reversed
            } else {
                Sense::Forward
            },
            loops: FaceLoops::classified(outer, loop_ids.collect()),
            name: None,
            color: self.association.color,
            tolerance: None,
        });
        self.derived(&surface_id)?;
        self.derived(&face_id)?;
        Ok(face_id)
    }
}

fn piece_curve(piece: &ExtrusionPiece, cap: usize) -> &NurbsCurve {
    if cap == 0 {
        &piece.start_nurbs
    } else {
        &piece.end_nurbs
    }
}

fn piece_pcurve(piece: &ExtrusionPiece, cap: usize) -> &CapPcurve {
    if cap == 0 {
        &piece.start_pcurve
    } else {
        &piece.end_pcurve
    }
}

fn nurbs_range(curve: &NurbsCurve) -> Option<[f64; 2]> {
    Some(cadmpeg_ir::eval::nurbs_curve_parameter_domain(curve)?.endpoints())
}

fn push_cap_pcurve(
    ctx: &DecodeContext<'_>,
    ir: &mut CadIr,
    id: &PcurveId,
    pcurve: &CapPcurve,
) -> Result<(), CandidateError> {
    let degree = usize::try_from(pcurve.degree).map_err(|error| {
        format!(
            "extrusion cap staging: pcurve degree {}: {error}",
            pcurve.degree
        )
    })?;
    let end_index = degree
        .checked_add(1)
        .and_then(|order| pcurve.knots.len().checked_sub(order))
        .ok_or_else(|| {
            format!(
                "extrusion cap staging: pcurve knot count {} cannot supply degree {degree} support",
                pcurve.knots.len()
            )
        })?;
    let parameter_range = pcurve
        .knots
        .get(degree)
        .copied()
        .zip(pcurve.knots.get(end_index).copied())
        .map(|(start, end)| [start, end])
        .ok_or_else(|| {
            format!(
                "extrusion cap staging: pcurve parameter range indexes {degree} and {end_index} exceed knot count {}",
                pcurve.knots.len()
            )
        })?;
    let nurbs = PcurveNurbs::from_lanes(
        ctx,
        pcurve.degree,
        ctx.copy_slice(&pcurve.knots, "Rhino extrusion cap pcurve knots")?,
        ctx.copy_slice(&pcurve.control_points, "Rhino extrusion cap pcurve poles")?,
        pcurve
            .weights
            .as_ref()
            .map(|weights| ctx.copy_slice(weights, "Rhino extrusion cap pcurve weights"))
            .transpose()?,
        pcurve.periodic,
    )?
    .map_err(|error| format!("extrusion cap staging: {error}"))?;
    ir.model.pcurves.push(Pcurve {
        id: copy_id!(ctx, id),
        geometry: PcurveGeometry::Nurbs { nurbs },
        metadata: cadmpeg_ir::geometry::pcurve::PcurveMetadata::general(
            None,
            Some(FiniteVector::new(parameter_range).ok_or_else(|| {
                format!(
                    "extrusion cap staging: {}",
                    cadmpeg_ir::geometry::pcurve::PcurveMetadata::NON_FINITE_PARAMETER_RANGE
                )
            })?),
            None,
        ),
    });
    Ok(())
}
