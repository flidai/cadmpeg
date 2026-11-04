// SPDX-License-Identifier: Apache-2.0
//! Synthetic Parasolid transmit-stream builders for the crate's `#[cfg(test)]`
//! suites.
//!
//! The record builders follow the compact record grammar in
//! `docs/formats/sldprt.md` sections 3 and 4, which the shared Parasolid
//! decoder reads. They construct raw bytes only. Every body is built from
//! literals in code; no vendor file is read or embedded.
#![allow(clippy::unwrap_used)]

const MAGIC: [u8; 8] = [0xc2, 0xbc, 0x92, 0x8f, 0x99, 0x6e, 0x00, 0x00];

pub(crate) fn parasolid_with_body(description: &str, schema: &str, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[b'P', b'S', 0x00, 0x00]);
    b.extend_from_slice(
        &(u16::try_from(description.len()).expect("length fits u16")).to_be_bytes(),
    );
    b.extend_from_slice(description.as_bytes());
    b.extend_from_slice(&[0x00, 0x00]); // padding
    b.push(u8::try_from(schema.len()).expect("length fits u8"));
    b.extend_from_slice(schema.as_bytes());
    b.extend_from_slice(body);
    b
}

pub(crate) fn be16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_be_bytes());
}

pub(crate) fn be32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_be_bytes());
}

pub(crate) fn bef64(b: &mut Vec<u8>, v: f64) {
    b.extend_from_slice(&v.to_be_bytes());
}

/// A compact analytic plane carrier (tag `00 32`, 9 f64): origin, normal, refdir.
pub(crate) fn plane_carrier(
    attr: u16,
    origin: [f64; 3],
    normal: [f64; 3],
    refdir: [f64; 3],
) -> Vec<u8> {
    let mut b = vec![0x00, 0x32];
    be16(&mut b, attr);
    be32(&mut b, 0); // ordinal
    for _ in 0..5 {
        be16(&mut b, 0); // refs[5]
    }
    b.push(0x2b); // marker
    for v in origin.into_iter().chain(normal).chain(refdir) {
        bef64(&mut b, v);
    }
    b
}

/// A compact analytic line carrier (tag `00 1e`, 6 f64): point, direction.
pub(crate) fn line_carrier(attr: u16, point: [f64; 3], dir: [f64; 3]) -> Vec<u8> {
    let mut b = vec![0x00, 0x1e];
    be16(&mut b, attr);
    be32(&mut b, 0);
    for _ in 0..5 {
        be16(&mut b, 0);
    }
    b.push(0x2b);
    for v in point.into_iter().chain(dir) {
        bef64(&mut b, v);
    }
    b
}

/// Bridge `00 0e`: `refs[2]` = loop head, `refs[4]` = surface carrier.
pub(crate) fn bridge(attr: u16, loop_attr: u16, surface_attr: u16) -> Vec<u8> {
    let mut b = vec![0x00, 0x0e];
    be16(&mut b, attr); // p+0
    be32(&mut b, 0); // p+2 seq
    be16(&mut b, 0); // p+6 ref0
    b.extend_from_slice(&MAGIC); // p+8..16
    let refs = [0u16, 0, loop_attr, 0, surface_attr];
    for r in refs {
        be16(&mut b, r); // p+16..26
    }
    b.push(0x2b); // p+26 marker
    b.extend_from_slice(&[0u8; 10]); // p+27..37 tail
    b
}

/// Loop head `00 0f`: `refs[1]` = first coedge, `refs[2]` = owning bridge.
pub(crate) fn loop_head(attr: u16, first_coedge: u16, bridge_attr: u16) -> Vec<u8> {
    let mut b = vec![0x00, 0x0f];
    be16(&mut b, attr); // p+0
    be32(&mut b, 0); // p+2 seq
    let refs = [0u16, first_coedge, bridge_attr, 0];
    for r in refs {
        be16(&mut b, r); // p+6..14
    }
    b
}

/// Coedge `00 11`: `refs[1]` owner loop, `refs[3]` next, `refs[4]` start
/// vertex-use, `refs[5]` twin, `refs[6]` edge-use; marker is the local sense.
#[allow(clippy::too_many_arguments)]
pub(crate) fn coedge(
    attr: u16,
    owner_loop: u16,
    next: u16,
    start_vuse: u16,
    twin: u16,
    edge_use: u16,
    reversed: bool,
) -> Vec<u8> {
    let mut b = vec![0x00, 0x11];
    be16(&mut b, attr); // p+0
    let refs = [0u16, owner_loop, 0, next, start_vuse, twin, edge_use, 0, 0];
    for r in refs {
        be16(&mut b, r); // p+2..20
    }
    b.push(if reversed { 0x2d } else { 0x2b }); // p+20 marker
    b
}

