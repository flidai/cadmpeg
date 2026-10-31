// SPDX-License-Identifier: Apache-2.0
//! Face sense on spheres and tori whose radius is negative.

#![allow(clippy::unwrap_used)]

use cadmpeg_ir::examples::unit_cube;
use cadmpeg_ir::geometry::{SolvedSurfaceGeometry, SurfaceGeometry};
use cadmpeg_ir::math::{Point3, Vector3};

use crate::export::write_step;
use crate::{StepSchema, StepWriteOptions};

/// The line of the first entity of type `entity`.
fn first_record<'a>(text: &'a str, entity: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(&format!("= {entity}(")))
        .unwrap_or_else(|| panic!("the file has a {entity} record"))
}

/// Writes the unit cube with its first surface replaced by `geometry`, the face
/// on it given `sense`, and returns the text.
fn cube_face_on(geometry: SolvedSurfaceGeometry, sense: cadmpeg_ir::topology::Sense) -> String {
    let mut ir = unit_cube().expect("unit cube fixture is admitted");
    ir.model.surfaces[0].geometry = SurfaceGeometry::Solved(geometry);
    let surface = ir.model.surfaces[0].id.clone();
    let faces = ir
        .model
        .faces
        .iter_mut()
        .filter(|face| face.surface == surface)
        .collect::<Vec<_>>();
    assert_eq!(faces.len(), 1, "one face lies on the replaced surface");
    for face in faces {
        face.sense = sense;
    }
    let mut buf = Vec::new();
    write_step(
        &ir,
        &mut buf,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .unwrap();
    String::from_utf8(buf).expect("STEP output is UTF-8")
}

/// The `same_sense` flag of the face on the first record of type `surface`.
fn face_flag_on(text: &str, surface: &str) -> String {
    let id = first_record(text, surface)
        .split(" = ")
        .next()
        .expect("record id");
    let needle = format!(",{id},");
    let face = text
        .lines()
        .find(|line| line.contains("= ADVANCED_FACE(") && line.contains(&needle))
        .unwrap_or_else(|| panic!("a face lies on {id}"));
    face[face.rfind(',').unwrap() + 1..]
        .trim_end_matches(");")
        .to_owned()
}

fn torus(major_radius: f64, minor_radius: f64) -> SolvedSurfaceGeometry {
    SolvedSurfaceGeometry::Torus(
        cadmpeg_ir::geometry::analytic::TorusSurface::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            major_radius,
            minor_radius,
        )
        .unwrap(),
    )
}

fn sphere(radius: f64) -> SolvedSurfaceGeometry {
    SolvedSurfaceGeometry::Sphere(
        cadmpeg_ir::geometry::analytic::SphereSurface::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            radius,
        )
        .unwrap(),
    )
}

/// A negative tube radius is written as its magnitude on a surface whose
/// normal points away from the tube, so the concave face states the opposite
/// `same_sense` and keeps its physical normal.
#[test]
fn face_on_a_torus_with_a_negative_tube_radius_states_the_opposite_sense() {
    use cadmpeg_ir::topology::Sense;

    for (minor_radius, sense, flag) in [
        (0.5, Sense::Forward, ".T."),
        (0.5, Sense::Reversed, ".F."),
        (-0.5, Sense::Forward, ".F."),
        (-0.5, Sense::Reversed, ".T."),
    ] {
        let text = cube_face_on(torus(2.0, minor_radius), sense);
        assert!(first_record(&text, "TOROIDAL_SURFACE").ends_with(",2.,0.5);"));
        assert_eq!(
            face_flag_on(&text, "TOROIDAL_SURFACE"),
            flag,
            "tube radius {minor_radius}, face {sense:?}"
        );
    }
}

/// A negative sphere radius turns the normal toward the center. The written
/// `SPHERICAL_SURFACE` has a positive radius and an outward normal.
#[test]
fn face_on_a_sphere_with_a_negative_radius_states_the_opposite_sense() {
    use cadmpeg_ir::topology::Sense;

    for (radius, sense, flag) in [
        (3.0, Sense::Forward, ".T."),
        (3.0, Sense::Reversed, ".F."),
        (-3.0, Sense::Forward, ".F."),
        (-3.0, Sense::Reversed, ".T."),
    ] {
        let text = cube_face_on(sphere(radius), sense);
        assert!(first_record(&text, "SPHERICAL_SURFACE").ends_with(",3.);"));
        assert_eq!(
            face_flag_on(&text, "SPHERICAL_SURFACE"),
            flag,
            "radius {radius}, face {sense:?}"
        );
    }
}
