// SPDX-License-Identifier: Apache-2.0
//! Measurement decode retains model geometry and omits report-only data.

use super::{decode_with_purpose, AsmBrep, DecodePurpose, Record, Token, FORMAT};
use cadmpeg_core::decode::DecodeContext;

pub(super) fn assert_wire_geometry_matches(
    ctx: &DecodeContext<'_>,
    mut records: Vec<Record>,
    bytes: &[u8],
    model: AsmBrep,
) {
    const ENDPOINT_OVERSHOOT: f64 = 1.0e-10;
    let measured = decode_with_purpose(
        ctx,
        &records,
        bytes,
        "BREP.saved-edge.smbh",
        FORMAT,
        DecodePurpose::Measure,
    )
    .expect("valid BREP tolerances");
    assert_measure_geometry_matches(model, measured);

    // Free-form carriers retain the Model path and its endpoint clamp.
    let mut edge_tokens = records[1].tokens.to_vec();
    edge_tokens[4] = Token::Double(-ENDPOINT_OVERSHOOT);
    edge_tokens[6] = Token::Double(1.0 + ENDPOINT_OVERSHOOT);
    records[1].tokens = edge_tokens.into();
    records[6] = Record {
        index: 6,
        name: "intcurve-curve".into(),
        tokens: vec![
            Token::Ident("nubs".into()),
            Token::Long(1),
            Token::Enum(0),
            Token::Long(2),
            Token::Double(0.0),
            Token::Long(1),
            Token::Double(1.0),
            Token::Long(1),
            Token::Double(0.0),
            Token::Double(0.0),
            Token::Double(0.0),
            Token::Double(1.0),
            Token::Double(0.0),
            Token::Double(0.0),
        ]
        .into(),
        offset: 0,
        len: 0,
    };
    let decode = |purpose| {
        decode_with_purpose(ctx, &records, bytes, "wire.smbh", FORMAT, purpose)
            .expect("valid BREP tolerances")
    };
    let model = decode(DecodePurpose::Model);
    assert_eq!(model.stats.nurbs_curves, 1);
    assert_eq!(
        model.edges[0]
            .param_range()
            .map(cadmpeg_ir::units::FiniteVector::get),
        Some([0.0, 1.0])
    );
    assert_measure_geometry_matches(model, decode(DecodePurpose::Measure));
}

fn assert_measure_geometry_matches(mut model: AsmBrep, measured: AsmBrep) {
    assert!(measured.attributes.is_empty());
    assert!(measured.unknowns.is_empty());
    assert!(measured.annotation_records.is_empty());
    assert_eq!(measured.stats.other_records(), 0);
    assert!(measured.stats.other_record_kinds.is_empty());
    assert!(!model.annotation_records.is_empty());
    model.attributes.clear();
    model.unknowns.clear();
    model.stats.other_record_kinds.clear();
    // Compare every serialized graph field, including native geometry data.
    assert_eq!(
        serde_value::to_value(model).unwrap(),
        serde_value::to_value(measured).unwrap(),
    );
}

#[test]
fn measure_preserves_sphere_geometry_without_native_attributes() {
    let bytes = b"23200 0 2 2 \n\
16 Autodesk Neutron 21 ASM 232.4.0.65535 OSX 9 Synthetic \n\
1 9.999999999999999547e-07 1.000000000000000036e-10 \n\
asmheader $-1 -1 @13 232.4.0.65535 #\n\
body $-1 -1 $-1 $2 $-1 $-1 #\n\
lump $-1 -1 $-1 $-1 $3 $1 #\n\
shell $-1 -1 $-1 $-1 $-1 $4 $-1 $2 #\n\
face $-1 -1 $-1 $-1 $-1 $3 $-1 $5 forward single #\n\
sphere-surface $-1 -1 $-1 0 0 0 25 1 0 0 0 0 1 forward_v I I I I #\n\
End-of-ASM-data\n";
    let ctx = cadmpeg_test_support::service_decode_context();
    let mut stream = crate::sat::parse(&ctx, bytes).unwrap();
    stream.records.push(Record {
        index: 6,
        name: "ATTRIB_CUSTOM-attrib".into(),
        tokens: vec![
            Token::Ref(-1),
            Token::Long(-1),
            Token::Ref(-1),
            Token::Ref(-1),
            Token::Ref(4),
        ]
        .into(),
        offset: 0,
        len: 0,
    });
    let header = stream.header.as_kernel_header();
    let decode = |purpose| {
        crate::brep::decode_with_header(
            &ctx,
            &stream.records,
            bytes,
            Some(&header),
            "sphere.sat",
            FORMAT,
            purpose,
        )
        .expect("valid BREP tolerances")
    };
    let model = decode(DecodePurpose::Model);
    assert_eq!(model.faces.len(), 1);
    assert_eq!(model.attributes.len(), 1);
    assert_measure_geometry_matches(model, decode(DecodePurpose::Measure));
}

