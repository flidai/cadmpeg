// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::unwrap_used)]
//! Body-kind classification from edge coedge-use counts.

use crate::brep::records;
use crate::brep::{id, AsmBrep};
use crate::ids::IdFormat;
use cadmpeg_ir::ids::{EdgeId, FaceId, LoopId, RegionId, ShellId};
use cadmpeg_ir::topology::{Loop, Shell};

const FORMAT: IdFormat = crate::asm_format!("f3d");

#[test]
fn degenerate_pole_edges_do_not_open_a_closed_body() {
    use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy};
    use cadmpeg_ir::ids::{BodyId, CoedgeId, CurveId, VertexId};
    use cadmpeg_ir::topology::{
        Body, BodyKind, Coedge, Edge, EdgeCarrier, Face, FaceLoops, LoopBoundary, LoopRing, Region,
        Sense,
    };

    fn body_with_edges(pole_edge_has_curve: bool) -> BodyKind {
        let mint = |index| id(FORMAT, index).into_string();
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::default()).unwrap();
        let body = BodyId::mint(mint(1)).unwrap();
        let region = RegionId::mint(mint(2)).unwrap();
        let shell = ShellId::mint(mint(3)).unwrap();
        let face = FaceId::mint(mint(4)).unwrap();
        let vertex = VertexId::mint(mint(5)).unwrap();
        let seam = EdgeId::mint(mint(6)).unwrap();
        let pole = EdgeId::mint(mint(7)).unwrap();
        let seam_curve = CurveId::mint(mint(8)).unwrap();
        let pole_curve = CurveId::mint(mint(9)).unwrap();
        let mut brep = AsmBrep::default();
        brep.bodies.push(Body {
            id: body.clone(),
            kind: BodyKind::Solid,
            regions: vec![region.clone()],
            transform: None,
            name: None,
            color: None,
            visible: None,
        });
        brep.regions.push(Region {
            id: region.clone(),
            body,
            shells: vec![shell.clone()],
        });
        // One closed face: a seam edge used twice and one pole edge used once.
        let uses = [(10, &seam), (11, &seam), (12, &pole)];
        let mut loop_ids = Vec::new();
        for (index, edge) in uses {
            let coedge = CoedgeId::mint(mint(index)).unwrap();
            let loop_id = LoopId::mint(mint(index + 10)).unwrap();
            brep.coedges.push(Coedge {
                id: coedge.clone(),
                owner_loop: loop_id.clone(),
                edge: edge.clone(),
                radial_next: coedge.clone(),
                sense: Sense::Forward,
                pcurves: Vec::new(),
                use_curve: None,
            });
            brep.loops.push(Loop {
                id: loop_id.clone(),
                face: face.clone(),
                boundary: LoopBoundary::Ring(LoopRing::single(coedge)),
            });
            loop_ids.push(loop_id);
        }
        brep.edges.push(Edge {
            id: seam.clone(),
            carrier: EdgeCarrier::unbounded(Some(seam_curve)),
            start: vertex.clone(),
            end: vertex.clone(),
            tolerance: None,
        });
        brep.edges.push(Edge {
            id: pole,
            carrier: EdgeCarrier::unbounded(pole_edge_has_curve.then_some(pole_curve)),
            start: vertex.clone(),
            end: vertex,
            tolerance: None,
        });
        brep.faces.push(Face {
            id: face.clone(),
            shell: shell.clone(),
            surface: cadmpeg_ir::ids::SurfaceId::mint(mint(20)).unwrap(),
            sense: Sense::Forward,
            loops: FaceLoops::unspecified(loop_ids),
            name: None,
            color: None,
            tolerance: None,
        });
        brep.shells
            .push(Shell::new(shell, region, vec![face], Vec::new(), Vec::new()).unwrap());
        crate::brep::geometry::classify_body_kinds(&ctx, &mut brep).expect("classification");
        brep.bodies[0].kind
    }

    // A curve-less pole edge is a degenerate collapse; the shell is closed.
    assert_eq!(body_with_edges(false), BodyKind::Solid);
    // A pole edge that carries a curve is a real boundary; the shell is open.
    assert_eq!(body_with_edges(true), BodyKind::Sheet);
}

