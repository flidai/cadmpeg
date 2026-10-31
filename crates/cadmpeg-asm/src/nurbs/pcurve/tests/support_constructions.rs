// SPDX-License-Identifier: Apache-2.0

use crate::kernel_header::RefWidth;
use crate::nurbs::pcurve::tests::curve_block;
use crate::nurbs::pcurve::tests::pcurve_block;
use crate::nurbs::pcurve::tests::push_f64;
use crate::nurbs::pcurve::tests::push_ident;
use crate::nurbs::pcurve::tests::push_int;
use crate::nurbs::pcurve::tests::surface_block;
use crate::nurbs::toks::lex_test_span;
use crate::nurbs::toks::test_table;
use cadmpeg_ir::geometry::SolvedSurfaceGeometry;
use cadmpeg_ir::geometry::SurfaceGeometry;

#[test]
fn intersection_support_reference_keeps_the_construction_it_names() {
    let asm_decode_arena = cadmpeg_core::decode::DecodeArena::new();
    let (asm_decode_ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
        &[],
        &asm_decode_arena,
        &cadmpeg_core::decode::DecodePolicy::default(),
    )
    .expect("test decode context");
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut support = vec![0x0f];
        push_ident(&mut support, "exactsur");
        support.extend_from_slice(&surface_block(int_width));
        push_f64(&mut support, 0.001);
        for value in [-1.0, 2.0, -3.0, 4.0] {
            push_f64(&mut support, value);
        }
        push_int(&mut support, 0x04, 9, int_width);
        support.push(0x10);

        let mut record = vec![0x0f];
        push_ident(&mut record, "int_int_cur");
        push_int(&mut record, 0x04, 22_507, int_width);
        push_int(&mut record, 0x15, 0, int_width);
        record.extend_from_slice(&curve_block(int_width));
        push_f64(&mut record, 1.0e-6);
        push_ident(&mut record, "spline");
        record.push(0x0b);
        record.push(0x0f);
        push_ident(&mut record, "ref");
        push_int(&mut record, 0x04, 0, int_width);
        record.push(0x10);
        record.extend_from_slice(&[0x0b; 4]);
        push_ident(&mut record, "null_surface");
        record.extend_from_slice(&pcurve_block(int_width));
        push_ident(&mut record, "nullbs");
        record.extend_from_slice(&[0x0b, 0x0b]);
        for _ in 0..4 {
            push_int(&mut record, 0x04, 0, int_width);
        }
        record.push(0x10);

        let mut active = support;
        active.extend_from_slice(&record);
        let decoded = crate::nurbs::proc_curve::procedural_curve_resolving_refs(
            &asm_decode_ctx,
            &lex_test_span(&record, int_width).expect("valid single-record byte fixture"),
            &test_table(&active, int_width).expect("valid single-record byte fixture"),
        )
        .transpose()
        .expect("decode the procedural curve")
        .unwrap_or_else(|| panic!("intersection at width {int_width}"));
        let crate::nurbs::proc_curve::ProceduralCurveConstruction::Intersection(context, _) =
            &decoded.construction
        else {
            panic!("typed intersection context")
        };
        let crate::nurbs::proc_curve::SupportSlot::Constructed {
            surface,
            construction,
        } = &context.surfaces[0]
        else {
            panic!("the referenced support names its construction")
        };
        assert!(matches!(
            surface,
            SurfaceGeometry::Solved(SolvedSurfaceGeometry::Nurbs(_))
        ));
        assert!(matches!(
            construction.definition(),
            crate::nurbs::proc_surface::DecodedProceduralSurfaceDefinition::Exact { .. }
        ));
        assert_eq!(
            construction.cache_fit_tolerance(),
            Some(0.001 * crate::nurbs::reader::LEN_TO_MM)
        );
        assert!(matches!(
            context.surfaces[1],
            crate::nurbs::proc_curve::SupportSlot::Absent
        ));
    }
}

/// A referenced support that names a construction this decoder does not read
/// keeps its cache and says so, instead of passing for a plain surface.
#[test]
fn intersection_support_reference_to_an_unread_construction_keeps_its_cache_as_undecoded() {
    let asm_decode_arena = cadmpeg_core::decode::DecodeArena::new();
    let (asm_decode_ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
        &[],
        &asm_decode_arena,
        &cadmpeg_core::decode::DecodePolicy::default(),
    )
    .expect("test decode context");
    for int_width in [RefWidth::Four, RefWidth::Eight] {
        let mut support = vec![0x0f];
        push_ident(&mut support, "swepttapersur");
        support.extend_from_slice(&surface_block(int_width));
        push_f64(&mut support, 0.001);
        for value in [-1.0, 2.0, -3.0, 4.0] {
            push_f64(&mut support, value);
        }
        push_int(&mut support, 0x04, 9, int_width);
        support.push(0x10);

        let mut record = vec![0x0f];
        push_ident(&mut record, "int_int_cur");
        push_int(&mut record, 0x04, 22_507, int_width);
        push_int(&mut record, 0x15, 0, int_width);
        record.extend_from_slice(&curve_block(int_width));
        push_f64(&mut record, 1.0e-6);
        push_ident(&mut record, "spline");
        record.push(0x0b);
        record.push(0x0f);
        push_ident(&mut record, "ref");
        push_int(&mut record, 0x04, 0, int_width);
        record.push(0x10);
        record.extend_from_slice(&[0x0b; 4]);
        push_ident(&mut record, "null_surface");
        record.extend_from_slice(&pcurve_block(int_width));
        push_ident(&mut record, "nullbs");
        record.extend_from_slice(&[0x0b, 0x0b]);
        for _ in 0..4 {
            push_int(&mut record, 0x04, 0, int_width);
        }
        record.push(0x10);

        let mut active = support;
        active.extend_from_slice(&record);
        let decoded = crate::nurbs::proc_curve::procedural_curve_resolving_refs(
            &asm_decode_ctx,
            &lex_test_span(&record, int_width).expect("valid single-record byte fixture"),
            &test_table(&active, int_width).expect("valid single-record byte fixture"),
        )
        .transpose()
        .expect("decode the procedural curve")
        .unwrap_or_else(|| panic!("intersection at width {int_width}"));
        let crate::nurbs::proc_curve::ProceduralCurveConstruction::Intersection(context, _) =
            &decoded.construction
        else {
            panic!("typed intersection context")
        };
        let crate::nurbs::proc_curve::SupportSlot::Undecoded(surface) = &context.surfaces[0] else {
            panic!("the referenced support names a construction that is not read")
        };
        assert!(matches!(
            surface,
            SurfaceGeometry::Solved(SolvedSurfaceGeometry::Nurbs(_))
        ));
        assert!(matches!(
            context.surfaces[1],
            crate::nurbs::proc_curve::SupportSlot::Absent
        ));
    }
}
