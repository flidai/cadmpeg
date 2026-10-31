// SPDX-License-Identifier: Apache-2.0

use crate::kernel_header::RefWidth;
use crate::nurbs::pcurve::tests::curve_block;
use crate::nurbs::pcurve::tests::exact_cache_first_curve;
use crate::nurbs::pcurve::tests::pcurve_block;
use crate::nurbs::pcurve::tests::push_f64;
use crate::nurbs::pcurve::tests::push_ident;
use crate::nurbs::pcurve::tests::push_int;
use crate::nurbs::pcurve::tests::push_position;
use crate::nurbs::pcurve::tests::push_string;
use crate::nurbs::pcurve::tests::push_vector;
use crate::nurbs::pcurve::tests::surface_block;
use crate::nurbs::proc_curve::compound_patch_layout;
use crate::nurbs::proc_curve::extrusion_patch_layout;
use crate::nurbs::proc_curve::helix_patch_layout;
use crate::nurbs::proc_curve::subset_patch_layout;
use crate::nurbs::proc_curve::vector_offset_patch_layout;
use crate::nurbs::proc_surface::DecodedProceduralSurfaceDefinition;
use crate::nurbs::toks::lex_test_span;
use crate::nurbs::toks::test_table;
use cadmpeg_ir::features::FinitePoint3;
use cadmpeg_ir::geometry::analytic::CircleCurve;
use cadmpeg_ir::geometry::{CurveGeometry, SolvedCurveGeometry};
use cadmpeg_ir::math::Point3;
use cadmpeg_ir::math::Vector3;
use cadmpeg_ir::scalar::PositiveLength;
use cadmpeg_ir::units::{OrthonormalFrame3, UnitVector3};

#[test]
fn extrusion_layout_walks_modern_and_legacy_names_at_both_widths() {
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        for name in ["cyl_spl_sur", "cylsur"] {
            let mut bytes = Vec::new();
            push_f64(&mut bytes, 99.0);
            push_vector(&mut bytes, [90.0, 91.0, 92.0]);
            push_position(&mut bytes, [93.0, 94.0, 95.0]);
            bytes.push(0x0f);
            push_ident(&mut bytes, name);
            push_f64(&mut bytes, -2.0);
            push_f64(&mut bytes, 3.0);
            push_vector(&mut bytes, [4.0, 5.0, 6.0]);
            push_position(&mut bytes, [7.0, 8.0, 9.0]);
            bytes.extend_from_slice(&curve_block(int_width));
            bytes.extend_from_slice(&surface_block(int_width));
            bytes.push(0x10);

            let layout = extrusion_patch_layout(&bytes, int_width)
                .unwrap_or_else(|| panic!("extrusion layout {name} at width {int_width}"));
            let interval = layout
                .parameter_interval
                .map(|offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()));
            assert_eq!(interval, [-2.0, 3.0]);
            assert_eq!(
                f64::from_le_bytes(
                    bytes[layout.direction..layout.direction + 8]
                        .try_into()
                        .unwrap()
                ),
                4.0
            );
            assert_eq!(
                f64::from_le_bytes(
                    bytes[layout.native_position..layout.native_position + 8]
                        .try_into()
                        .unwrap()
                ),
                7.0
            );
        }
    }
}

#[test]
fn extrusion_definition_decodes_without_a_solved_surface_cache() {
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut bytes = vec![0x0f];
        push_ident(&mut bytes, "cyl_spl_sur");
        push_f64(&mut bytes, -2.0);
        push_f64(&mut bytes, 3.0);
        push_vector(&mut bytes, [4.0, 5.0, 6.0]);
        push_position(&mut bytes, [7.0, 8.0, 9.0]);
        bytes.extend_from_slice(&curve_block(int_width));
        bytes.push(0x10);

        let decoded = crate::nurbs::blend::cyl_spl_sur(
            &lex_test_span(&bytes, int_width).expect("valid single-record byte fixture"),
            None,
        )
        .unwrap_or_else(|| panic!("cache-less extrusion at width {int_width}"));
        assert_eq!(decoded.legacy_cache_fit_tolerance(), None);
        let (definition, _) = decoded.into_parts();
        let DecodedProceduralSurfaceDefinition::Extrusion {
            parameter_interval,
            direction,
            native_position,
            ..
        } = definition
        else {
            panic!("expected extrusion definition")
        };
        assert_eq!(parameter_interval, [-2.0, 3.0]);
        assert_eq!(direction, Vector3::new(40.0, 50.0, 60.0));
        assert_eq!(native_position, Point3::new(70.0, 80.0, 90.0));
    }
}

