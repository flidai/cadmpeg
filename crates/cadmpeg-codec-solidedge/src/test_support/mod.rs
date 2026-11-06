// SPDX-License-Identifier: Apache-2.0
//! Synthetic Solid Edge documents for the crate's `#[cfg(test)]` suites.
//!
//! Every image is built from literals in code: a Compound File around a
//! Parasolid partition and deltas pair. No Solid Edge file is read or embedded.
#![allow(clippy::unwrap_used)]

pub(crate) mod cfb;
pub(crate) mod parasolid;

use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy};

use cfb::{compound_file, storage, stream, Node};
use parasolid::{
    bridge, coedge, edge_use, line_carrier, loop_head, parasolid_with_body, plane_carrier,
    typed_face, typed_single_face_ownership, vertex_use, with_text_header, world_point,
};

/// Schema token of the synthetic streams.
pub(crate) const SCHEMA: &str = "SCH_3201255_32001";

/// Metres per side of the synthetic triangle.
pub(crate) const SIDE_METRES: f64 = 0.1;

/// A planar triangle in the z = 0 plane with three line edges.
pub(crate) fn triangle_body() -> Vec<u8> {
    let side = SIDE_METRES;
    let diagonal = std::f64::consts::FRAC_1_SQRT_2;
    let mut b = Vec::new();
    b.extend(plane_carrier(
        100,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    ));
    b.extend(line_carrier(101, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
    b.extend(line_carrier(
        102,
        [side, 0.0, 0.0],
        [-diagonal, diagonal, 0.0],
    ));
    b.extend(line_carrier(103, [0.0, side, 0.0], [0.0, -1.0, 0.0]));
    b.extend(bridge(10, 20, 100));
    b.extend(loop_head(20, 30, 10));
    b.extend(coedge(30, 20, 31, 50, 0, 40, false));
    b.extend(coedge(31, 20, 32, 51, 0, 41, false));
    b.extend(coedge(32, 20, 30, 52, 0, 42, false));
    b.extend(edge_use(40, 101));
    b.extend(edge_use(41, 102));
    b.extend(edge_use(42, 103));
    b.extend(vertex_use(50, 60));
    b.extend(vertex_use(51, 61));
    b.extend(vertex_use(52, 62));
    b.extend(world_point(60, [0.0, 0.0, 0.0]));
    b.extend(world_point(61, [side, 0.0, 0.0]));
    b.extend(world_point(62, [0.0, side, 0.0]));
    typed_single_face_ownership(&mut b, 900, 901, 902, 10, 100, 1);
    b
}

/// Two planar triangles that share the edge from the origin along x: the first in the z = 0 plane,
/// the second in the y = 0 plane. The shared edge runs forward in the first loop and reversed in
/// the second, so the coedge senses place both faces on the same side. The face records carry the
/// senses given: `second_reversed` makes the second face the reversed one.
pub(crate) fn two_triangle_body(second_reversed: bool) -> Vec<u8> {
    let side = SIDE_METRES;
    let diagonal = std::f64::consts::FRAC_1_SQRT_2;
    let marker = if second_reversed { 0x2d } else { 0x2b };
    let mut b = Vec::new();
    b.extend(plane_carrier(
        100,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    ));
    b.extend(plane_carrier(
        104,
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ));
    b.extend(line_carrier(101, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
    b.extend(line_carrier(
        102,
        [side, 0.0, 0.0],
        [-diagonal, diagonal, 0.0],
    ));
    b.extend(line_carrier(103, [0.0, side, 0.0], [0.0, -1.0, 0.0]));
    b.extend(line_carrier(
        105,
        [side, 0.0, 0.0],
        [-diagonal, 0.0, diagonal],
    ));
    b.extend(line_carrier(106, [0.0, 0.0, side], [0.0, 0.0, -1.0]));
    // Face one.
    let mut face_one = bridge(10, 20, 100);
    b.append(&mut face_one);
    b.extend(loop_head(20, 30, 10));
    b.extend(coedge(30, 20, 31, 50, 0, 40, false));
    b.extend(coedge(31, 20, 32, 51, 0, 41, false));
    b.extend(coedge(32, 20, 30, 52, 0, 42, false));
    // Face two, whose first coedge runs the shared edge backwards.
    let mut face_two = bridge(11, 21, 104);
    let marker_at = face_two.len() - 11;
    face_two[marker_at] = marker;
    b.append(&mut face_two);
    b.extend(loop_head(21, 33, 11));
    b.extend(coedge(33, 21, 34, 53, 0, 40, true));
    b.extend(coedge(34, 21, 35, 54, 0, 43, false));
    b.extend(coedge(35, 21, 33, 55, 0, 44, false));
    b.extend(edge_use(40, 101));
    b.extend(edge_use(41, 102));
    b.extend(edge_use(42, 103));
    b.extend(edge_use(43, 105));
    b.extend(edge_use(44, 106));
    for (attr, point) in [(50, 60), (51, 61), (52, 62), (53, 61), (54, 63), (55, 60)] {
        b.extend(vertex_use(attr, point));
    }
    b.extend(world_point(60, [0.0, 0.0, 0.0]));
    b.extend(world_point(61, [side, 0.0, 0.0]));
    b.extend(world_point(62, [0.0, side, 0.0]));
    b.extend(world_point(63, [0.0, 0.0, side]));
    typed_single_face_ownership(&mut b, 900, 901, 902, 10, 100, 1);
    typed_face(&mut b, 11, 901, 104, marker);
    b
}

/// A deltas body that adds one point record.
pub(crate) fn deltas_body() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(world_point(63, [0.5, 0.5, 0.5]));
    b
}

/// A deltas body that moves the point record 60 to x = 20 mm.
pub(crate) fn moved_point_deltas_body() -> Vec<u8> {
    world_point(60, [0.02, 0.0, 0.0])
}

/// A partition transmit stream around `body`, behind the standard text header.
pub(crate) fn partition_stream(body: &[u8]) -> Vec<u8> {
    with_text_header(SCHEMA, &binary_partition(body))
}

/// A partition transmit stream of one schema, behind the standard text header.
pub(crate) fn partition_stream_of(schema: &str, body: &[u8]) -> Vec<u8> {
    with_text_header(
        schema,
        &parasolid_with_body("TRANSMIT FILE (partition) created by test", schema, body),
    )
}

/// A deltas transmit stream around `body`, behind the standard text header.
pub(crate) fn deltas_stream(body: &[u8]) -> Vec<u8> {
    with_text_header(SCHEMA, &binary_deltas(body))
}

/// A standalone transmit stream around `body`, behind the standard text header.
pub(crate) fn transmit_stream(body: &[u8]) -> Vec<u8> {
    with_text_header(
        SCHEMA,
        &parasolid_with_body("TRANSMIT FILE created by test", SCHEMA, body),
    )
}

/// The binary part of a partition stream.
pub(crate) fn binary_partition(body: &[u8]) -> Vec<u8> {
    parasolid_with_body("TRANSMIT FILE (partition) created by test", SCHEMA, body)
}

/// The binary part of a deltas stream.
pub(crate) fn binary_deltas(body: &[u8]) -> Vec<u8> {
    parasolid_with_body("TRANSMIT FILE (deltas) created by test", SCHEMA, body)
}

/// A document with the `STREAM0` pair and no other entry.
pub(crate) fn part_with_pair() -> Vec<u8> {
    document(vec![storage(
        "PARASOLID",
        vec![
            stream("STREAM0.P_B", &partition_stream(&triangle_body())),
            stream("STREAM0.D_B", &deltas_stream(&deltas_body())),
        ],
    )])
}

/// A document built from arbitrary root members.
pub(crate) fn document(children: Vec<(String, Node)>) -> Vec<u8> {
    compound_file(children)
}

pub(crate) fn with_decode_context<T>(root: &[u8], f: impl FnOnce(&DecodeContext<'_>) -> T) -> T {
    let arena = DecodeArena::new();
    let (ctx, _) = DecodeContext::from_root_bytes(root, &arena, &DecodePolicy::service())
        .expect("test root is admitted");
    f(&ctx)
}
