// SPDX-License-Identifier: Apache-2.0
//! A staged capped extrusion: one shell whose caps and lateral faces share edges.

use super::{
    cap_extrusion, object_record, scan_with_objects, stage_cap_extrusion, with_collection_limit,
    with_expand, ArchiveVersion, DecodeContext,
};
use cadmpeg_ir::document::CadIr;
use cadmpeg_ir::topology::{BodyKind, Sense};

/// Uses of each edge by the coedges of `ir`, with the sense of every use.
fn edge_uses(ir: &CadIr) -> std::collections::BTreeMap<String, Vec<Sense>> {
    let mut uses = std::collections::BTreeMap::<String, Vec<Sense>>::new();
    for coedge in &ir.model.coedges {
        uses.entry(coedge.edge.to_string())
            .or_default()
            .push(coedge.sense);
    }
    uses
}

#[test]
fn extrusion_staging_closes_a_solid_with_caps_and_lateral_faces_on_shared_edges() {
    for (caps, faces, pcurves) in [
        ([true, true], 10, 16),
        ([true, false], 9, 8),
        ([false, true], 9, 8),
    ] {
        let mut ir = CadIr::empty();
        let extrusion = cap_extrusion(caps);
        with_collection_limit(u64::MAX, |ctx| {
            stage_cap_extrusion(ctx, &mut ir, &extrusion).expect("extrusion stages");
        });
        // Two square boundaries of four segments each: eight lateral faces.
        assert_eq!(ir.model.faces.len(), faces);
        assert_eq!(ir.model.bodies.len(), 1);
        assert_eq!(ir.model.regions.len(), 1);
        assert_eq!(ir.model.shells.len(), 1);
        assert_eq!(ir.model.shells[0].faces().len(), faces);
        assert_eq!(
            ir.model.loops.len(),
            faces + usize::from(caps[0]) + usize::from(caps[1])
        );
        assert_eq!(ir.model.pcurves.len(), pcurves);
        assert_eq!(
            ir.model.bodies[0].kind,
            if caps == [true, true] {
                BodyKind::Solid
            } else {
                BodyKind::Sheet
            }
        );
        let uses = edge_uses(&ir);
        assert_eq!(uses.len(), 24);
        let open = uses.values().filter(|senses| senses.len() == 1).count();
        assert_eq!(open, if caps == [true, true] { 0 } else { 8 });
        for senses in uses.values() {
            assert!(matches!(senses.len(), 1 | 2));
            if let [first, second] = senses.as_slice() {
                assert_ne!(first, second, "the two uses of an edge run opposite ways");
            }
        }
        for edge in &ir.model.edges {
            assert_ne!(
                edge.start, edge.end,
                "a four-segment boundary has no closed edge"
            );
        }
        let cap_senses: Vec<_> = ir
            .model
            .faces
            .iter()
            .filter(|face| face.id.as_str().contains(".cap-"))
            .map(|face| (face.id.as_str().ends_with(".cap-0"), face.sense))
            .collect();
        for (start, sense) in cap_senses {
            assert_eq!(
                sense,
                if start {
                    Sense::Reversed
                } else {
                    Sense::Forward
                }
            );
        }
        assert!(ir.model.faces.iter().all(|face| face.loops.len()
            == if face.id.as_str().contains(".cap-") {
                2
            } else {
                1
            }));
        assert!(staged_model_is_admissible(&ir));
    }
}

/// Whether the staged candidate passes the draft admission checks, which leave
/// the arena order to finalization.
fn staged_model_is_admissible(ir: &CadIr) -> bool {
    cadmpeg_ir::admit(
        &cadmpeg_test_support::service_decode_context(),
        ir,
        cadmpeg_ir::RHINO_INSTANCE_CHECKS,
        Vec::new(),
    )
    .expect("resource allocation did not fail")
    .is_ok()
}

#[test]
fn a_closed_smooth_boundary_stages_one_lateral_face_on_a_seam() {
    let mut extrusion = cap_extrusion([true, true]);
    extrusion.boundaries.truncate(1);
    // One piece for the whole boundary: its start and end vertices coincide.
    let boundary = &mut extrusion.boundaries[0];
    boundary.pieces.truncate(1);
    boundary.pieces[0].start_nurbs = boundary.start_nurbs.clone();
    boundary.pieces[0].end_nurbs = boundary.end_nurbs.clone();
    boundary.pieces[0].start_pcurve = boundary.start_pcurve.clone();
    boundary.pieces[0].end_pcurve = boundary.end_pcurve.clone();
    let mut ir = CadIr::empty();
    with_collection_limit(u64::MAX, |ctx| {
        stage_cap_extrusion(ctx, &mut ir, &extrusion).expect("extrusion stages");
    });
    assert_eq!(ir.model.faces.len(), 3);
    let uses = edge_uses(&ir);
    assert_eq!(uses.len(), 3);
    // The seam is used twice by the lateral face, once each way.
    assert!(uses
        .values()
        .all(|senses| senses.len() == 2 && senses[0] != senses[1]));
    assert_eq!(
        ir.model
            .edges
            .iter()
            .filter(|edge| edge.start == edge.end)
            .count(),
        2
    );
    assert!(staged_model_is_admissible(&ir));
}

#[test]
fn a_capped_extrusion_commits_as_one_solid_body() {
    let object = object_record(ArchiveVersion::V5, 8, [0; 16]);
    let scan = scan_with_objects(&[object]);
    with_expand(&scan, |expand| {
        let mut context = DecodeContext::new(&scan, expand).expect("test transaction");
        assert!(context
            .commit_extrusion(0, cap_extrusion([true, true]))
            .expect("candidate validation completes"));
        let model = &context.session.document().model;
        assert_eq!(model.bodies.len(), 1);
        assert_eq!(model.bodies[0].kind, BodyKind::Solid);
        assert_eq!(model.faces.len(), 10);
        assert_eq!(model.procedural_surfaces.len(), 8);
        assert!(context.report.phase_warnings.is_empty());
    });
}