#[test]
fn helix_layout_walks_optional_range_flags_at_both_widths() {
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut bytes = Vec::new();
        push_f64(&mut bytes, 99.0);
        push_position(&mut bytes, [90.0, 91.0, 92.0]);
        push_vector(&mut bytes, [93.0, 94.0, 95.0]);
        bytes.push(0x0f);
        push_ident(&mut bytes, "helix_int_cur");
        push_int(&mut bytes, 0x04, 23_100, int_width);
        bytes.push(0x0b);
        push_f64(&mut bytes, -1.0);
        push_f64(&mut bytes, 2.0);
        push_position(&mut bytes, [3.0, 4.0, 5.0]);
        push_vector(&mut bytes, [6.0, 7.0, 8.0]);
        push_vector(&mut bytes, [9.0, 10.0, 11.0]);
        push_vector(&mut bytes, [12.0, 13.0, 14.0]);
        push_f64(&mut bytes, 15.0);
        push_vector(&mut bytes, [16.0, 17.0, 18.0]);
        bytes.extend_from_slice(&curve_block(int_width));
        bytes.push(0x10);

        assert!(crate::nurbs::proc_curve::helix_definition(
            &lex_test_span(&bytes, int_width).expect("valid single-record byte fixture")
        )
        .is_some());
        let layout = helix_patch_layout(&bytes, int_width)
            .unwrap_or_else(|| panic!("helix layout at width {int_width}"));
        let range = layout
            .angle_range
            .map(|offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()));
        assert_eq!(range, [-1.0, 2.0]);
        assert_eq!(
            f64::from_le_bytes(
                bytes[layout.frame_vectors[0]..layout.frame_vectors[0] + 8]
                    .try_into()
                    .unwrap()
            ),
            3.0
        );
        assert_eq!(
            f64::from_le_bytes(
                bytes[layout.apex_factor..layout.apex_factor + 8]
                    .try_into()
                    .unwrap()
            ),
            15.0
        );
        assert_eq!(
            f64::from_le_bytes(bytes[layout.axis..layout.axis + 8].try_into().unwrap()),
            16.0
        );
    }
}

