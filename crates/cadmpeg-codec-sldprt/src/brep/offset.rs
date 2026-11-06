// SPDX-License-Identifier: Apache-2.0
//! Compact Parasolid offset-surface carriers.

use std::collections::HashMap;

use cadmpeg_core::decode::{u64_from_index, View};
use cadmpeg_ir::scalar::FiniteReal;

use super::LEN_TO_MM;

use crate::layout::offset_surface_00_3c as offset_surf;

const TAG: [u8; 2] = [0x00, 0x3c];
const COMMON_REFERENCE_COUNT: usize = 5;

/// One exact offset-surface construction, keyed by its stream-local attribute.
#[derive(Debug, Clone, Copy)]
pub(super) struct OffsetCarrier {
    /// Attribute of the support-surface carrier.
    pub(super) support: u16,
    /// Signed offset distance in millimetres.
    pub(super) distance: FiniteReal,
    /// Byte offset of the `00 3c` tag.
    pub(super) offset: usize,
}

fn parse_payload(
    body: &[u8],
    tail: usize,
    tripled_support: bool,
    wide: bool,
    offset: usize,
) -> Option<OffsetCarrier> {
    let discriminator = *body.get(tail)?;
    matches!(discriminator, b'V' | b'I' | b'U').then_some(())?;
    match body.get(tail + (offset_surf::TRUE_OFFSET - offset_surf::DISCRIMINATOR))? {
        0 | 1 => {}
        _ => return None,
    }
    let support_at = tail.checked_add(offset_surf::SUPPORT - offset_surf::DISCRIMINATOR)?;
    let (support, distance_at) = if wide {
        // The partition form with wide pointers: a support above `0x7ffe` takes two cells.
        let mut at = support_at;
        let support = u16::try_from(super::typed::read_ref(body, &mut at)?).ok()?;
        (support, at)
    } else {
        let support = View::u16_be_at(body, support_at)?;
        if tripled_support && body.get(support_at + 2) != Some(&1) {
            return None;
        }
        let distance_at = if tripled_support {
            support_at + 3
        } else {
            tail + (offset_surf::DISTANCE - offset_surf::DISCRIMINATOR)
        };
        (support, distance_at)
    };
    (support > 1).then_some(())?;
    let distance = View::f64_be_at(body, distance_at)? * LEN_TO_MM;
    Some(OffsetCarrier {
        support,
        distance: FiniteReal::new(distance)?,
        offset,
    })
}

fn parse_at(body: &[u8], offset: usize, wide: bool) -> Option<(u16, OffsetCarrier)> {
    (body.get(offset..offset + TAG.len())? == TAG).then_some(())?;
    let mut header = offset.checked_add(TAG.len())?;
    if body.get(header) == Some(&0xff) {
        header += 1;
    }
    let attr = View::u16_be_at(body, header + offset_surf::ATTR)?;

    let references = header.checked_add(offset_surf::REFS)?;
    let partition_marker = header.checked_add(offset_surf::MARKER)?;
    let partition_framed = attr > 1 && matches!(body.get(partition_marker), Some(0x2b | 0x2d));
    let partition = partition_framed
        .then(|| parse_payload(body, partition_marker + 1, false, false, offset))
        .flatten();

    let tripled_marker = references.checked_add(COMMON_REFERENCE_COUNT * 3)?;
    let tripled_references =
        (0..COMMON_REFERENCE_COUNT).all(|index| body.get(references + index * 3 + 2) == Some(&1));
    let tripled =
        (attr > 1 && tripled_references && matches!(body.get(tripled_marker), Some(0x2b | 0x2d)))
            .then(|| parse_payload(body, tripled_marker + 1, true, false, offset))
            .flatten();

    match (partition, tripled) {
        (Some(carrier), None) | (None, Some(carrier)) => return Some((attr, carrier)),
        (Some(_), Some(_)) => return None,
        (None, None) if !wide => return None,
        (None, None) => {}
    }

    // The partition form with wide pointers: a reference above `0x7ffe` takes two cells, and
    // each wide reference before the marker moves it two bytes. These readings apply only when
    // the fixed offsets fail, and only when exactly one of them passes.
    let wide_support = partition_framed
        .then(|| parse_payload(body, partition_marker + 1, false, true, offset))
        .flatten()
        .map(|carrier| (attr, carrier));
    let shifted = super::wide_partition_marker(body, header)
        .filter(|found| found.attr > 1 && matches!(body.get(found.marker_at), Some(0x2b | 0x2d)))
        .and_then(|found| {
            parse_payload(body, found.marker_at + 1, false, true, offset)
                .map(|carrier| (found.attr, carrier))
        });
    match (wide_support, shifted) {
        (Some(reading), None) | (None, Some(reading)) => Some(reading),
        (None, None) | (Some(_), Some(_)) => None,
    }
}

