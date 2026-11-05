// SPDX-License-Identifier: Apache-2.0

use crate::test_support::test_archive::archive_entries;
use crate::FcstdCodec;
use cadmpeg_ir::topology::BodyKind;
use cadmpeg_ir::{Codec, DecodeOptions};
use std::io::Cursor;

const DOCUMENT: &str = r#"<Document SchemaVersion="4" FileVersion="1">
<Objects Count="1"><Object type="Part::Feature" name="Shape" id="1"/></Objects>
<ObjectData Count="1"><Object name="Shape"><Properties Count="1"><Property name="Shape" type="Part::PropertyPartShape"><Part file="Shape.brp"/></Property></Properties></Object></ObjectData>
</Document>"#;

/// One plane face in a shell and a solid, below a compound that lists `children`.
fn compound_brep(children: &str) -> String {
    format!(
        "CASCADE Topology V1, (c) Matra-Datavision
Locations 3
1 1 0 0 10 0 1 0 0 0 0 1 0
1 -2 0 0 0 0 2 0 5 0 0 2 0
1 1 0 0 20 0 1 0 0 0 0 1 0
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
Sh 1001000 +4 2 *
So 1001000 +3 0 *
Co 1001000 {children} *
+1 0 *"
    )
}

fn kind_of(children: &str) -> BodyKind {
    let brep = compound_brep(children);
    let bytes = archive_entries(&[
        ("Document.xml", DOCUMENT.as_bytes()),
        ("Shape.brp", brep.as_bytes()),
    ]);
    let result = FcstdCodec
        .decode(&mut Cursor::new(bytes), &DecodeOptions::default())
        .expect("compound body");
    assert_eq!(result.ir().model.bodies.len(), 1);
    result.ir().model.bodies[0].kind
}

#[test]
fn a_compound_of_solids_is_a_solid_body() {
    assert_eq!(kind_of("+2 1 +2 3"), BodyKind::Solid);
}

#[test]
fn a_compound_of_shells_is_a_sheet_body() {
    assert_eq!(kind_of("+3 1 +3 3"), BodyKind::Sheet);
}

#[test]
fn a_compound_of_solids_and_shells_is_a_general_body() {
    assert_eq!(kind_of("+2 1 +3 3"), BodyKind::General);
}