#[test]
fn decodes_current_cacheless_helix_record() {
    let hex = "0e08696e7463757276650d0563757276650cffffffff04ffffffff0cffffffff0b0f0d0d68656c69785f696e745f637572043c5a00000a067701e4b803dd04400a0605738860695607401338aee5545e6a7e3cbfab714dc0c45b3c13b8e608728f9dbf14930e205da081e83ffbd1d341709ad73f000000000000000014fbd1d341709ad73f930e205da081e8bf00000000000000001400000000000000000000000000000000cdccccccccccf43f0600000000000000001400000000000000000000000000000000000000000000f03f0d0c6e756c6c5f737572666163650d0c6e756c6c5f737572666163650d066e756c6c62730d066e756c6c6273100b0b11";
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|digits| u8::from_str_radix(std::str::from_utf8(digits).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();

    let [record]: [_; 1] = crate::test_support::sab::frame(&bytes, 0, bytes.len(), RefWidth::Four)
        .expect("valid complete helix record")
        .try_into()
        .expect("one helix record");
    let definition = crate::nurbs::proc_curve::helix_definition(&record.tokens)
        .expect("current cache-less helix definition");
    let crate::nurbs::proc_curve::HelixDefinition {
        angle_range,
        pitch,
        apex_factor,
        axis,
        ..
    } = definition;
    assert!(angle_range[0] < angle_range[1]);
    assert_eq!(pitch, Vector3::new(0.0, 0.0, 13.0));
    assert_eq!(apex_factor, 0.0);
    assert_eq!(axis, Vector3::new(0.0, 0.0, 1.0));
}

#[test]
fn decodes_current_cacheless_helix_surface_at_both_widths() {
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut bytes = vec![0x0f];
        push_ident(&mut bytes, "helix_spl_line");
        push_int(&mut bytes, 0x04, 23_100, int_width);
        for value in [-0.5, 0.5, -2.0, 3.0, 0.0, std::f64::consts::TAU] {
            bytes.push(0x0a);
            push_f64(&mut bytes, value);
        }
        push_position(&mut bytes, [1.0, 2.0, 3.0]);
        push_vector(&mut bytes, [2.0, 0.0, 0.0]);
        push_vector(&mut bytes, [0.0, 2.0, 0.0]);
        push_vector(&mut bytes, [0.0, 0.0, 4.0]);
        push_f64(&mut bytes, 0.25);
        push_vector(&mut bytes, [0.0, 0.0, 1.0]);
        for sentinel in ["null_surface", "null_surface", "nullbs", "nullbs"] {
            push_ident(&mut bytes, sentinel);
        }
        push_vector(&mut bytes, [5.0, 6.0, 7.0]);
        bytes.push(0x10);

        let decoded = crate::nurbs::proc_surface::helix_spl_sur(
            &lex_test_span(&bytes, int_width).expect("valid single-record byte fixture"),
        )
        .unwrap_or_else(|| panic!("current helix surface at width {int_width}"));
        let (definition, _) = decoded.into_parts();
        let DecodedProceduralSurfaceDefinition::Helix(construction) = definition else {
            panic!("expected helix surface definition")
        };
        let path = construction.path();
        let profile = construction.profile();
        let pitch = path.pitch();
        assert_eq!(*pitch, Vector3::new(0.0, 0.0, 40.0));
        assert_eq!(
            *profile,
            cadmpeg_ir::geometry::HelixSurfaceProfile::Line(
                cadmpeg_ir::geometry::HelixLineProfile::try_new(Vector3::new(5.0, 6.0, 70.0))
                    .unwrap()
            )
        );
    }
}

#[test]
fn vector_offset_layout_ignores_outer_vectors_at_both_widths() {
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut bytes = Vec::new();
        push_f64(&mut bytes, 99.0);
        push_vector(&mut bytes, [90.0, 91.0, 92.0]);
        bytes.push(0x0f);
        push_ident(&mut bytes, "offset_int_cur");
        bytes.push(0x0b);
        bytes.extend_from_slice(&curve_block(int_width));
        push_f64(&mut bytes, -2.0);
        push_f64(&mut bytes, 5.0);
        push_vector(&mut bytes, [0.5, -1.0, 2.0]);
        push_string(&mut bytes, "source");
        push_int(&mut bytes, 0x04, 7, int_width);
        push_string(&mut bytes, "offset");
        push_int(&mut bytes, 0x04, 9, int_width);
        bytes.extend_from_slice(&curve_block(int_width));
        bytes.push(0x10);

        let layout = vector_offset_patch_layout(&bytes, int_width)
            .unwrap_or_else(|| panic!("vector-offset layout at width {int_width}"));
        let range = layout
            .parameter_range
            .map(|offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()));
        assert_eq!(range, [-2.0, 5.0]);
        assert_eq!(
            f64::from_le_bytes(bytes[layout.offset..layout.offset + 8].try_into().unwrap()),
            0.5
        );
    }
}

#[test]
fn subset_layout_ignores_outer_curve_cache_at_both_widths() {
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut bytes = curve_block(int_width);
        push_f64(&mut bytes, 99.0);
        bytes.push(0x0f);
        push_ident(&mut bytes, "subset_int_cur");
        bytes.extend_from_slice(&curve_block(int_width));
        push_f64(&mut bytes, -1.5);
        push_f64(&mut bytes, 3.5);
        bytes.extend_from_slice(&curve_block(int_width));
        bytes.push(0x10);

        let layout = subset_patch_layout(&bytes, int_width)
            .unwrap_or_else(|| panic!("subset layout at width {int_width}"));
        let range = layout
            .parameter_range
            .map(|offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()));
        assert_eq!(range, [-1.5, 3.5]);
    }
}