fn surface_grid(side: u32, origin: f64) -> Vec<Token> {
    let mut tokens = vec![
        Token::Ident("nubs".into()),
        Token::Long(1),
        Token::Long(1),
        Token::Enum(0),
        Token::Enum(0),
        Token::Enum(0),
        Token::Enum(0),
        Token::Long(i64::from(side)),
        Token::Long(i64::from(side)),
    ];
    for _ in 0..2 {
        for index in 0..side {
            tokens.extend([Token::Double(f64::from(index)), Token::Long(1)]);
        }
    }
    for v in 0..side {
        for u in 0..side {
            tokens.extend([
                Token::Double(origin + f64::from(u)),
                Token::Double(f64::from(v)),
                Token::Double(0.0),
            ]);
        }
    }
    tokens
}

fn optional_support_records(support_side: u32) -> Vec<Record> {
    let record = |index, name: &str, tokens: Vec<Token>| Record {
        index,
        name: name.into(),
        tokens: tokens.into(),
        offset: 0,
        len: 0,
    };
    let mut construction = vec![
        Token::SubtypeOpen,
        Token::Ident("rb_blend_spl_sur".into()),
        Token::Long(22_300),
        Token::Str("blend_support_surface".into()),
        Token::Ident("spline".into()),
        Token::False,
        Token::SubtypeOpen,
        Token::Ident("exact_spl_sur".into()),
    ];
    construction.extend(surface_grid(support_side, 0.0));
    construction.push(Token::SubtypeClose);
    construction.extend(std::iter::repeat_n(Token::False, 4));
    construction.extend([
        Token::Ident("null_curve".into()),
        Token::Ident("nullbs".into()),
        Token::Position([0.0; 3]),
        Token::Ident("nullbs".into()),
        Token::Str("blend_support_surface".into()),
        Token::Ident("plane".into()),
        Token::Position([0.0; 3]),
        Token::Vector3([0.0, 0.0, 1.0]),
        Token::Vector3([1.0, 0.0, 0.0]),
        Token::False,
    ]);
    construction.extend(std::iter::repeat_n(Token::False, 4));
    construction.extend([
        Token::Ident("null_curve".into()),
        Token::Ident("nullbs".into()),
        Token::Position([0.0; 3]),
        Token::Ident("nullbs".into()),
        Token::Ident("straight".into()),
        Token::Position([0.0; 3]),
        Token::Vector3([1.0, 0.0, 0.0]),
        Token::False,
        Token::False,
        Token::Double(1.0),
        Token::Double(1.0),
        Token::Enum(-1),
    ]);
    construction.extend(std::iter::repeat_n(Token::False, 4));
    construction.extend([
        Token::Long(0),
        Token::Double(0.0),
        Token::Double(1.0),
        Token::Long(0),
        Token::Enum(0),
    ]);
    construction.extend(surface_grid(2, 7.0));
    construction.push(Token::Double(0.001));
    construction.extend(std::iter::repeat_n(Token::Long(0), 6));
    construction.push(Token::False);
    construction.extend(std::iter::repeat_n(Token::Long(0), 3));
    construction.push(Token::SubtypeClose);
    vec![
        record(
            0,
            "body",
            vec![
                Token::Ref(-1),
                Token::Long(0),
                Token::Ref(-1),
                Token::Ref(1),
                Token::Ref(-1),
                Token::Ref(-1),
            ],
        ),
        record(
            1,
            "lump",
            vec![
                Token::Ref(-1),
                Token::Long(0),
                Token::Ref(-1),
                Token::Ref(-1),
                Token::Ref(2),
                Token::Ref(0),
            ],
        ),
        record(
            2,
            "shell",
            vec![
                Token::Ref(-1),
                Token::Long(0),
                Token::Ref(-1),
                Token::Ref(-1),
                Token::Ref(-1),
                Token::Ref(3),
                Token::Ref(-1),
                Token::Ref(1),
            ],
        ),
        record(
            3,
            "face",
            vec![
                Token::Ref(-1),
                Token::Long(0),
                Token::Ref(-1),
                Token::Ref(-1),
                Token::Ref(-1),
                Token::Ref(2),
                Token::Ref(-1),
                Token::Ref(4),
                Token::False,
                Token::False,
            ],
        ),
        record(
            4,
            "spline",
            vec![
                Token::Ref(-1),
                Token::Long(0),
                Token::Ref(-1),
                Token::False,
                Token::SubtypeOpen,
                Token::Ident("ref".into()),
                Token::Long(0),
                Token::SubtypeClose,
                Token::False,
                Token::False,
                Token::False,
                Token::False,
            ],
        ),
        record(5, "spline", construction),
    ]
}

