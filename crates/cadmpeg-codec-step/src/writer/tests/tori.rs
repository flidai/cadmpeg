// SPDX-License-Identifier: Apache-2.0
//! STEP writer torus tests.

#![allow(clippy::unwrap_used)]

use cadmpeg_ir::examples::unit_cube;
use cadmpeg_ir::geometry::{SolvedSurfaceGeometry, SurfaceGeometry};
use cadmpeg_ir::math::{Point3, Vector3};

use crate::export::write_step;
use crate::loss::StepLossCode;
use crate::{StepSchema, StepWriteOptions};

/// Writes a document whose single surface is a torus of `minor_radius` attached
/// to a degenerate torus construction, and returns the report with the text.
fn degenerate_torus_report(
    minor_radius: f64,
) -> (cadmpeg_ir::report::export::ExportReport, String) {
    let mut ir = unit_cube().expect("unit cube fixture is admitted");
    ir.model.surfaces[0].geometry = SurfaceGeometry::Solved(SolvedSurfaceGeometry::Torus(
        cadmpeg_ir::geometry::analytic::TorusSurface::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            1.0,
            minor_radius,
        )
        .unwrap(),
    ));
    let owner = ir.model.surfaces[0].id.clone();
    ir.model
        .add_procedural_surface(
            &owner,
            cadmpeg_ir::geometry::ProceduralSurface::new(
                cadmpeg_ir::ids::ProceduralSurfaceId::mint(
                    "test:model:procedural-surface#degenerate_torus",
                )
                .expect("identity grammar"),
                cadmpeg_ir::geometry::ProceduralSurfaceDefinition::DegenerateTorus {
                    select_outer: true,
                },
                None,
            ),
        )
        .expect("a torus carrier admits a degenerate torus construction");

    let mut buf = Vec::new();
    let report = write_step(
        &ir,
        &mut buf,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .unwrap();
    let text = String::from_utf8(buf).expect("STEP output is UTF-8");
    (report, text)
}

#[test]
fn degenerate_torus_wider_than_its_major_radius_is_not_reported() {
    let (report, text) = degenerate_torus_report(2.0);

    assert!(text.contains("DEGENERATE_TOROIDAL_SURFACE"));
    assert!(!report
        .losses
        .iter()
        .any(|loss| loss.code == StepLossCode::AnalyticSurfaceNormalized.kind()));
}

#[test]
fn degenerate_torus_with_a_negative_tube_radius_is_reported() {
    let (report, text) = degenerate_torus_report(-2.0);

    assert!(text.contains("DEGENERATE_TOROIDAL_SURFACE"));
    assert!(report
        .losses
        .iter()
        .any(|loss| loss.code == StepLossCode::AnalyticSurfaceNormalized.kind()));
}

/// Writes a document whose single surface is a torus and returns the report
/// with the text.
fn torus_report(
    major_radius: f64,
    minor_radius: f64,
) -> (cadmpeg_ir::report::export::ExportReport, String) {
    let mut ir = unit_cube().expect("unit cube fixture is admitted");
    ir.model.surfaces[0].geometry = SurfaceGeometry::Solved(SolvedSurfaceGeometry::Torus(
        cadmpeg_ir::geometry::analytic::TorusSurface::try_new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
            Vector3::new(1.0, 0.0, 0.0),
            major_radius,
            minor_radius,
        )
        .unwrap(),
    ));
    let mut buf = Vec::new();
    let report = write_step(
        &ir,
        &mut buf,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .unwrap();
    let text = String::from_utf8(buf).expect("STEP output is UTF-8");
    (report, text)
}

/// The line of the first entity of type `entity`.
fn first_record<'a>(text: &'a str, entity: &str) -> &'a str {
    text.lines()
        .find(|line| line.contains(&format!("= {entity}(")))
        .unwrap_or_else(|| panic!("the file has a {entity} record"))
}

/// The line of entity `#id`.
fn record_of<'a>(text: &'a str, id: &str) -> &'a str {
    text.lines()
        .find(|line| line.starts_with(&format!("{id} = ")))
        .unwrap_or_else(|| panic!("the file has a record {id}"))
}

/// The parameters of a record line, split at top-level commas.
fn parameters(line: &str) -> Vec<&str> {
    let open = line.find('(').expect("record parameters");
    line[open + 1..line.rfind(')').expect("record end")]
        .split(',')
        .collect()
}

#[test]
fn torus_wider_than_its_major_radius_is_written_as_a_revolved_tube() {
    for minor_radius in [2.0, -2.0] {
        let (report, text) = torus_report(1.0, minor_radius);

        assert!(text.contains("= SURFACE_OF_REVOLUTION("));
        assert!(!text.contains("= TOROIDAL_SURFACE("));
        assert!(first_record(&text, "CIRCLE").ends_with(",2.);"));
        assert!(!report
            .losses
            .iter()
            .any(|loss| loss.code == StepLossCode::AnalyticSurfaceNormalized.kind()));
    }
}

/// A negative tube radius adds a half turn to the `v` angle, so the tube circle
/// of a revolved tube starts opposite the reference direction.
#[test]
fn revolved_tube_keeps_the_torus_chart_for_either_tube_sign() {
    let tube_placement_direction = |text: &str, slot: usize| -> Vec<f64> {
        let circle = parameters(first_record(text, "CIRCLE"));
        let placement = parameters(record_of(text, circle[1]));
        let direction = record_of(text, placement[slot]);
        let open = direction.find("'',(").expect("direction components") + 4;
        let close = direction[open..].find(')').expect("direction end") + open;
        direction[open..close]
            .split(',')
            .map(|component| component.parse().expect("component"))
            .collect()
    };
    let (_, positive) = torus_report(1.0, 2.0);
    let (_, negative) = torus_report(1.0, -2.0);

    for text in [&positive, &negative] {
        assert_eq!(tube_placement_direction(text, 2), [0.0, -1.0, 0.0]);
    }
    assert_eq!(tube_placement_direction(&positive, 3), [1.0, 0.0, 0.0]);
    assert_eq!(tube_placement_direction(&negative, 3), [-1.0, 0.0, 0.0]);
}

#[test]
fn torus_narrower_than_its_major_radius_stays_a_toroidal_surface() {
    let (_, text) = torus_report(2.0, 1.0);

    assert!(text.contains("= TOROIDAL_SURFACE("));
    assert!(!text.contains("= SURFACE_OF_REVOLUTION("));
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

/// The revolved tube keeps the signed chart and is not covered by the sense
/// rule of a negative tube radius.
#[test]
fn face_on_a_revolved_tube_keeps_its_sense_for_either_tube_sign() {
    use cadmpeg_ir::topology::Sense;

    for minor_radius in [2.0, -2.0] {
        let text = cube_face_on(torus(1.0, minor_radius), Sense::Forward);
        assert_eq!(face_flag_on(&text, "SURFACE_OF_REVOLUTION"), ".T.");
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