#[test]
fn compound_layout_requires_framed_subtype_at_both_widths() {
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut bytes = Vec::new();
        push_string(&mut bytes, "comp_int_cur");
        push_int(&mut bytes, 0x04, 1, int_width);
        push_f64(&mut bytes, 99.0);
        bytes.push(0x0f);
        push_ident(&mut bytes, "comp_int_cur");
        push_int(&mut bytes, 0x04, 3, int_width);
        for value in [0.0, 0.5, 1.0] {
            push_f64(&mut bytes, value);
        }
        push_int(&mut bytes, 0x04, 2, int_width);
        for value in [-2.0, 4.0] {
            push_f64(&mut bytes, value);
        }
        bytes.push(0x0b);
        bytes.extend_from_slice(&curve_block(int_width));
        bytes.extend_from_slice(&curve_block(int_width));
        bytes.push(0x10);

        let layout = compound_patch_layout(&bytes, int_width)
            .unwrap_or_else(|| panic!("compound layout at width {int_width}"));
        let parameters = layout
            .parameters
            .iter()
            .map(|offset| f64::from_le_bytes(bytes[*offset..*offset + 8].try_into().unwrap()))
            .collect::<Vec<_>>();
        let component_parameters = layout
            .component_parameters
            .iter()
            .map(|offset| f64::from_le_bytes(bytes[*offset..*offset + 8].try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(parameters, [0.0, 0.5, 1.0]);
        assert_eq!(component_parameters, [-2.0, 4.0]);
    }
}

/// A cache-first `off_surf_int_cur` whose embedded base curve is `base`.
fn cache_first_surface_offset(int_width: RefWidth, base: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x0f];
    push_ident(&mut bytes, "off_surf_int_cur");
    push_int(&mut bytes, 0x04, 23_100, int_width);
    push_int(&mut bytes, 0x15, 0, int_width);
    bytes.extend_from_slice(&curve_block(int_width));
    push_f64(&mut bytes, 0.0);
    push_ident(&mut bytes, "plane");
    push_position(&mut bytes, [0.0, 0.0, 0.0]);
    push_vector(&mut bytes, [0.0, 0.0, 1.0]);
    push_vector(&mut bytes, [1.0, 0.0, 0.0]);
    bytes.extend_from_slice(&[0x0b; 5]);
    push_ident(&mut bytes, "null_surface");
    bytes.extend_from_slice(&pcurve_block(int_width));
    push_ident(&mut bytes, "nullbs");
    bytes.extend_from_slice(&[0x0b; 2]);
    for _ in 0..4 {
        push_int(&mut bytes, 0x04, 0, int_width);
    }
    // Base surface U and V intervals.
    for value in [-1.0, 1.0, -1.0, 1.0] {
        bytes.push(0x0a);
        push_f64(&mut bytes, value);
    }
    bytes.extend_from_slice(base);
    // Base endpoints and base interval, stated on the sense-selected curve.
    for value in [0.0, 1.0, 0.0, 1.0] {
        bytes.push(0x0a);
        push_f64(&mut bytes, value);
    }
    for value in [-0.25, 0.0, 1.0] {
        push_f64(&mut bytes, value);
    }
    bytes.push(0x10);
    bytes
}

