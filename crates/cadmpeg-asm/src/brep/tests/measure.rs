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
    let header = stream.header.as_kernel_header(&ctx).unwrap();
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
