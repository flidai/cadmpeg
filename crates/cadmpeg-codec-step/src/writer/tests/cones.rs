// SPDX-License-Identifier: Apache-2.0
//! STEP writer cone chart tests.

#![allow(clippy::unwrap_used)]
#![allow(clippy::default_trait_access)]

use std::io::Cursor;

use cadmpeg_ir::codec::{Codec, DecodeOptions};
use cadmpeg_ir::geometry::{SolvedSurfaceGeometry, SurfaceGeometry};
use cadmpeg_ir::math::{Point3, Vector3};

use cadmpeg_ir::examples::unit_cube;

use super::round_trips::emit_surface_only;
use crate::export::write_step;
use crate::loss::StepLossCode;
use crate::test_support::exchange::export;
use crate::{StepCodec, StepSchema, StepWriteOptions};

#[test]
fn negative_cone_face_pcurve_is_written_in_reversed_chart() {
    let mut ir = StepCodec::default()
        .decode(
            &mut Cursor::new(include_bytes!("../../../tests/fixtures/ap214_sheet.p21")),
            &DecodeOptions::default(),
        )
        .expect("decode sheet pcurve")
        .into_parts()
        .0;
    let surface_id = ir.model.faces[0].surface.clone();
    let surface = ir
        .model
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == surface_id)
        .expect("face surface");
    surface.geometry = SurfaceGeometry::Solved(SolvedSurfaceGeometry::Cone(
        cadmpeg_ir::geometry::analytic::ConeSurface::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            2.0,
            1.0,
            -0.5,
        )
        .expect("negative cone"),
    ));

    let mut output = Vec::new();
    let report = write_step(
        &ir,
        &mut output,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .expect("write cone pcurve");
    let text = String::from_utf8(output).expect("STEP output is UTF-8");
    assert!(text.contains("PCURVE("));
    assert!(text.contains("CURVE_REPLICA("));
    assert!(text.contains("CARTESIAN_TRANSFORMATION_OPERATOR_2D("));
    assert!(text.contains("DIRECTION('',(-1.,0.))"));
    assert!(text.contains("DIRECTION('',(0.,-1.))"));
    assert!(!report
        .losses
        .iter()
        .any(|loss| loss.code == StepLossCode::PcurveCarrierUnwritable.kind()));
}

#[test]
fn unmappable_negative_cone_pcurve_loss_names_its_face() {
    let mut ir = StepCodec::default()
        .decode(
            &mut Cursor::new(include_bytes!("../../../tests/fixtures/ap214_sheet.p21")),
            &DecodeOptions::default(),
        )
        .expect("decode sheet pcurve")
        .into_parts()
        .0;
    let face_id = ir.model.faces[0].id.clone();
    let surface_id = ir.model.faces[0].surface.clone();
    ir.model
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == surface_id)
        .expect("face surface")
        .geometry = SurfaceGeometry::Solved(SolvedSurfaceGeometry::Cone(
        cadmpeg_ir::geometry::analytic::ConeSurface::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            2.0,
            1.0,
            -0.5,
        )
        .expect("negative cone"),
    ));
    ir.model.pcurves[0].geometry = cadmpeg_ir::geometry::pcurve::PcurveGeometry::Harmonic(
        cadmpeg_ir::geometry::pcurve::HarmonicPcurve::try_new(
            cadmpeg_ir::math::Point2::new(0.0, 0.0),
            cadmpeg_ir::math::Point2::new(1.0, 0.0),
            cadmpeg_ir::math::Point2::new(0.0, 1.0),
        )
        .expect("harmonic pcurve"),
    );

    let mut output = Vec::new();
    let report = write_step(
        &ir,
        &mut output,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .expect("write cone without unmappable pcurve");
    assert!(!String::from_utf8(output)
        .expect("STEP output is UTF-8")
        .contains("PCURVE("));
    assert!(report.losses.iter().any(|loss| {
        loss.code == StepLossCode::PcurveCarrierUnwritable.kind()
            && loss.message.contains(face_id.as_str())
            && loss.message.contains(ir.model.pcurves[0].id.as_str())
    }));
}

#[test]
fn zero_half_angle_cone_round_trips_as_a_cylinder_of_the_same_chart() {
    const EPS_CYLINDER_POINT: f64 = 1.0e-9;
    let source = SolvedSurfaceGeometry::Cone(
        cadmpeg_ir::geometry::analytic::ConeSurface::try_new(
            Point3::new(1.0, 2.0, 3.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            2.0,
            1.0,
            0.0,
        )
        .expect("finite cone"),
    );
    let mut emitter = crate::writer::Emitter::new();
    crate::geometry::surface(&mut emitter, &source).expect("surface geometry is writable");
    let text = emitter.into_lines().expect("finite reals").join("\n");
    assert!(text.contains("CYLINDRICAL_SURFACE('',#4,2.)"));
    assert!(!text.contains("CONICAL_SURFACE"));

    let mut ir = unit_cube().expect("unit cube fixture is admitted");
    let source_surface = ir.model.surfaces[0].id.clone();
    let source_face_sense = ir
        .model
        .faces
        .iter()
        .find(|face| face.surface == source_surface)
        .expect("cone face")
        .sense;
    ir.model.surfaces[0].geometry = SurfaceGeometry::Solved(source.clone());

    let decoded = StepCodec::default()
        .decode(&mut Cursor::new(export(&ir)), &DecodeOptions::default())
        .expect("decode written cylinder");
    let (surface_id, written) = decoded
        .ir()
        .model
        .surfaces
        .iter()
        .find_map(|surface| match surface.geometry.solved() {
            Some(SolvedSurfaceGeometry::Cylinder(cylinder)) => {
                Some((&surface.id, SolvedSurfaceGeometry::Cylinder(*cylinder)))
            }
            _ => None,
        })
        .expect("written cylinder surface");
    let written_face = decoded
        .ir()
        .model
        .faces
        .iter()
        .find(|face| &face.surface == surface_id)
        .expect("written cylinder face");
    assert_eq!(written_face.sense, source_face_sense);
    for (u, v) in [(0.0, 0.0), (0.3, 0.7), (-1.2, 1.1), (2.5, -0.4)] {
        let point = |geometry: &SolvedSurfaceGeometry| {
            cadmpeg_ir::eval::decode::surface_point_solved(
                cadmpeg_ir::eval::admission::EvaluationAdmission::Standard,
                geometry,
                u,
                v,
            )
            .expect("surface point")
            .get()
        };
        let (source_point, written_point) = (point(&source), point(&written));
        let distance = (source_point.x - written_point.x)
            .hypot(source_point.y - written_point.y)
            .hypot(source_point.z - written_point.z);
        assert!(distance < EPS_CYLINDER_POINT);
    }
}

#[test]
fn zero_semi_angle_cone_writes_the_cylinder_of_the_same_radius() {
    let source = SolvedSurfaceGeometry::Cone(
        cadmpeg_ir::geometry::analytic::ConeSurface::try_new(
            Point3::new(1.0, 2.0, 3.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            2.0,
            1.0,
            0.0,
        )
        .expect("finite cone"),
    );
    let text = emit_surface_only(&SurfaceGeometry::Solved(source));
    assert!(text.contains("CYLINDRICAL_SURFACE('',#4,2.)"), "{text}");
    assert!(!text.contains("CONICAL_SURFACE"));
}
