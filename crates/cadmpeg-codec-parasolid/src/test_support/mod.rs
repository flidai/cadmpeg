// SPDX-License-Identifier: Apache-2.0
//! Synthetic Parasolid transmit files for the crate's `#[cfg(test)]` suites.
//!
//! Every file is built from literals in code. No Parasolid file is read or
//! embedded.
#![allow(clippy::unwrap_used)]

pub(crate) mod parasolid;

use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy};

use parasolid::{
    bridge, coedge, edge_use, line_carrier, loop_head, parasolid_with_body, plane_carrier,
    typed_single_face_ownership, vertex_use, with_text_header, world_point,
};

/// Schema token of the synthetic files.
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

/// The binary transmit data around `body`.
pub(crate) fn binary_transmit(body: &[u8]) -> Vec<u8> {
    parasolid_with_body("TRANSMIT FILE created by test", SCHEMA, body)
}

/// A transmit file around `body`, behind the standard text header.
pub(crate) fn transmit_file(body: &[u8]) -> Vec<u8> {
    with_text_header(SCHEMA, &binary_transmit(body))
}

/// A transmit file around `body` of one schema, behind the standard text header.
pub(crate) fn transmit_file_of(schema: &str, body: &[u8]) -> Vec<u8> {
    with_text_header(
        schema,
        &parasolid_with_body("TRANSMIT FILE created by test", schema, body),
    )
}

pub(crate) fn with_decode_context<T>(root: &[u8], f: impl FnOnce(&DecodeContext<'_>) -> T) -> T {
    let arena = DecodeArena::new();
    let (ctx, _) = DecodeContext::from_root_bytes(root, &arena, &DecodePolicy::service())
        .expect("test root is admitted");
    f(&ctx)
}