/// Scan all structurally valid type-60 offset-surface records. With `wide`, also read the
/// partition form whose references take two cells above `0x7ffe`.
pub(super) fn scan(
    ctx: &cadmpeg_core::decode::DecodeContext<'_>,
    body: &[u8],
    wide: bool,
) -> Result<HashMap<u16, OffsetCarrier>, cadmpeg_core::CodecError> {
    ctx.charge_work(u64_from_index(body.len()), "scan SLDPRT offset carriers")?;
    let mut out = HashMap::new();
    let Some(last_offset) = body.len().checked_sub(1) else {
        return Ok(out);
    };
    for offset in 0..last_offset {
        if let Some((attr, carrier)) = parse_at(body, offset, wide) {
            ctx.admit_hash_map_entry(&mut out, &attr, "index SLDPRT offset carriers")?;
            out.insert(attr, carrier);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{OffsetCarrier, TAG};
    use std::collections::HashMap;

    fn scan_with_service_context(bytes: &[u8]) -> HashMap<u16, OffsetCarrier> {
        scan_framed(bytes, false)
    }

    fn scan_framed(bytes: &[u8], wide: bool) -> HashMap<u16, OffsetCarrier> {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            bytes,
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("test carrier bytes fit service policy");
        super::scan(&ctx, bytes, wide).expect("test carriers fit service policy")
    }

    fn partition(discriminator: u8, flag: u8, support: u16, distance: f64) -> Vec<u8> {
        let mut bytes = TAG.to_vec();
        bytes.extend_from_slice(&12u16.to_be_bytes());
        bytes.extend_from_slice(&907u32.to_be_bytes());
        bytes.extend_from_slice(&[0; 10]);
        bytes.push(b'+');
        bytes.push(discriminator);
        bytes.push(flag);
        bytes.extend_from_slice(&support.to_be_bytes());
        bytes.extend_from_slice(&distance.to_be_bytes());
        bytes
    }

    fn deltas(distance: f64) -> Vec<u8> {
        let mut bytes = TAG.to_vec();
        bytes.push(0xff);
        bytes.extend_from_slice(&12u16.to_be_bytes());
        bytes.extend_from_slice(&907u32.to_be_bytes());
        for reference in [1u16, 1, 1, 1, 1] {
            bytes.extend_from_slice(&reference.to_be_bytes());
            bytes.push(1);
        }
        bytes.push(b'-');
        bytes.push(b'I');
        bytes.push(0);
        bytes.extend_from_slice(&6u16.to_be_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&distance.to_be_bytes());
        bytes
    }

    #[test]
    fn parses_partition_and_deltas_framing() {
        let partition = scan_with_service_context(&partition(b'V', 1, 6, -0.0025));
        let carrier = partition.get(&12).expect("partition offset surface");
        assert_eq!(carrier.support, 6);
        assert!((carrier.distance.get() + 2.5).abs() < 1.0e-12);

        let deltas = scan_with_service_context(&deltas(0.0045));
        let carrier = deltas.get(&12).expect("deltas offset surface");
        assert_eq!(carrier.support, 6);
        assert!((carrier.distance.get() - 4.5).abs() < 1.0e-12);
    }

    #[test]
    fn a_partition_offset_with_wide_pointers_is_read_only_with_the_wide_framing() {
        let mut bytes = TAG.to_vec();
        bytes.extend_from_slice(&12u16.to_be_bytes());
        bytes.extend_from_slice(&907u32.to_be_bytes());
        // One header reference and the support are above `0x7ffe`: two cells each.
        bytes.extend_from_slice(&[0x80, 0x05, 0x00, 0x01]);
        bytes.extend_from_slice(&[0; 8]);
        bytes.push(b'+');
        bytes.push(b'V');
        bytes.push(1);
        bytes.extend_from_slice(&[0xa0, 0x00, 0x00, 0x01]);
        bytes.extend_from_slice(&(-0.0025f64).to_be_bytes());

        assert!(scan_framed(&bytes, false).is_empty());
        let wide = scan_framed(&bytes, true);
        let carrier = wide.get(&12).expect("wide offset surface");
        assert_eq!(carrier.support, 0x2000 | 0x8000);
        assert!((carrier.distance.get() + 2.5).abs() < 1.0e-12);
    }

    #[test]
    fn rejects_invalid_fields_and_nonfinite_converted_distance() {
        assert!(scan_with_service_context(&partition(b'X', 1, 6, 1.0)).is_empty());
        assert!(scan_with_service_context(&partition(b'V', 2, 6, 1.0)).is_empty());
        assert!(scan_with_service_context(&partition(b'V', 1, 1, 1.0)).is_empty());
        assert!(scan_with_service_context(&partition(b'V', 1, 6, f64::INFINITY)).is_empty());
        assert!(scan_with_service_context(&partition(b'V', 1, 6, f64::MAX)).is_empty());

        let mut malformed_deltas = deltas(1.0);
        let support_terminator = malformed_deltas.len() - 9;
        malformed_deltas[support_terminator] = 0;
        assert!(scan_with_service_context(&malformed_deltas).is_empty());
    }
}