#[test]
fn oversized_optional_support_leaves_owned_cache_in_every_purpose() {
    use cadmpeg_core::decode::{DecodeArena, DecodePolicy};
    use cadmpeg_ir::geometry::{
        ProceduralSurfaceDefinition, SolvedSurfaceGeometry, SurfaceGeometry,
    };
    use cadmpeg_ir::math::Point3;

    // A real, complete support grid exceeds the recovery ceiling; the
    // aliased blend still owns a separate four-pole current face cache.
    let records = optional_support_records(449);
    for purpose in [DecodePurpose::Measure, DecodePurpose::Model] {
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::service()).unwrap();
        let result = crate::brep::decode_with_header(
            &ctx,
            &records,
            &[],
            None,
            "measurement.sat",
            FORMAT,
            purpose,
        );
        let model = result.expect("an oversized optional support is not recovered, not refused");
        assert_eq!(model.bodies.len(), 1);
        assert_eq!(model.faces.len(), 1);
        assert_eq!(model.stats.nurbs_surfaces, 1);
        assert_eq!(model.surfaces.len(), 1);
        assert!(model.curves.is_empty());
        // The retained face cache has an untyped construction marker; the
        // oversized support and its exact construction are not emitted.
        assert_eq!(model.procedural_surfaces.len(), 1);
        assert_eq!(model.procedural_surfaces[0].0, model.surfaces[0].id);
        assert!(matches!(
            model.procedural_surfaces[0].1.definition(),
            ProceduralSurfaceDefinition::Unknown {
                record: Some(_),
                cache: None,
            }
        ));
        let SurfaceGeometry::Solved(SolvedSurfaceGeometry::Nurbs(surface)) =
            &model.surfaces[0].geometry
        else {
            panic!("the owner cache remains the measured face surface");
        };
        assert_eq!((surface.u_count(), surface.v_count()), (2, 2));
        assert_eq!(
            surface.pole(0, 0).unwrap().get(),
            Point3::new(70.0, 0.0, 0.0)
        );
        ctx.finish_session()
            .expect("the oversized support did not poison the session");
    }
}

#[test]
fn small_optional_constructions_decode_in_every_purpose() {
    use cadmpeg_core::decode::{DecodeArena, DecodePolicy};
    let records = optional_support_records(2);
    for purpose in [DecodePurpose::Model, DecodePurpose::Measure] {
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&[], &arena, &DecodePolicy::service()).unwrap();
        let model = crate::brep::decode_with_header(
            &ctx,
            &records,
            &[],
            None,
            "measurement.sat",
            FORMAT,
            purpose,
        )
        .unwrap();
        assert_eq!(model.faces.len(), 1);
        assert_eq!(model.stats.nurbs_surfaces, 1);
        assert_eq!(model.procedural_surfaces.len(), 1);
        ctx.finish_session().unwrap();
    }
}
