// SPDX-License-Identifier: Apache-2.0
//! Supported constructions remain writable without a solved surface cache.

use cadmpeg_ir::{
    codec::{Codec, DecodeOptions},
    document::admission::StandardAdmission,
    geometry::{
        analytic::LineCurve,
        surface_payloads::{
            ExtrusionSurfaceConstruction, LinearSweepSurfaceConstruction,
            RevolutionSurfaceConstruction,
        },
        Curve, CurveGeometry, ProceduralSurface, ProceduralSurfaceDefinition, SolvedCurveGeometry,
        SolvedSurfaceGeometry, SurfaceGeometry,
    },
    ids::{CurveId, ProceduralSurfaceId},
};
use std::io::Cursor;

#[test]
fn a_writable_surface_construction_preserves_faces_without_a_solved_cache() {
    for opaque_cache in [false, true] {
        let mut ir = cadmpeg_ir::examples::unit_cube().unwrap();
        let SurfaceGeometry::Solved(SolvedSurfaceGeometry::Plane(plane)) =
            &ir.model.surfaces[0].geometry
        else {
            panic!("cube plane")
        };
        let directrix = CurveId::mint("test:model:curve#directrix").unwrap();
        let direction = plane.frame().binormal().into();
        ir.model.curves.push(Curve {
            id: directrix.clone(),
            geometry: CurveGeometry::Solved(SolvedCurveGeometry::Line(
                LineCurve::try_new(plane.origin().get(), *plane.frame().reference().as_raw())
                    .unwrap(),
            )),
            parameter_range: None,
            source_object: None,
        });
        let owner = ir.model.surfaces[0].id.clone();
        ir.model
            .add_procedural_surface(
                &StandardAdmission,
                &owner,
                ProceduralSurface::new(
                    ProceduralSurfaceId::mint("test:model:procedural-surface#sweep").unwrap(),
                    ProceduralSurfaceDefinition::LinearSweep(
                        LinearSweepSurfaceConstruction::try_new(directrix, direction).unwrap(),
                    ),
                    None,
                ),
            )
            .unwrap()
            .unwrap();
        let SurfaceGeometry::Procedural { cache, .. } = &mut ir.model.surfaces[0].geometry else {
            panic!("construction owner")
        };
        *cache = opaque_cache.then_some(SolvedSurfaceGeometry::Unknown { record: None });
        let checked = cadmpeg_ir::validate_neutral(&ir, Vec::new()).unwrap();
        assert!(checked.is_ok(), "{:?}", checked.findings);
        let mut bytes = Vec::new();
        let report = super::write_step(
            &ir,
            &mut bytes,
            crate::StepSchema::Ap214,
            &crate::StepWriteOptions::default(),
        )
        .unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert_eq!(text.matches("ADVANCED_FACE").count(), 6);
        assert!(text.contains("SURFACE_OF_LINEAR_EXTRUSION"));
        assert!(!report
            .losses
            .iter()
            .any(|loss| loss.code == crate::loss::StepLossCode::UnknownSurfaceFaceOmitted.kind()));
        let decoded = crate::StepCodec::default()
            .decode(&mut Cursor::new(bytes), &DecodeOptions::default())
            .unwrap();
        assert_eq!(decoded.ir().model.faces.len(), 6);
        assert_eq!(decoded.ir().model.bodies.len(), 1);
        let checked = cadmpeg_ir::validate_neutral(decoded.ir(), Vec::new()).unwrap();
        assert!(checked.is_ok(), "{:?}", checked.findings);
    }
}

/// Writes the unit cube with its first face on `definition` and no solved cache.
fn cube_with_opaque_construction(
    definition: impl FnOnce(
        CurveId,
        &cadmpeg_ir::geometry::analytic::PlaneSurface,
    ) -> ProceduralSurfaceDefinition,
) -> (String, super::ExportReport) {
    let mut ir = cadmpeg_ir::examples::unit_cube().unwrap();
    let SurfaceGeometry::Solved(SolvedSurfaceGeometry::Plane(plane)) =
        ir.model.surfaces[0].geometry.clone()
    else {
        panic!("cube plane")
    };
    let directrix = CurveId::mint("test:model:curve#directrix").unwrap();
    ir.model.curves.push(Curve {
        id: directrix.clone(),
        geometry: CurveGeometry::Solved(SolvedCurveGeometry::Line(
            LineCurve::try_new(plane.origin().get(), *plane.frame().reference().as_raw()).unwrap(),
        )),
        parameter_range: None,
        source_object: None,
    });
    let owner = ir.model.surfaces[0].id.clone();
    ir.model
        .add_procedural_surface(
            &StandardAdmission,
            &owner,
            ProceduralSurface::new(
                ProceduralSurfaceId::mint("test:model:procedural-surface#construction").unwrap(),
                definition(directrix, &plane),
                None,
            ),
        )
        .unwrap()
        .unwrap();
    let SurfaceGeometry::Procedural { cache, .. } = &mut ir.model.surfaces[0].geometry else {
        panic!("construction owner")
    };
    *cache = Some(SolvedSurfaceGeometry::Unknown { record: None });
    let mut bytes = Vec::new();
    let report = super::write_step(
        &ir,
        &mut bytes,
        crate::StepSchema::Ap214,
        &crate::StepWriteOptions::default(),
    )
    .unwrap();
    (String::from_utf8(bytes).unwrap(), report)
}

fn omits_an_unknown_surface_face(report: &super::ExportReport) -> bool {
    report
        .losses
        .iter()
        .any(|loss| loss.code == crate::loss::StepLossCode::UnknownSurfaceFaceOmitted.kind())
}