#[test]
fn single_sided_faces_pair_edges_in_even_counts_to_form_a_solid() {
    use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy};
    use cadmpeg_ir::ids::{BodyId, CoedgeId, CurveId, SurfaceId, VertexId};
    use cadmpeg_ir::topology::{
        Body, BodyKind, Coedge, Edge, EdgeCarrier, Face, FaceLoops, LoopBoundary, LoopRing, Region,
        Sense,
    };

    fn body_with_edge_uses(uses: usize, double_sided: bool) -> BodyKind {
        let mint = |index| id(FORMAT, index).into_string();
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::default()).unwrap();
        let body = BodyId::mint(mint(1)).unwrap();
        let region = RegionId::mint(mint(2)).unwrap();
        let shell = ShellId::mint(mint(3)).unwrap();
        let face = FaceId::mint(mint(4)).unwrap();
        let vertex = VertexId::mint(mint(5)).unwrap();
        let edge = EdgeId::mint(mint(6)).unwrap();
        let mut brep = AsmBrep::default();
        brep.bodies.push(Body {
            id: body.clone(),
            kind: BodyKind::Solid,
            regions: vec![region.clone()],
            transform: None,
            name: None,
            color: None,
            visible: None,
        });
        brep.regions.push(Region {
            id: region.clone(),
            body,
            shells: vec![shell.clone()],
        });
        let mut loop_ids = Vec::new();
        for index in 0..i64::try_from(uses).unwrap() {
            let coedge = CoedgeId::mint(mint(100 + index)).unwrap();
            let loop_id = LoopId::mint(mint(200 + index)).unwrap();
            brep.coedges.push(Coedge {
                id: coedge.clone(),
                owner_loop: loop_id.clone(),
                edge: edge.clone(),
                radial_next: coedge.clone(),
                sense: Sense::Forward,
                pcurves: Vec::new(),
                use_curve: None,
            });
            brep.loops.push(Loop {
                id: loop_id.clone(),
                face: face.clone(),
                boundary: LoopBoundary::Ring(LoopRing::single(coedge)),
            });
            loop_ids.push(loop_id);
        }
        brep.edges.push(Edge {
            id: edge,
            carrier: EdgeCarrier::unbounded(Some(CurveId::mint(mint(8)).unwrap())),
            start: vertex.clone(),
            end: vertex,
            tolerance: None,
        });
        brep.faces.push(Face {
            id: face.clone(),
            shell: shell.clone(),
            surface: SurfaceId::mint(mint(20)).unwrap(),
            sense: Sense::Forward,
            loops: FaceLoops::unspecified(loop_ids),
            name: None,
            color: None,
            tolerance: None,
        });
        brep.face_sidedness.push(records::FaceSidedness {
            source_namespace: records::identity::NativeRecordNamespace::new(FORMAT),
            record_index: 1,
            face: face.clone(),
            native_sense: Sense::Forward,
            carrier_flipped: false,
            containment: double_sided.then_some(records::FaceContainment::In),
        });
        brep.shells
            .push(Shell::new(shell, region, vec![face], Vec::new(), Vec::new()).unwrap());
        crate::brep::geometry::classify_body_kinds(&ctx, &mut brep).expect("classification");
        brep.bodies[0].kind
    }

    assert_eq!(body_with_edge_uses(2, false), BodyKind::Solid);
    // Four single-sided uses: solids that touch along the edge.
    assert_eq!(body_with_edge_uses(4, false), BodyKind::Solid);
    // The same count on a double-sided face is a crossing of sheets.
    assert_eq!(body_with_edge_uses(4, true), BodyKind::Sheet);
    // An odd count leaves a free edge.
    assert_eq!(body_with_edge_uses(3, false), BodyKind::Sheet);
}
