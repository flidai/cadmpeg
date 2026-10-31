// SPDX-License-Identifier: Apache-2.0
//! The STEP bounds of a face whose outer boundary is the natural boundary of
//! its curve-bounded surface's support.

use cadmpeg_ir::examples::unit_cube;
use cadmpeg_ir::geometry::analytic::{SphereSurface, TorusSurface};
use cadmpeg_ir::geometry::{SolvedSurfaceGeometry, SurfaceGeometry};
use cadmpeg_ir::math::{Point3, Vector3};
use cadmpeg_ir::topology::{Face, FaceLoops, Sense, Shell};

use crate::export::write_step;
use crate::loss::StepLossCode;
use crate::test_support::curve_bounded::{
    implicit_outer_sheet, unbounded_plane, unit_square_patch,
};
use crate::{StepSchema, StepWriteOptions};

fn written(ir: &cadmpeg_ir::CadIr) -> (String, Vec<cadmpeg_ir::report::loss::LossNote>) {
    let mut output = Vec::new();
    let report = write_step(
        ir,
        &mut output,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .expect("report mode writes the document");
    (String::from_utf8(output).expect("utf8"), report.losses)
}

fn records(text: &str, entity: &str) -> usize {
    text.matches(&format!("= {entity}(")).count()
}

/// A plane has no natural boundary, so the face's domain is not finite and
/// STEP has no face for it: the writer omits the face and states why.
#[test]
fn implicit_outer_on_an_unbounded_plane_omits_the_face_with_a_loss() {
    let (text, losses) = written(&implicit_outer_sheet(unbounded_plane(), Sense::Forward));
    assert!(losses.iter().any(|loss| {
        loss.code == StepLossCode::FaceImplicitOuterBoundUnwritable.kind()
            && loss.severity == cadmpeg_ir::report::Severity::Error
            && loss.message.contains("the support is unbounded")
    }));
    assert_eq!(records(&text, "ADVANCED_FACE"), 0);
    assert_eq!(records(&text, "FACE_BOUND"), 0);
}

/// A NURBS support states its parameter rectangle, which the writer emits as
/// the face's outer bound beside the hole's bound.
#[test]
fn implicit_outer_on_a_nurbs_patch_writes_the_parameter_rectangle() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let (text, losses) = written(&implicit_outer_sheet(unit_square_patch(), sense));
        assert!(!losses
            .iter()
            .any(|loss| loss.code == StepLossCode::FaceImplicitOuterBoundUnwritable.kind()));
        assert_eq!(records(&text, "ADVANCED_FACE"), 1);
        assert_eq!(records(&text, "FACE_OUTER_BOUND"), 1);
        assert_eq!(records(&text, "FACE_BOUND"), 1);
        assert_eq!(records(&text, "EDGE_CURVE"), 8);
        assert_eq!(records(&text, "VERTEX_POINT"), 8);
    }
}

/// One body whose only face has no loop and lies on `carrier`.
fn loopless_face_doc(carrier: SolvedSurfaceGeometry) -> cadmpeg_ir::CadIr {
    let mut ir = unit_cube().expect("unit cube fixture is admitted");
    let face = ir.model.faces[0].clone();
    ir.model
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == face.surface)
        .expect("face surface")
        .geometry = SurfaceGeometry::Solved(carrier);
    let shell = Shell::with_face(
        ir.model.shells[0].id.clone(),
        ir.model.shells[0].region.clone(),
        face.id.clone(),
    );
    ir.model.shells = vec![shell];
    ir.model.faces = vec![Face {
        loops: FaceLoops::unspecified(Vec::new()),
        ..face
    }];
    ir.model.loops.clear();
    ir.model.coedges.clear();
    ir
}

fn sphere(radius: f64) -> SolvedSurfaceGeometry {
    SolvedSurfaceGeometry::Sphere(
        SphereSurface::try_new(
            Point3::new(1.0, 2.0, 3.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            radius,
        )
        .expect("sphere"),
    )
}

fn torus(major_radius: f64, minor_radius: f64) -> SolvedSurfaceGeometry {
    SolvedSurfaceGeometry::Torus(
        TorusSurface::try_new(
            Point3::new(1.0, 2.0, 3.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            major_radius,
            minor_radius,
        )
        .expect("torus"),
    )
}

fn no_bounds_loss(losses: &[cadmpeg_ir::report::loss::LossNote]) -> bool {
    losses
        .iter()
        .any(|loss| loss.code == StepLossCode::FaceNoWritableBounds.kind())
}

/// A sphere face with no loop is the whole sphere. STEP bounds it with the
/// `u = 0` meridian from pole to pole, used once in each direction.
#[test]
fn sphere_without_loops_is_bounded_by_its_meridian() {
    let (text, losses) = written(&loopless_face_doc(sphere(2.0)));

    assert!(!no_bounds_loss(&losses));
    assert_eq!(records(&text, "ADVANCED_FACE"), 1);
    assert_eq!(records(&text, "FACE_OUTER_BOUND"), 1);
    assert_eq!(records(&text, "EDGE_LOOP"), 1);
    assert_eq!(records(&text, "EDGE_CURVE"), 1);
    assert_eq!(records(&text, "ORIENTED_EDGE"), 2);
    assert_eq!(records(&text, "VERTEX_POINT"), 2);
    assert_eq!(records(&text, "CIRCLE"), 1);
    assert!(text.contains("CARTESIAN_POINT('',(1.,2.,1.))"));
    assert!(text.contains("CARTESIAN_POINT('',(1.,2.,5.))"));
    assert_eq!(records(&text, "CLOSED_SHELL"), 1);
    assert_eq!(records(&text, "MANIFOLD_SOLID_BREP"), 1);
}

/// A torus face with no loop is bounded by its `u = 0` meridian circle and its
/// outer equator, each closed at the vertex on the reference direction.
#[test]
fn torus_without_loops_is_bounded_by_its_meridian_and_equator() {
    for sense in [Sense::Forward, Sense::Reversed] {
        let mut ir = loopless_face_doc(torus(3.0, 1.0));
        ir.model.faces[0].sense = sense;
        let (text, losses) = written(&ir);

        assert!(!no_bounds_loss(&losses));
        assert_eq!(records(&text, "ADVANCED_FACE"), 1);
        assert_eq!(records(&text, "FACE_OUTER_BOUND"), 1);
        assert_eq!(records(&text, "EDGE_CURVE"), 2);
        assert_eq!(records(&text, "ORIENTED_EDGE"), 4);
        assert_eq!(records(&text, "VERTEX_POINT"), 1);
        assert_eq!(records(&text, "CIRCLE"), 2);
        assert!(text.contains("CARTESIAN_POINT('',(5.,2.,3.))"));
        assert_eq!(records(&text, "MANIFOLD_SOLID_BREP"), 1);
    }
}

/// A tube radius that is not smaller than the major radius leaves the torus
/// without a seam, so the face stays unwritten.
#[test]
fn torus_without_loops_and_without_a_seam_is_omitted_with_a_loss() {
    let (text, losses) = written(&loopless_face_doc(torus(1.0, 1.5)));

    assert!(no_bounds_loss(&losses));
    assert_eq!(records(&text, "ADVANCED_FACE"), 0);
}