#[test]
fn a_bounded_extrusion_is_written_as_a_linear_extrusion() {
    let (text, report) = cube_with_opaque_construction(|directrix, plane| {
        ProceduralSurfaceDefinition::Extrusion(ExtrusionSurfaceConstruction::legacy(
            directrix,
            None,
            plane.frame().binormal().into(),
            None,
            None,
        ))
    });
    assert_eq!(text.matches("ADVANCED_FACE").count(), 6);
    assert!(text.contains("SURFACE_OF_LINEAR_EXTRUSION"));
    assert!(!omits_an_unknown_surface_face(&report));
}

#[test]
fn a_transposed_full_turn_is_written_as_a_surface_of_revolution() {
    let revolution = |transposed: bool, interval: [f64; 2]| {
        move |directrix: CurveId, plane: &cadmpeg_ir::geometry::analytic::PlaneSurface| {
            let axis = cadmpeg_ir::geometry::surface_payloads::admit_revolution_axis(
                plane.origin().get(),
                plane.frame().binormal().into(),
            )
            .unwrap();
            ProceduralSurfaceDefinition::Revolution(
                RevolutionSurfaceConstruction::try_new(
                    directrix,
                    axis,
                    interval,
                    None,
                    None,
                    transposed,
                    cadmpeg_ir::geometry::CacheContract::from_form(None),
                )
                .unwrap(),
            )
        }
    };
    let (text, report) =
        cube_with_opaque_construction(revolution(true, [0.0, std::f64::consts::TAU]));
    assert_eq!(text.matches("ADVANCED_FACE").count(), 6);
    assert!(text.contains("SURFACE_OF_REVOLUTION"));
    assert!(!omits_an_unknown_surface_face(&report));

    // A chart the writer cannot state stays unwritten and the face is reported.
    for (transposed, interval) in [(false, [0.0, std::f64::consts::TAU]), (true, [0.0, 1.0])] {
        let (text, report) = cube_with_opaque_construction(revolution(transposed, interval));
        assert_eq!(text.matches("ADVANCED_FACE").count(), 5);
        assert!(!text.contains("SURFACE_OF_REVOLUTION"));
        assert!(omits_an_unknown_surface_face(&report));
    }
}

#[test]
fn a_sheet_body_writes_a_shell_that_meets_every_edge_twice_as_closed() {
    use cadmpeg_ir::topology::{BodyKind, Shell};
    let write = |ir: &cadmpeg_ir::document::CadIr| {
        let mut bytes = Vec::new();
        super::write_step(
            ir,
            &mut bytes,
            crate::StepSchema::Ap214,
            &crate::StepWriteOptions::default(),
        )
        .unwrap();
        String::from_utf8(bytes).unwrap()
    };

    let mut closed = cadmpeg_ir::examples::unit_cube().unwrap();
    closed.model.bodies[0].kind = BodyKind::Sheet;
    let text = write(&closed);
    assert!(text.contains("SHELL_BASED_SURFACE_MODEL"));
    assert!(text.contains("CLOSED_SHELL"));
    assert!(!text.contains("OPEN_SHELL"));
    assert!(!text.contains("MANIFOLD_SOLID_BREP"));
    let decoded = crate::StepCodec::default()
        .decode(
            &mut Cursor::new(text.into_bytes()),
            &DecodeOptions::default(),
        )
        .unwrap();
    assert_eq!(decoded.ir().model.bodies.len(), 1);
    assert_eq!(decoded.ir().model.bodies[0].kind, BodyKind::Sheet);

    let mut open = cadmpeg_ir::examples::unit_cube().unwrap();
    open.model.bodies[0].kind = BodyKind::Sheet;
    let first_face = open.model.shells[0].faces()[0].clone();
    let (id, region) = (
        open.model.shells[0].id.clone(),
        open.model.shells[0].region.clone(),
    );
    open.model.shells[0] =
        Shell::new(id, region, vec![first_face], Vec::new(), Vec::new()).unwrap();
    let text = write(&open);
    assert!(text.contains("OPEN_SHELL"));
    assert!(!text.contains("CLOSED_SHELL"));
}

#[test]
fn a_solid_that_loses_a_face_is_written_as_a_surface_model() {
    let mut ir = cadmpeg_ir::examples::unit_cube().unwrap();
    ir.model.surfaces[0].geometry =
        SurfaceGeometry::Solved(SolvedSurfaceGeometry::Unknown { record: None });
    let mut bytes = Vec::new();
    let report = super::write_step(
        &ir,
        &mut bytes,
        crate::StepSchema::Ap214,
        &crate::StepWriteOptions::default(),
    )
    .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(text.matches("ADVANCED_FACE").count(), 5);
    assert!(text.contains("SHELL_BASED_SURFACE_MODEL"));
    assert!(text.contains("OPEN_SHELL"));
    assert!(!text.contains("MANIFOLD_SOLID_BREP"));
    assert!(!text.contains("CLOSED_SHELL"));
    assert!(report
        .losses
        .iter()
        .any(|loss| loss.code == crate::loss::StepLossCode::UnknownSurfaceFaceOmitted.kind()));
}

#[test]
fn a_solid_that_keeps_every_face_is_written_as_a_solid() {
    let ir = cadmpeg_ir::examples::unit_cube().unwrap();
    let mut bytes = Vec::new();
    super::write_step(
        &ir,
        &mut bytes,
        crate::StepSchema::Ap214,
        &crate::StepWriteOptions::default(),
    )
    .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("MANIFOLD_SOLID_BREP"));
    assert!(text.contains("CLOSED_SHELL"));
    assert!(!text.contains("OPEN_SHELL"));
}