#[test]
fn reversed_embedded_base_intcurve_of_a_surface_offset_is_held_on_negated_parameter() {
    let asm_decode_arena = cadmpeg_core::decode::DecodeArena::new();
    let (asm_decode_ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
        &[],
        &asm_decode_arena,
        &cadmpeg_core::decode::DecodePolicy::default(),
    )
    .expect("test decode context");
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let base_of = |reversed| {
            // The base curve: the sense, then the exact subtype that owns its
            // cache.
            let mut base = vec![if reversed { 0x0a } else { 0x0b }];
            base.extend_from_slice(&exact_cache_first_curve(int_width));
            let record = cache_first_surface_offset(int_width, &base);
            let decoded = crate::nurbs::proc_curve::procedural_curve_resolving_refs(
                &asm_decode_ctx,
                &lex_test_span(&record, int_width).expect("valid single-record byte fixture"),
                &test_table(&record, int_width).expect("valid single-record byte fixture"),
            )
            .transpose()
            .expect("decode the procedural curve")
            .unwrap_or_else(|| panic!("cache-first surface offset at width {int_width}"));
            let crate::nurbs::proc_curve::ProceduralCurveConstruction::SurfaceOffset(offset) =
                decoded.construction
            else {
                panic!("typed surface-offset construction")
            };
            assert_eq!(offset.base_range, [0.0, 1.0]);
            let CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(base)) = offset.base else {
                panic!("a cached base curve is a NURBS curve")
            };
            base
        };
        let forward = base_of(false);
        let reversed = base_of(true);

        assert_eq!(forward.knots().as_slice().first(), Some(&0.0));
        assert_eq!(forward.knots().as_slice().last(), Some(&1.0));
        // `C(-t)` of the serialized curve: knots negated, poles in reverse.
        assert_eq!(reversed.knots().as_slice().first(), Some(&-1.0));
        assert_eq!(reversed.knots().as_slice().last(), Some(&-0.0));
        let mut expected = forward.control_points();
        expected.reverse();
        assert_eq!(reversed.control_points(), expected);
    }
}

/// An embedded `ellipse` base is stated on the native angle of the conic. A
/// NURBS conversion of it runs over `[0, 1]` and cannot take an angle.
#[test]
fn embedded_ellipse_base_of_a_surface_offset_keeps_the_angle_parameter() {
    let asm_decode_arena = cadmpeg_core::decode::DecodeArena::new();
    let (asm_decode_ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
        &[],
        &asm_decode_arena,
        &cadmpeg_core::decode::DecodePolicy::default(),
    )
    .expect("test decode context");
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        for (ratio, circular) in [(1.0, true), (0.5, false)] {
            let mut base = Vec::new();
            push_ident(&mut base, "ellipse");
            push_position(&mut base, [1.0, 2.0, 3.0]);
            push_vector(&mut base, [0.0, 0.0, 1.0]);
            push_vector(&mut base, [2.0, 0.0, 0.0]);
            push_f64(&mut base, ratio);
            let record = cache_first_surface_offset(int_width, &base);
            let decoded = crate::nurbs::proc_curve::procedural_curve_resolving_refs(
                &asm_decode_ctx,
                &lex_test_span(&record, int_width).expect("valid single-record byte fixture"),
                &test_table(&record, int_width).expect("valid single-record byte fixture"),
            )
            .transpose()
            .expect("decode the procedural curve")
            .unwrap_or_else(|| panic!("cache-first surface offset at width {int_width}"));
            let crate::nurbs::proc_curve::ProceduralCurveConstruction::SurfaceOffset(offset) =
                decoded.construction
            else {
                panic!("typed surface-offset construction")
            };
            match (circular, offset.base) {
                (true, CurveGeometry::Solved(SolvedCurveGeometry::Circle(circle))) => {
                    let frame = OrthonormalFrame3::from_units(
                        UnitVector3::normalized(Vector3::new(0.0, 0.0, 1.0)).unwrap(),
                        UnitVector3::normalized(Vector3::new(1.0, 0.0, 0.0)).unwrap(),
                    )
                    .unwrap();
                    let expected = CircleCurve::new(
                        FinitePoint3::new(Point3::new(10.0, 20.0, 30.0)).unwrap(),
                        frame,
                        PositiveLength::new(20.0).unwrap(),
                    );
                    assert_eq!(circle, expected);
                }
                (false, CurveGeometry::Solved(SolvedCurveGeometry::Ellipse(_))) => {}
                (_, other) => panic!("unexpected base curve {other:?}"),
            }
        }
    }
}
