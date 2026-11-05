// SPDX-License-Identifier: Apache-2.0

use super::{is_identity, NearOccurrenceKey, OccurrenceKey};
use crate::test_support::test_archive::{archive_entries, assert_valid_document};
use crate::FcstdCodec;
use cadmpeg_ir::transform::Transform;
use cadmpeg_ir::{Codec, DecodeOptions};
use std::io::Cursor;

fn translation(x: f64, y: f64, z: f64) -> Transform {
    Transform::affine([[1.0, 0.0, 0.0, x], [0.0, 1.0, 0.0, y], [0.0, 0.0, 1.0, z]])
        .expect("finite translation")
}

#[test]
fn near_occurrence_keys_merge_rounding_inside_one_region_only() {
    let positive = translation(0.5e-12, 0.0, 0.0);
    let negative = translation(-0.5e-12, 0.0, 0.0);
    assert_eq!(
        NearOccurrenceKey::new(1, 7, positive),
        NearOccurrenceKey::new(1, 7, negative)
    );
    assert_eq!(
        NearOccurrenceKey::new(1, 7, positive),
        NearOccurrenceKey::new(1, 7, Transform::identity())
    );
    assert_ne!(
        NearOccurrenceKey::new(1, 7, positive),
        NearOccurrenceKey::new(2, 7, positive)
    );
    assert_ne!(
        NearOccurrenceKey::new(1, 7, positive),
        NearOccurrenceKey::new(1, 8, positive)
    );
    assert_ne!(
        NearOccurrenceKey::new(1, 14, translation(258.75, 0.0, 0.0)),
        NearOccurrenceKey::new(1, 14, translation(258.75, 0.0, 1.0e-8))
    );
}

#[test]
fn opposite_rotation_datums_share_the_near_key_of_the_identity() {
    // A pattern member rotated by 120 degrees and composed with the datum of
    // the opposite rotation is the identity up to rounding.
    let angle = 2.0 * std::f64::consts::PI / 3.0;
    let rotation = |angle: f64| {
        Transform::affine([
            [angle.cos(), -angle.sin(), 0.0, 0.0],
            [angle.sin(), angle.cos(), 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ])
        .expect("finite rotation")
    };
    let product = rotation(angle)
        .compose(rotation(-angle))
        .expect("finite product");
    assert!(!is_identity(product));
    assert_ne!(
        OccurrenceKey::new(36, product),
        OccurrenceKey::new(36, Transform::identity())
    );
    assert_eq!(
        NearOccurrenceKey::new(3, 36, product),
        NearOccurrenceKey::new(3, 36, Transform::identity())
    );
}

#[test]
fn faces_placed_by_rounding_equal_locations_share_edges_and_one_shell() {
    let document = r#"<Document SchemaVersion="4" FileVersion="1">
<Objects Count="1"><Object type="Part::Feature" name="Shape" id="1"/></Objects>
<ObjectData Count="1"><Object name="Shape"><Properties Count="1"><Property name="Shape" type="Part::PropertyPartShape"><Part file="Shape.brp"/></Property></Properties></Object></ObjectData>
</Document>"#;
    let brep = b"CASCADE Topology V1, (c) Matra-Datavision
Locations 2
1 1 0 0 0.0000000000005 0 1 0 0 0 0 1 0
1 1 0 0 0.0000000000007 0 1 0 0 0 0 1 0
Curve2ds 2
1 0 0 1 0
1 1 0 -1 0
Curves 2
1 0 0 0 1 0 0
1 1 0 0 -1 0 0
Polygon3D 0
PolygonOnTriangulations 0
Surfaces 1
1 0 0 0 0 0 1 1 0 0 0 1 0
Triangulations 0
TShapes 9
Ve 0.001 0 0 0 0 0 1001000 *
Ve 0.001 1 0 0 0 0 1001000 *
Ed 0.001 1 1 0 1 1 0 0 1 2 1 1 0 0 1 0 1001000 +9 0 -8 0 *
Ed 0.001 1 1 0 1 2 0 0 1 2 2 1 0 0 1 0 1001000 +8 0 -9 0 *
Wi 1001000 +7 0 +6 0 *
Fa 0 0.001 1 0 1001000 +5 0 *
Sh 1001000 +4 1 +4 2 *
So 1001000 +3 0 *
Co 1001000 +2 0 *
+1 0 *";
    let bytes = archive_entries(&[("Document.xml", document.as_bytes()), ("Shape.brp", brep)]);
    let result = FcstdCodec
        .decode(&mut Cursor::new(bytes), &DecodeOptions::default())
        .expect("rounding-equal face placements");
    let model = &result.ir().model;
    assert_eq!(model.faces.len(), 2);
    assert_eq!(model.edges.len(), 2);
    assert_eq!(model.vertices.len(), 2);
    assert_eq!(model.shells.len(), 1);
    assert_valid_document(result.ir());
}