/// Edge-use `00 10`: `refs[3]` = support curve carrier (0 = none).
pub(crate) fn edge_use(attr: u16, curve_attr: u16) -> Vec<u8> {
    edge_use_with_canonical(attr, 0, curve_attr)
}

/// Bare edge-use `refs[0]` names the forward coedge that stores the edge
/// direction. A zero canonical reference is reserved for compact fixtures
/// whose unique forward coedge supplies the same relation.
fn edge_use_with_canonical(attr: u16, canonical_coedge: u16, curve_attr: u16) -> Vec<u8> {
    let mut b = vec![0x00, 0x10];
    be16(&mut b, attr); // p+0
    be32(&mut b, 0); // p+2 seq
    be16(&mut b, 0); // p+6 ref0
    b.extend_from_slice(&MAGIC); // p+8..16
    let refs = [canonical_coedge, 0, 0, curve_attr, 0, 0];
    for r in refs {
        be16(&mut b, r); // p+16..28
    }
    b
}

/// Vertex-use `00 12`: `refs[4]` = world-point attr; magic at body+16.
pub(crate) fn vertex_use(attr: u16, point_attr: u16) -> Vec<u8> {
    let mut b = vec![0x00, 0x12];
    be16(&mut b, attr); // p+0
    be32(&mut b, 0); // p+2 seq
    let refs = [0u16, 0, 0, 0, point_attr];
    for r in refs {
        be16(&mut b, r); // p+6..16
    }
    b.extend_from_slice(&MAGIC); // p+16..24
    b
}

/// World point `00 1d`: xyz f64 BE (metres) at body+14.
pub(crate) fn world_point(attr: u16, xyz: [f64; 3]) -> Vec<u8> {
    let mut b = vec![0x00, 0x1d];
    be16(&mut b, attr); // p+0
    be32(&mut b, 0); // p+2 seq
    for _ in 0..4 {
        be16(&mut b, 0); // p+6..14 refs[4]
    }
    for v in xyz {
        bef64(&mut b, v); // p+14..38
    }
    b
}

fn typed_ref(bytes: &mut Vec<u8>, value: u32) {
    assert!(value <= 0x7ffe, "synthetic typed reference must fit u16");
    be16(bytes, u16::try_from(value).expect("value fits u16"));
}

fn typed_prefix(bytes: &mut Vec<u8>, tag: [u8; 2], attr: u16, node_id: u32) {
    bytes.extend_from_slice(&tag);
    bytes.push(0xff);
    be16(bytes, attr);
    be32(bytes, node_id);
}

/// Append one validated XT ownership graph for a compact face bridge.
pub(crate) fn typed_single_face_ownership(
    bytes: &mut Vec<u8>,
    body_attr: u16,
    shell_attr: u16,
    region_attr: u16,
    face_attr: u16,
    surface_attr: u16,
    body_type: u8,
) {
    typed_prefix(
        bytes,
        [0x00, 0x0c],
        body_attr,
        100_000 + u32::from(body_attr),
    );
    for value in [5, 6, 1, 1, 1, 1] {
        typed_ref(bytes, value);
    }
    bef64(bytes, 1000.0);
    bef64(bytes, 1.0e-8);
    for value in [1, 1, 1] {
        typed_ref(bytes, value);
    }
    bytes.push(1);
    typed_ref(bytes, 2);
    bytes.push(body_type);
    bytes.push(1);
    for value in [u32::from(shell_attr), 1, 1, 1, 1, 1, 1] {
        typed_ref(bytes, value);
    }
    typed_ref(bytes, u32::from(region_attr));
    typed_ref(bytes, 1);
    typed_ref(bytes, 1);
    typed_ref(bytes, 1);

    typed_prefix(
        bytes,
        [0x00, 0x0d],
        shell_attr,
        200_000 + u32::from(shell_attr),
    );
    for value in [
        1,
        u32::from(body_attr),
        1,
        1,
        1,
        1,
        u32::from(region_attr),
        1,
    ] {
        typed_ref(bytes, value);
    }

    typed_prefix(
        bytes,
        [0x00, 0x13],
        region_attr,
        300_000 + u32::from(region_attr),
    );
    for value in [1, u32::from(body_attr), 1, 1, u32::from(shell_attr)] {
        typed_ref(bytes, value);
    }
    bytes.push(b'S');

    typed_prefix(
        bytes,
        [0x00, 0x0e],
        face_attr,
        400_000 + u32::from(face_attr),
    );
    typed_ref(bytes, 1);
    bytes.extend_from_slice(&MAGIC);
    for value in [1, 1, 0, u32::from(shell_attr), u32::from(surface_attr)] {
        typed_ref(bytes, value);
    }
    bytes.push(0x2b);
}
