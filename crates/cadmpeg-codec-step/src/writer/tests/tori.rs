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

/// The `same_sense` flag of the first face of the unit cube on a torus with
/// `R = 1` and tube radius `minor_radius`, with every vertex of the face moved
/// to `position`. The face sense is forward.
fn apple_face_sense(minor_radius: f64, position: [f64; 3]) -> String {
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
    let target =
        cadmpeg_ir::features::FinitePoint3::new(Point3::new(position[0], position[1], position[2]))
            .unwrap();
    let face_loops = ir.model.faces[0].loops.clone();
    let mut points = Vec::new();
    for loop_id in &face_loops {
        let ring = ir
            .model
            .loops
            .iter()
            .find(|ring| &ring.id == loop_id)
            .unwrap();
        for coedge_id in ring.coedges() {
            let coedge = ir
                .model
                .coedges
                .iter()
                .find(|c| &c.id == coedge_id)
                .unwrap();
            let edge = ir.model.edges.iter().find(|e| e.id == coedge.edge).unwrap();
            let vertex = ir
                .model
                .vertices
                .iter()
                .find(|v| v.id == edge.start)
                .unwrap();
            points.push(vertex.point.clone());
        }
    }
    for point in ir
        .model
        .points
        .iter_mut()
        .filter(|p| points.contains(&p.id))
    {
        point.set_position(target);
    }
    let mut buf = Vec::new();
    write_step(
        &ir,
        &mut buf,
        StepSchema::Ap214,
        &StepWriteOptions::default(),
    )
    .unwrap();
    let text = String::from_utf8(buf).expect("STEP output is UTF-8");
    let surface = first_record(&text, "SURFACE_OF_REVOLUTION");
    let surface_id = surface.split(" = ").next().unwrap();
    let line = text
        .lines()
        .find(|line| line.contains("= ADVANCED_FACE(") && line.contains(&format!(",{surface_id},")))
        .expect("the face on the revolved tube");
    line[..line.rfind(')').unwrap()]
        .rsplit(',')
        .next()
        .unwrap()
        .to_owned()
}

/// A face on the outer sheet (`rho > 0`) of a torus wider than its major
/// radius states `same_sense` against the sign of the tube radius, because the
/// face sense of a negative tube radius is relative to the normal turned toward
/// the tube center. A face on the inner sheet keeps the carrier sense: on the
/// reference data the inner-sheet rule was right for some solids and wrong for
/// others, so it is left unflipped.
#[test]
fn face_on_a_torus_wider_than_its_major_radius_follows_the_sheet() {
    // `R = 1`, `|r| = 2`: `v` with `cos v = 0.5` gives `rho = 2` on the outer
    // sheet and `cos v = -0.75` gives `rho = -0.5` on the inner sheet. The
    // point is at radial distance `|rho|` from the axis.
    let outer = [2.0, 0.0, 2.0 * 0.75_f64.sqrt()];
    let inner = [0.5, 0.0, 2.0 * (1.0_f64 - 0.5625).sqrt()];
    assert_eq!(apple_face_sense(2.0, outer), ".T.");
    assert_eq!(apple_face_sense(2.0, inner), ".T.");
    assert_eq!(apple_face_sense(-2.0, outer), ".F.");
    assert_eq!(apple_face_sense(-2.0, inner), ".T.");
}
