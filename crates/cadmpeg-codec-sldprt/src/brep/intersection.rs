// SPDX-License-Identifier: Apache-2.0
//! Surface-intersection curve carriers.
//!
//! A `00 26` composite record or `00 01 5a` intersection-data entity carries a
//! curve defined by the intersection of two support surfaces. Its payload
//! references a `00 28` chart record (the solved point cache), two `00 29`
//! terminator records (the exact curve endpoints), and a `00 cc` support-UV
//! record. Referenced terminators select the chart entry width and replace its
//! approximate endpoints. A complete width-4 UV record additionally yields
//! co-parameterized support pcurve caches.

use std::collections::HashMap;

use cadmpeg_core::bytes::find_iter;
use cadmpeg_core::decode::{u64_from_index, DecodeContext, View};
use cadmpeg_core::CodecError;
use cadmpeg_ir::geometry::{CurveGeometry, SolvedCurveGeometry};
use cadmpeg_ir::math::{Point2, Point3};
use cadmpeg_ir::report::loss::LossNote;

use super::{CurveCarrier, LEN_TO_MM};

use crate::layout::intersection_composite as isect;
use crate::layout::support_uv_00_cc as support_uv;

/// Chart parameter sentinel marking an absent value.
const MISSING_PARAMETER: f64 = -31_415_800_000_000.0;
/// Fixed bytes between an inline `term_use` label and its terminator body.
const INLINE_TERM_TAIL: &[u8] = b"\x00\x00\x00\x01\x01\x63\x43\x5a";
/// Fixed bytes between an inline `values` label and its support-UV body.
const INLINE_UV_TAIL: &[u8] = b"\x00\x00\x00\x02\x01\x66\x01";

/// One decoded chart: solved points in metres and parameter bookkeeping.
struct Chart {
    endpoints: [[f64; 3]; 2],
    interior_points: Vec<[f64; 3]>,
    base_parameter: f64,
    base_scale: f64,
    chordal_error: f64,
}

/// One validated intersection curve and its solved chart.
pub(super) struct IntersectionCarrier {
    pub(super) carrier: CurveCarrier,
    pub(super) support_data: IntersectionSupportData,
}

/// Ordered supports and optional UV lanes for the model-space chart curve.
#[derive(Clone)]
pub(super) struct IntersectionSupportData {
    pub(super) supports: [u16; 2],
    pub(super) fit_tolerance_mm: f64,
    pub(super) support_uv: Option<[Vec<Point2>; 2]>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UvWidth {
    Two,
    Four,
}

impl UvWidth {
    fn values_per_point(self) -> usize {
        match self {
            Self::Two => 2,
            Self::Four => 4,
        }
    }
}

struct UvRecord {
    width: UvWidth,
    values: Vec<f64>,
}

struct SolvedChart {
    geometry: CurveGeometry,
    parameters: Vec<f64>,
    fit_tolerance_mm: f64,
    reversed: bool,
    endpoint_displacement: f64,
}

/// Offsets of every `00 tt` tag, with the optional `0xff` escape skipped.
fn record_bodies(bytes: &[u8], tt: u8) -> impl Iterator<Item = usize> + '_ {
    (0..bytes.len()).filter_map(move |at| {
        if bytes.get(at) != Some(&0x00) || bytes.get(at + 1) != Some(&tt) {
            return None;
        }
        let body = at + 2;
        Some(if bytes.get(body) == Some(&0xff) {
            body + 1
        } else {
            body
        })
    })
}

/// One reading of an intersection record: the tag offset, the body offset, the attribute, the
/// payload-marker offset, and whether a payload reference above `0x7ffe` takes two cells.
type Composite = (usize, usize, u16, usize, bool);

/// The readings of the record whose body starts at `body`, in order: the fixed offsets, and with
/// `wide` the partition form whose references take two cells above `0x7ffe` (the payload alone,
/// then the header and the payload). The first reading that resolves keeps the attribute.
fn composite_readings(
    bytes: &[u8],
    offset: usize,
    body: usize,
    wide: bool,
) -> impl Iterator<Item = Composite> {
    let marker = body.checked_add(isect::MARKER);
    let strict = marker
        .filter(|marker| matches!(bytes.get(*marker), Some(0x2b | 0x2d)))
        .and_then(|marker| {
            View::u16_be_at(bytes, body + isect::ATTR).map(|attr| (offset, body, attr, marker))
        });
    let wide_payload = strict
        .filter(|&(_, _, _, marker)| wide && payload_has_wide_cell(bytes, marker))
        .map(|(offset, body, attr, marker)| (offset, body, attr, marker, true));
    let shifted = wide
        .then(|| super::wide_partition_marker(bytes, body))
        .flatten()
        .filter(|found| matches!(bytes.get(found.marker_at), Some(0x2b | 0x2d)))
        .map(|found| (offset, body, found.attr, found.marker_at, true));
    strict
        .map(|(offset, body, attr, marker)| (offset, body, attr, marker, false))
        .into_iter()
        .chain(wide_payload)
        .chain(shifted)
}

/// Whether a cell of the fixed six-reference payload after `marker` is above `0x7ffe`.
fn payload_has_wide_cell(bytes: &[u8], marker: usize) -> bool {
    (0..6).any(|index| {
        View::u16_be_at(bytes, marker + (isect::PAYLOAD - isect::MARKER) + index * 2)
            .is_some_and(|cell| cell > 0x7ffe)
    })
}

/// Every reading of both intersection forms.
fn composite_records(bytes: &[u8], wide: bool) -> impl Iterator<Item = Composite> + '_ {
    record_bodies(bytes, 0x26)
        .flat_map(move |body| composite_readings(bytes, body - 2, body, wide))
        .chain(
            (0..bytes.len().checked_sub(20).map_or(0, |end| end))
                .filter(|offset| bytes.get(*offset..*offset + 3) == Some(&[0x00, 0x01, 0x5a]))
                .flat_map(move |offset| composite_readings(bytes, offset, offset + 3, wide)),
        )
}

/// The six payload references after the marker, and the offset after them. With `wide`, a
/// reference above `0x7ffe` takes two cells and must fit sixteen bits.
fn composite_payload(bytes: &[u8], marker: usize, wide: bool) -> Option<([u16; 6], usize)> {
    let mut at = marker.checked_add(isect::PAYLOAD - isect::MARKER)?;
    let mut refs = [0_u16; 6];
    for reference in &mut refs {
        *reference = if wide {
            u16::try_from(super::typed::read_ref(bytes, &mut at)?).ok()?
        } else {
            let value = View::u16_be_at(bytes, at)?;
            at += 2;
            value
        };
    }
    // The fixed reading ends at the fixed record length.
    let end = if wide {
        at
    } else {
        marker.checked_add(isect::LEN - isect::MARKER)?
    };
    Some((refs, end))
}

/// The own attribute of a chart, terminator, or support-UV record at `at`, and how many bytes it
/// moves the fields after it. With `wide_attr`, the attribute is the two cells of the partition
/// form (the first cell above `0x7ffe`); otherwise one cell.
fn own_attr(bytes: &[u8], at: usize, wide_attr: bool) -> Option<(u16, usize)> {
    if !wide_attr {
        return View::u16_be_at(bytes, at).map(|attr| (attr, 0));
    }
    let mut end = at;
    let value = super::typed::read_ref(bytes, &mut end)?;
    (end - at == 4).then_some(())?;
    Some((u16::try_from(value).ok()?, 2))
}

fn finite_point(bytes: &[u8], at: usize) -> Option<[f64; 3]> {
    let point = [
        View::f64_be_at(bytes, at)?,
        View::f64_be_at(bytes, at + 8)?,
        View::f64_be_at(bytes, at + 16)?,
    ];
    point
        .iter()
        .all(|value| value.is_finite() && value.abs() < 1e6)
        .then_some(point)
}

fn finite_tangent(bytes: &[u8], at: usize) -> bool {
    let Some(tangent) = (|| {
        Some([
            View::f64_be_at(bytes, at)?,
            View::f64_be_at(bytes, at + 8)?,
            View::f64_be_at(bytes, at + 16)?,
        ])
    })() else {
        return false;
    };
    if tangent.iter().any(|value| !value.is_finite()) {
        return false;
    }
    tangent.iter().any(|value| *value != 0.0)
}

/// Parse every `00 28` chart record: `count:u32 attr:u16 base_parameter:f64
/// base_scale:f64 chart_count:u32 chordal_error:f64`, two [`MISSING_PARAMETER`]
/// sentinels at +36/+44, then `count` point entries at +52 (88-byte entries
/// carrying a finite nonzero tangent at +56, or bare 24-byte points).
fn chart_records(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    wide: bool,
) -> Result<HashMap<u16, Vec<Chart>>, CodecError> {
    let mut out: HashMap<u16, Vec<Chart>> = HashMap::new();
    for body in record_bodies(bytes, 0x28) {
        let mut found = chart_candidates(ctx, bytes, body, false)?;
        if found.is_none() && wide {
            found = chart_candidates(ctx, bytes, body, true)?;
        }
        let Some((attr, candidates)) = found else {
            continue;
        };
        ctx.admit_hash_map_entry(&mut out, &attr, "collect Parasolid intersection charts")?;
        ctx.extend_vec(
            out.entry(attr).or_default(),
            candidates,
            "collect Parasolid intersection chart candidates",
        )?;
    }
    Ok(out)
}

fn chart_candidates(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    body: usize,
    wide_attr: bool,
) -> Result<Option<(u16, Vec<Chart>)>, CodecError> {
    let Some(count) = View::u32_be_at(bytes, body).and_then(|count| usize::try_from(count).ok())
    else {
        return Ok(None);
    };
    let Some((attr, shift)) = own_attr(bytes, body + 4, wide_attr) else {
        return Ok(None);
    };
    let preamble = body + 6 + shift;
    let (Some(base_parameter), Some(base_scale), Some(chart_count), Some(chordal_error)) = (
        View::f64_be_at(bytes, preamble),
        View::f64_be_at(bytes, preamble + 8),
        View::u32_be_at(bytes, preamble + 16).and_then(|count| usize::try_from(count).ok()),
        View::f64_be_at(bytes, preamble + 20),
    ) else {
        return Ok(None);
    };
    if !(2..=4096).contains(&count)
        || chart_count != count
        || !base_parameter.is_finite()
        || !base_scale.is_finite()
        || base_scale == 0.0
        || !chordal_error.is_finite()
        || chordal_error <= 0.0
        || View::f64_be_at(bytes, preamble + 36) != Some(MISSING_PARAMETER)
        || View::f64_be_at(bytes, preamble + 44) != Some(MISSING_PARAMETER)
    {
        return Ok(None);
    }
    let block = preamble + 52;
    let mut candidates = Vec::new();
    for (stride, extended) in [(88usize, true), (24usize, false)] {
        let Some(end) = stride
            .checked_mul(count)
            .and_then(|size| block.checked_add(size))
        else {
            continue;
        };
        if end > bytes.len() {
            continue;
        }
        if extended && !(0..count).all(|index| finite_tangent(bytes, block + index * stride + 56)) {
            continue;
        }
        let (Some(first), Some(last)) = (
            finite_point(bytes, block),
            finite_point(bytes, block + (count - 1) * stride),
        ) else {
            continue;
        };
        if !(1..count - 1).all(|index| finite_point(bytes, block + index * stride).is_some()) {
            continue;
        }
        let mut interior_points =
            ctx.collection_vec(count - 2, "decode Parasolid chart interior points")?;
        for index in 1..count - 1 {
            if let Some(point) = finite_point(bytes, block + index * stride) {
                interior_points.push(point);
            }
        }
        if !extended && first == last && interior_points.iter().all(|point| *point == first) {
            continue;
        }
        ctx.push_vec(
            &mut candidates,
            Chart {
                endpoints: [first, last],
                interior_points,
                base_parameter,
                base_scale,
                chordal_error,
            },
            "collect Parasolid chart stride candidates",
        )?;
    }
    Ok((!candidates.is_empty()).then_some((attr, candidates)))
}

/// Parse a terminator body: `count:u32 attr:u16`, a kind label, then the
/// endpoint. The label is one kind character (`L` limit, `H` ring, `T`
/// terminator) with an optional second character (`?`, `F`, or `S`). Both
/// label widths yield a candidate endpoint; composite validation selects the
/// candidate that matches the chart.
fn term_at(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    body: usize,
    wide: bool,
    out: &mut HashMap<u16, Vec<[f64; 3]>>,
) -> Result<(), CodecError> {
    if !term_at_framed(ctx, bytes, body, false, out)? && wide {
        term_at_framed(ctx, bytes, body, true, out)?;
    }
    Ok(())
}

/// [`term_at`] with one reading of the attribute; whether an endpoint was read.
fn term_at_framed(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    body: usize,
    wide_attr: bool,
    out: &mut HashMap<u16, Vec<[f64; 3]>>,
) -> Result<bool, CodecError> {
    let (Some(count), Some((attr, shift))) = (
        View::u32_be_at(bytes, body),
        own_attr(bytes, body + 4, wide_attr),
    ) else {
        return Ok(false);
    };
    if !(1..=2).contains(&count) {
        return Ok(false);
    }
    let label = body + 6 + shift;
    if !matches!(bytes.get(label), Some(b'L' | b'H' | b'T')) {
        return Ok(false);
    }
    let two_char = matches!(bytes.get(label + 1), Some(b'?' | b'F' | b'S'));
    let mut read = false;
    for label_len in [2usize, 1] {
        if label_len == 2 && !two_char {
            continue;
        }
        if let Some(point) = finite_point(bytes, label + label_len) {
            read = true;
            ctx.push_hash_group(
                out,
                attr,
                point,
                "collect Parasolid terminator groups",
                "collect Parasolid terminator points",
            )?;
        }
    }
    Ok(read)
}

/// Every `00 29` or inline `term_use` terminator, keyed by attribute.
fn term_records(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    wide: bool,
) -> Result<HashMap<u16, Vec<[f64; 3]>>, CodecError> {
    let mut out: HashMap<u16, Vec<[f64; 3]>> = HashMap::new();
    for body in record_bodies(bytes, 0x29) {
        term_at(ctx, bytes, body, wide, &mut out)?;
    }
    for label in find_iter(bytes, b"term_use") {
        let tail = label + b"term_use".len();
        if bytes.get(tail..tail + INLINE_TERM_TAIL.len()) == Some(INLINE_TERM_TAIL) {
            term_at(ctx, bytes, tail + INLINE_TERM_TAIL.len(), wide, &mut out)?;
        }
    }
    Ok(out)
}

/// Parse a support-UV body: `count:u32 attr:u16 width_marker:u8(2|3|4)` then
/// `count` finite f64 values.
fn uv_at(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    body: usize,
    wide: bool,
) -> Result<Option<(u16, UvRecord)>, CodecError> {
    let record = uv_at_framed(ctx, bytes, body, false)?;
    if record.is_none() && wide {
        return uv_at_framed(ctx, bytes, body, true);
    }
    Ok(record)
}

/// [`uv_at`] with one reading of the attribute.
fn uv_at_framed(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    body: usize,
    wide_attr: bool,
) -> Result<Option<(u16, UvRecord)>, CodecError> {
    let Some((attr, shift)) = own_attr(bytes, body + support_uv::ATTR, wide_attr) else {
        return Ok(None);
    };
    let (Some(count), Some(marker)) = (
        View::u32_be_at(bytes, body + support_uv::COUNT)
            .and_then(|count| usize::try_from(count).ok()),
        bytes.get(body + shift + support_uv::WIDTH).copied(),
    ) else {
        return Ok(None);
    };
    let width = match marker {
        2 | 3 => UvWidth::Two,
        4 => UvWidth::Four,
        _ => return Ok(None),
    };
    if count < width.values_per_point() * 2 || !count.is_multiple_of(width.values_per_point()) {
        return Ok(None);
    }
    let Some(values_at) = body.checked_add(shift + support_uv::LEN) else {
        return Ok(None);
    };
    if cadmpeg_core::decode::bounded_len(
        u64_from_index(count),
        8,
        bytes.len().checked_sub(values_at).map_or(0, |len| len),
    ) != Some(count)
    {
        return Ok(None);
    }
    if !(0..count)
        .all(|index| View::f64_be_at(bytes, values_at + index * 8).is_some_and(f64::is_finite))
    {
        return Ok(None);
    }
    let mut values = ctx.collection_vec(count, "decode Parasolid support UV values")?;
    for index in 0..count {
        if let Some(value) = View::f64_be_at(bytes, values_at + index * 8) {
            values.push(value);
        }
    }
    Ok(values
        .iter()
        .all(|value| value.is_finite())
        .then_some((attr, UvRecord { width, values })))
}

/// Every `00 cc` or inline `values` support-UV record, keyed by attribute.
fn uv_records(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    wide: bool,
) -> Result<HashMap<u16, Vec<UvRecord>>, CodecError> {
    let mut out: HashMap<u16, Vec<UvRecord>> = HashMap::new();
    for body in record_bodies(bytes, 0xcc) {
        if let Some((attr, shape)) = uv_at(ctx, bytes, body, wide)? {
            ctx.push_hash_group(
                &mut out,
                attr,
                shape,
                "collect Parasolid support UV groups",
                "collect Parasolid support UV records",
            )?;
        }
    }
    for label in find_iter(bytes, b"values") {
        let tail = label + b"values".len();
        if bytes.get(tail..tail + INLINE_UV_TAIL.len()) == Some(INLINE_UV_TAIL) {
            if let Some((attr, shape)) = uv_at(ctx, bytes, tail + INLINE_UV_TAIL.len(), wide)? {
                ctx.push_hash_group(
                    &mut out,
                    attr,
                    shape,
                    "collect Parasolid support UV groups",
                    "collect Parasolid support UV records",
                )?;
            }
        }
    }
    Ok(out)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    Point3::from(a).distance(Point3::from(b))
}

/// Build the derived polyline curve for one validated composite.
fn solved_curve(
    ctx: &DecodeContext<'_>,
    chart: &Chart,
    start: [f64; 3],
    end: [f64; 3],
    record_attr: u16,
    refusal: &mut crate::lane_refusal::LaneRefusals,
) -> Result<Option<(CurveGeometry, Vec<f64>, bool)>, CodecError> {
    let mut parameter = chart.base_parameter;
    let point_count = chart.interior_points.len() + 2;
    let mut parameters =
        ctx.collection_vec(point_count, "construct intersection chart parameters")?;
    parameters.push(parameter);
    let mut previous = chart.endpoints[0];
    for &point in chart
        .interior_points
        .iter()
        .chain(std::iter::once(&chart.endpoints[1]))
    {
        parameter += distance(previous, point) * chart.base_scale;
        parameters.push(parameter);
        previous = point;
    }
    let mut points = ctx.collection_vec(point_count, "construct intersection chart points")?;
    points.push(start);
    points.extend(chart.interior_points.iter().copied());
    points.push(end);
    let reversed = if parameters.windows(2).all(|pair| pair[0] < pair[1]) {
        false
    } else if parameters.windows(2).all(|pair| pair[0] > pair[1]) {
        parameters.reverse();
        points.reverse();
        true
    } else {
        return Ok(None);
    };
    let (first, last) = if reversed {
        (parameter, chart.base_parameter)
    } else {
        (chart.base_parameter, parameter)
    };
    let mut knots = ctx.collection_vec(point_count + 2, "construct intersection chart knots")?;
    knots.push(first);
    knots.extend(parameters.iter().copied());
    knots.push(last);
    let mut controls = ctx.collection_vec(point_count, "construct intersection curve controls")?;
    controls.extend(
        points
            .iter()
            .map(|p| Point3::new(p[0] * LEN_TO_MM, p[1] * LEN_TO_MM, p[2] * LEN_TO_MM)),
    );
    let nurbs = match cadmpeg_ir::geometry::nurbs::NurbsCurve::from_lanes(
        ctx, 1, knots, controls, None, false,
    )? {
        Ok(nurbs) => nurbs,
        Err(error) => {
            refusal.note(
                ctx,
                format_args!("sldprt intersection chart curve for record attr {record_attr}"),
                &error,
            )?;
            return Ok(None);
        }
    };
    Ok(Some((
        CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(nurbs)),
        parameters,
        reversed,
    )))
}

fn solved_support_uv(
    ctx: &DecodeContext<'_>,
    parameters: &[f64],
    reversed: bool,
    records: Option<&[UvRecord]>,
) -> Result<Option<[Vec<Point2>; 2]>, CodecError> {
    let Some(expected_values) = parameters.len().checked_mul(4) else {
        return Ok(None);
    };
    let Some(records) = records else {
        return Ok(None);
    };
    let mut candidate: Option<[Vec<Point2>; 2]> = None;
    for record in records {
        if record.width != UvWidth::Four || record.values.len() != expected_values {
            continue;
        }
        let mut controls = [Vec::new(), Vec::new()];
        for (support, control_points) in controls.iter_mut().enumerate() {
            ctx.reserve_vec(
                control_points,
                parameters.len(),
                "construct intersection support UV controls",
            )?;
            control_points.extend(
                record
                    .values
                    .chunks_exact(4)
                    .map(|row| Point2::new(row[support * 2], row[support * 2 + 1])),
            );
            if reversed {
                control_points.reverse();
            }
        }
        if candidate
            .as_ref()
            .is_some_and(|previous| previous != &controls)
        {
            return Ok(None);
        }
        candidate = Some(controls);
    }
    Ok(candidate)
}

fn nearest_term(
    records: &HashMap<u16, Vec<[f64; 3]>>,
    attr: u16,
    endpoint: [f64; 3],
) -> Option<([f64; 3], f64)> {
    records
        .get(&attr)?
        .iter()
        .copied()
        .fold(None, |best, point| {
            let candidate = (point, distance(point, endpoint));
            match best {
                Some(best) if best.1 <= candidate.1 => Some(best),
                _ => Some(candidate),
            }
        })
}

/// Scan intersection carriers whose referenced chart and terminators resolve,
/// keyed by carrier attribute.
///
/// The composite body is `attr:u16 ordinal:u32 refs:u16[5] marker:u8(0x2b|0x2d)`
/// then six payload references `[support0, support1, chart, term_start,
/// term_end, uv]`. Terminators replace the approximate chart endpoints. When
/// both 24-byte and 88-byte chart strides frame, the referenced terminators
/// select the unique candidate with the least endpoint displacement. An absent
/// or inconsistent optional UV record does not invalidate the model-space
/// curve; only a unique complete width-4 record supplies solved pcurves. With
/// `wide`, the partition form whose references take two cells above `0x7ffe`
/// is read as well.
pub(super) fn scan_intersection_carriers(
    ctx: &DecodeContext<'_>,
    bytes: &[u8],
    wide: bool,
    lane_refusals: &mut Vec<LossNote>,
) -> Result<HashMap<u16, IntersectionCarrier>, CodecError> {
    let charts = chart_records(ctx, bytes, wide)?;
    let terms = term_records(ctx, bytes, wide)?;
    let uvs = uv_records(ctx, bytes, wide)?;
    if charts.is_empty() || terms.is_empty() {
        return Ok(HashMap::new());
    }
    let mut out = HashMap::new();
    for (offset, _, attr, marker, wide_payload) in composite_records(bytes, wide) {
        let Some((refs, end)) = composite_payload(bytes, marker, wide_payload) else {
            continue;
        };
        let (chart_ref, start_ref, end_ref, uv_ref) = (refs[2], refs[3], refs[4], refs[5]);
        let Some(candidates) = charts.get(&chart_ref) else {
            continue;
        };
        let mut chart_refusal = crate::lane_refusal::LaneRefusals::new();
        let mut selected: Option<SolvedChart> = None;
        let mut ambiguous = false;
        for chart in candidates {
            let [first, last] = chart.endpoints;
            let Some((start, start_distance)) = nearest_term(&terms, start_ref, first) else {
                continue;
            };
            let Some((end, end_distance)) = nearest_term(&terms, end_ref, last) else {
                continue;
            };
            let endpoint_displacement = start_distance + end_distance;
            let Some((geometry, parameters, reversed)) =
                solved_curve(ctx, chart, start, end, attr, &mut chart_refusal)?
            else {
                continue;
            };
            let fit_tolerance_mm = chart.chordal_error * LEN_TO_MM;
            if !fit_tolerance_mm.is_finite() {
                continue;
            }
            let candidate = SolvedChart {
                geometry,
                parameters,
                fit_tolerance_mm,
                reversed,
                endpoint_displacement,
            };
            match selected.as_ref() {
                None => selected = Some(candidate),
                Some(previous) => match candidate
                    .endpoint_displacement
                    .total_cmp(&previous.endpoint_displacement)
                {
                    std::cmp::Ordering::Less => {
                        selected = Some(candidate);
                        ambiguous = false;
                    }
                    std::cmp::Ordering::Equal => ambiguous = true,
                    std::cmp::Ordering::Greater => {}
                },
            }
        }
        for record in chart_refusal.take_records() {
            let note = crate::loss::spline_lane_refusal(
                ctx,
                format_args!("intersection chart for attr {attr}: {record}"),
            )?;
            ctx.reserve_vec(lane_refusals, 1, "collect intersection chart losses")?;
            lane_refusals.push(note);
        }
        let Some(selected) = selected else {
            continue;
        };
        if ambiguous {
            continue;
        }
        let supports = [refs[0], refs[1]];
        let support_uv = solved_support_uv(
            ctx,
            &selected.parameters,
            selected.reversed,
            uvs.get(&uv_ref).map(Vec::as_slice),
        )?;
        ctx.admit_hash_map_entry(&mut out, &attr, "collect Parasolid intersection carriers")?;
        out.entry(attr).or_insert(IntersectionCarrier {
            carrier: CurveCarrier {
                attr,
                offset,
                end,
                geometry: selected.geometry,
                parameter_range: None,
            },
            support_data: IntersectionSupportData {
                supports,
                fit_tolerance_mm: selected.fit_tolerance_mm,
                support_uv,
            },
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_partition_terminator_with_a_wide_attribute_is_read_only_with_the_wide_framing() {
        let mut bytes = vec![0x00, 0x29];
        bytes.extend_from_slice(&1u32.to_be_bytes());
        // Attribute 65,523 as the two cells of the partition form.
        bytes.extend_from_slice(&[0xff, 0xf3, 0x00, 0x01]);
        bytes.extend_from_slice(b"L?");
        for value in [0.001f64, 0.002, 0.003] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &bytes,
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .expect("test bytes fit service policy");
        let narrow = super::term_records(&ctx, &bytes, false).expect("terminator scan");
        assert!(!narrow.contains_key(&65_523));
        let wide = super::term_records(&ctx, &bytes, true).expect("terminator scan");
        assert_eq!(wide.get(&65_523), Some(&vec![[0.001, 0.002, 0.003]]));
    }

    #[test]
    fn a_partition_intersection_with_wide_pointers_is_read_only_with_the_wide_framing() {
        let mut bytes = vec![0x00, 0x26];
        bytes.extend_from_slice(&21u16.to_be_bytes());
        bytes.extend_from_slice(&5u32.to_be_bytes());
        // The second header reference and the chart reference are above `0x7ffe`.
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&[0x80, 0x07, 0x00, 0x01]);
        bytes.extend_from_slice(&[0, 3, 0, 4, 0, 1]);
        bytes.push(0x2b);
        for reference in [31u16, 32] {
            bytes.extend_from_slice(&reference.to_be_bytes());
        }
        bytes.extend_from_slice(&[0x80, 0x09, 0x00, 0x01]);
        for reference in [34u16, 35, 36] {
            bytes.extend_from_slice(&reference.to_be_bytes());
        }

        assert_eq!(super::composite_records(&bytes, false).count(), 0);
        let readings: Vec<_> = super::composite_records(&bytes, true).collect();
        assert_eq!(readings, vec![(0, 2, 21, 20, true)]);
        let (refs, end) = super::composite_payload(&bytes, 20, true).expect("payload");
        assert_eq!(refs, [31, 32, 0x8009, 34, 35, 36]);
        assert_eq!(end, bytes.len());
    }

    #[test]
    fn numerical_ranges_short_chart_chord_preserves_parameter_increment() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let endpoints = [[0., 0., 0.], [1e-200, 0., 0.]];
        let chart = super::Chart {
            endpoints,
            interior_points: Vec::new(),
            base_parameter: 0.,
            base_scale: 1e200,
            chordal_error: 1e-210,
        };
        let (_, parameters, reversed) = super::solved_curve(
            &ctx,
            &chart,
            endpoints[0],
            endpoints[1],
            0,
            &mut crate::lane_refusal::LaneRefusals::new(),
        )
        .expect("curve admission")
        .unwrap();
        assert_eq!(parameters, vec![0., 1.]);
        assert!(!reversed);
    }

    use super::super::LEN_TO_MM;
    use super::{
        chart_candidates, chart_records, scan_intersection_carriers, term_records, uv_at,
        uv_records, UvWidth, MISSING_PARAMETER,
    };
    use cadmpeg_core::convert::f64_from_index;
    use cadmpeg_core::decode::{DecodeArena, DecodeContext, DecodePolicy, ResourceDimension};
    use cadmpeg_ir::geometry::CurveGeometry;
    use cadmpeg_ir::geometry::SolvedCurveGeometry;
    use cadmpeg_ir::math::Point2;
    use cadmpeg_ir::math::Point3;

    const POINTS: [[f64; 3]; 3] = [[0.0, 0.0, 0.0], [0.01, 0.0, 0.0], [0.01, 0.01, 0.0]];

    fn chart(attr: u16, points: &[[f64; 3]]) -> Vec<u8> {
        let mut bytes = vec![0, 0x28];
        bytes.extend_from_slice(
            &u32::try_from(points.len())
                .expect("point count fits u32")
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&attr.to_be_bytes());
        bytes.extend_from_slice(&0.0f64.to_be_bytes());
        bytes.extend_from_slice(&1.0f64.to_be_bytes());
        bytes.extend_from_slice(
            &u32::try_from(points.len())
                .expect("point count fits u32")
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&1e-5f64.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        bytes.extend_from_slice(&MISSING_PARAMETER.to_be_bytes());
        bytes.extend_from_slice(&MISSING_PARAMETER.to_be_bytes());
        for point in points {
            for value in point {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes
    }

    fn extended_chart(attr: u16, points: &[[f64; 3]], tangent: [f64; 3]) -> Vec<u8> {
        let mut bytes = vec![0, 0x28];
        bytes.extend_from_slice(
            &u32::try_from(points.len())
                .expect("point count fits u32")
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&attr.to_be_bytes());
        bytes.extend_from_slice(&0.0f64.to_be_bytes());
        bytes.extend_from_slice(&1.0f64.to_be_bytes());
        bytes.extend_from_slice(
            &u32::try_from(points.len())
                .expect("point count fits u32")
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&1e-5f64.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        bytes.extend_from_slice(&MISSING_PARAMETER.to_be_bytes());
        bytes.extend_from_slice(&MISSING_PARAMETER.to_be_bytes());
        for point in points {
            for value in point {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&[0u8; 32]);
            for value in tangent {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
            bytes.extend_from_slice(&[0u8; 8]);
        }
        bytes
    }

    fn term(attr: u16, point: [f64; 3]) -> Vec<u8> {
        let mut bytes = vec![0, 0x29];
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&attr.to_be_bytes());
        bytes.extend_from_slice(b"L?");
        for value in point {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    fn uv(attr: u16, rows: usize) -> Vec<u8> {
        let mut bytes = vec![0, 0xcc];
        bytes.extend_from_slice(
            &u32::try_from(rows * 4)
                .expect("row count fits u32")
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&attr.to_be_bytes());
        bytes.push(4);
        for index in 0..rows * 4 {
            bytes.extend_from_slice(
                &f64_from_index(index)
                    .expect("index is exactly representable")
                    .to_be_bytes(),
            );
        }
        bytes
    }

    fn composite(attr: u16, payload: [u16; 6]) -> Vec<u8> {
        let mut bytes = vec![0, 0x26];
        bytes.extend_from_slice(&attr.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 4]);
        bytes.extend_from_slice(&[0u8; 10]);
        bytes.push(0x2b);
        for reference in payload {
            bytes.extend_from_slice(&reference.to_be_bytes());
        }
        bytes
    }

    fn intersection_data(attr: u16, payload: [u16; 6]) -> Vec<u8> {
        let mut bytes = vec![0, 1, 0x5a];
        bytes.extend_from_slice(&attr.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 4]);
        bytes.extend_from_slice(&[0u8; 10]);
        bytes.push(0x2b);
        for reference in payload {
            bytes.extend_from_slice(&reference.to_be_bytes());
        }
        bytes
    }

    fn stream() -> Vec<u8> {
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart(4, &POINTS));
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(uv(7, POINTS.len()));
        bytes
    }

    macro_rules! intersection_collection_boundary {
        ($name:ident, $fixture:expr, $route:ident, $cap:expr, $operation:literal) => {
            #[test]
            fn $name() {
                let bytes = $fixture;
                let arena = DecodeArena::new();
                let mut policy = DecodePolicy::service();
                policy.limits.max_collection_items = $cap;
                let (ctx, _) =
                    DecodeContext::from_root_bytes(&bytes, &arena, &policy).expect("root");
                let Err(error) = $route(&ctx, &bytes, false) else {
                    panic!("record collection exceeded its limit");
                };
                assert!(matches!(error,
                    cadmpeg_core::CodecError::ResourceLimit(limit)
                        if limit.dimension == ResourceDimension::CollectionItems
                            && limit.operation == $operation), "{error:?}");
                let arena = DecodeArena::new();
                let (ctx, _) = DecodeContext::from_root_bytes(
                    &bytes,
                    &arena,
                    &DecodePolicy::service(),
                )
                .expect("root");
                assert!($route(&ctx, &bytes, false).is_ok());
            }
        };
    }

    intersection_collection_boundary!(
        parasolid_chart_interior_points_refuse_before_allocation,
        chart(4, &POINTS),
        chart_records,
        0,
        "decode Parasolid chart interior points"
    );
    intersection_collection_boundary!(
        parasolid_chart_stride_candidates_refuse_before_collection,
        chart(4, &POINTS),
        chart_records,
        1,
        "collect Parasolid chart stride candidates"
    );
    intersection_collection_boundary!(
        parasolid_chart_groups_refuse_before_collection,
        chart(4, &POINTS),
        chart_records,
        2,
        "collect Parasolid intersection charts"
    );
    intersection_collection_boundary!(
        parasolid_chart_candidates_refuse_before_collection,
        chart(4, &POINTS),
        chart_records,
        3,
        "collect Parasolid intersection chart candidates"
    );
    intersection_collection_boundary!(
        parasolid_terminator_groups_refuse_before_collection,
        term(5, POINTS[0]),
        term_records,
        0,
        "collect Parasolid terminator groups"
    );
    intersection_collection_boundary!(
        parasolid_terminator_points_refuse_before_collection,
        term(5, POINTS[0]),
        term_records,
        1,
        "collect Parasolid terminator points"
    );
    intersection_collection_boundary!(
        parasolid_support_uv_values_refuse_before_allocation,
        uv(7, POINTS.len()),
        uv_records,
        11,
        "decode Parasolid support UV values"
    );
    intersection_collection_boundary!(
        parasolid_support_uv_groups_refuse_before_collection,
        uv(7, POINTS.len()),
        uv_records,
        12,
        "collect Parasolid support UV groups"
    );
    intersection_collection_boundary!(
        parasolid_support_uv_records_refuse_before_collection,
        uv(7, POINTS.len()),
        uv_records,
        13,
        "collect Parasolid support UV records"
    );

    fn intersection_curve_boundary(cap: u64, operation: &'static str) {
        let bytes = [0_u8; 1];
        let chart = super::Chart {
            endpoints: [POINTS[0], POINTS[2]],
            interior_points: vec![POINTS[1]],
            base_parameter: 0.0,
            base_scale: 1.0,
            chordal_error: 0.00001,
        };
        let arena = DecodeArena::new();
        let mut policy = DecodePolicy::service();
        policy.limits.max_collection_items = cap;
        let (ctx, _) = DecodeContext::from_root_bytes(&bytes, &arena, &policy).expect("root");
        let Err(error) = super::solved_curve(
            &ctx,
            &chart,
            POINTS[0],
            POINTS[2],
            9,
            &mut crate::lane_refusal::LaneRefusals::new(),
        ) else {
            panic!("curve construction exceeded its limit");
        };
        assert!(
            matches!(error,
            cadmpeg_core::CodecError::ResourceLimit(limit)
                if limit.dimension == ResourceDimension::CollectionItems
                    && limit.operation == operation),
            "{error:?}"
        );
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&bytes, &arena, &DecodePolicy::service()).expect("root");
        assert!(super::solved_curve(
            &ctx,
            &chart,
            POINTS[0],
            POINTS[2],
            9,
            &mut crate::lane_refusal::LaneRefusals::new(),
        )
        .expect("service curve")
        .is_some());
    }

    #[test]
    fn parasolid_intersection_parameters_refuse_before_allocation() {
        intersection_curve_boundary(2, "construct intersection chart parameters");
    }

    #[test]
    fn parasolid_intersection_points_refuse_before_allocation() {
        intersection_curve_boundary(5, "construct intersection chart points");
    }

    #[test]
    fn parasolid_intersection_knots_refuse_before_allocation() {
        intersection_curve_boundary(10, "construct intersection chart knots");
    }

    #[test]
    fn parasolid_intersection_controls_refuse_before_allocation() {
        intersection_curve_boundary(13, "construct intersection curve controls");
    }

    #[test]
    fn parasolid_intersection_poles_refuse_before_admission() {
        intersection_curve_boundary(16, "IR NURBS admitted poles");
    }

    #[test]
    fn parasolid_intersection_support_uv_controls_refuse_before_allocation() {
        let bytes = [0_u8; 1];
        let records = [super::UvRecord {
            width: UvWidth::Four,
            values: vec![0.0; 12],
        }];
        let parameters = [0.0, 0.5, 1.0];
        let arena = DecodeArena::new();
        let mut policy = DecodePolicy::service();
        policy.limits.max_collection_items = 5;
        let (ctx, _) = DecodeContext::from_root_bytes(&bytes, &arena, &policy).expect("root");
        let Err(error) = super::solved_support_uv(&ctx, &parameters, false, Some(&records)) else {
            panic!("six controls exceed five items");
        };
        assert!(matches!(error,
            cadmpeg_core::CodecError::ResourceLimit(limit)
                if limit.dimension == ResourceDimension::CollectionItems
                    && limit.operation == "construct intersection support UV controls"));
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&bytes, &arena, &DecodePolicy::service()).expect("root");
        assert_eq!(
            super::solved_support_uv(&ctx, &parameters, false, Some(&records))
                .expect("service UV")
                .map(|controls| [controls[0].len(), controls[1].len()]),
            Some([3, 3])
        );
    }

    #[test]
    fn parasolid_intersection_carriers_refuse_before_collection() {
        let bytes = stream();
        let arena = DecodeArena::new();
        let mut policy = DecodePolicy::service();
        policy.limits.max_collection_items = 46;
        let (ctx, _) = DecodeContext::from_root_bytes(&bytes, &arena, &policy).expect("root");
        let Err(error) = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new()) else {
            panic!("carrier insertion exceeds the collection limit");
        };
        assert!(
            matches!(error,
            cadmpeg_core::CodecError::ResourceLimit(limit)
                if limit.dimension == ResourceDimension::CollectionItems
                    && limit.operation == "collect Parasolid intersection carriers"),
            "{error:?}"
        );
        let arena = DecodeArena::new();
        let (ctx, _) =
            DecodeContext::from_root_bytes(&bytes, &arena, &DecodePolicy::service()).expect("root");
        assert!(
            scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
                .expect("service scan")
                .contains_key(&9)
        );
    }

    #[test]
    fn marker_three_uv_has_two_values_per_chart_point() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let points = (0..9)
            .map(|index| [f64::from(index) * 0.01, 0.0, 0.0])
            .collect::<Vec<_>>();
        let mut record = uv(7, points.len());
        record
            .get_mut(2..6)
            .unwrap()
            .copy_from_slice(&18_u32.to_be_bytes());
        *record.get_mut(8).unwrap() = 3;
        record.truncate(9 + 18 * 8);
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart(4, &points));
        bytes.extend(term(5, *points.first().unwrap()));
        bytes.extend(term(6, *points.last().unwrap()));
        bytes.extend(record);
        let charts = chart_records(&ctx, &bytes, false).expect("chart scan");
        let records = uv_records(&ctx, &bytes, false).expect("UV scan");
        let chart = &charts[&4][0];
        let uv = &records[&7][0];
        assert_eq!(chart.interior_points.len() + 2, 9);
        assert!(uv.width == UvWidth::Two);
        assert_eq!(uv.values.len(), (chart.interior_points.len() + 2) * 2);
        assert!(
            scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
                .expect("intersection scan")
                .contains_key(&9)
        );
    }

    #[test]
    fn width_two_uv_is_legal_without_a_paired_support_cache() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let mut record = vec![0, 0xcc];
        record.extend_from_slice(&6_u32.to_be_bytes());
        record.extend_from_slice(&7_u16.to_be_bytes());
        record.push(2);
        for value in [0.0_f64, 1.0, 2.0, 3.0, 4.0, 5.0] {
            record.extend_from_slice(&value.to_be_bytes());
        }
        assert!(uv_at(&ctx, &record, 2, false).expect("UV parse").is_some());
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart(4, &POINTS));
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(record);
        let carriers = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan");
        let carrier = carriers
            .get(&9)
            .expect("width-two support leaves the curve available");
        assert!(carrier.support_data.support_uv.is_none());
    }

    #[test]
    fn consistent_composite_yields_polyline() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let carriers = scan_intersection_carriers(&ctx, &stream(), false, &mut Vec::new())
            .expect("intersection scan");
        let carrier = carriers.get(&9).expect("composite decoded");
        let Some(SolvedCurveGeometry::Nurbs(curve)) = carrier.carrier.geometry.solved() else {
            panic!("expected a NURBS polyline");
        };
        assert_eq!(curve.degree(), 1);
        assert_eq!(curve.control_points().len(), 3);
        assert_eq!(curve.control_points()[1], Point3::new(10.0, 0.0, 0.0));
        assert_eq!(curve.knots().len(), 5);
        assert!((curve.knots()[2] - 0.01).abs() < 1.0e-12);
        assert!((curve.knots()[3] - 0.02).abs() < 1.0e-12);
        let support_data = &carrier.support_data;
        assert_eq!(support_data.supports, [2, 3]);
        let support_uv = support_data
            .support_uv
            .as_ref()
            .expect("width-four UV cache");
        assert_eq!(
            support_uv[0],
            &[
                Point2::new(0.0, 1.0),
                Point2::new(4.0, 5.0),
                Point2::new(8.0, 9.0)
            ]
        );
        assert_eq!(support_data.fit_tolerance_mm, 0.01);
    }

    #[test]
    fn intersection_data_entity_uses_the_same_composite_payload() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let mut bytes = intersection_data(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart(4, &POINTS));
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(uv(7, POINTS.len()));

        let carrier = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan")
            .remove(&9)
            .expect("intersection-data entity decoded");
        assert_eq!(carrier.carrier.offset, 0);
        let Some(SolvedCurveGeometry::Nurbs(curve)) = carrier.carrier.geometry.solved() else {
            panic!("expected a NURBS polyline");
        };
        assert_eq!(curve.control_points().len(), POINTS.len());
    }

    #[test]
    fn negative_chart_scale_reverses_curve_and_uv_caches_atomically() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        let mut chart = chart(4, &POINTS);
        chart[16..24].copy_from_slice(&(-1.0f64).to_be_bytes());
        bytes.extend(chart);
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(uv(7, POINTS.len()));

        let carriers = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan");
        let carrier = carriers.get(&9).expect("decreasing chart decoded");
        let Some(SolvedCurveGeometry::Nurbs(curve)) = carrier.carrier.geometry.solved() else {
            panic!("expected a NURBS polyline");
        };
        assert_eq!(curve.knots().as_slice(), [-0.02, -0.02, -0.01, 0.0, 0.0]);
        assert_eq!(
            curve.control_points(),
            [
                Point3::new(10.0, 10.0, 0.0),
                Point3::new(10.0, 0.0, 0.0),
                Point3::new(0.0, 0.0, 0.0),
            ]
        );
        let support_data = &carrier.support_data;
        let control_points = &support_data.support_uv.as_ref().expect("UV cache")[0];
        assert_eq!(control_points[0], Point2::new(8.0, 9.0));
        assert_eq!(control_points[2], Point2::new(0.0, 1.0));
        assert_eq!(curve.knots().as_slice(), [-0.02, -0.02, -0.01, 0.0, 0.0]);
    }

    #[test]
    fn seam_row_uv_count_is_accepted() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart(4, &POINTS));
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(uv(7, POINTS.len() + 1));
        let carriers = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan");
        let carrier = carriers.get(&9).expect("seam-row carrier decoded");
        assert!(carrier.support_data.support_uv.is_none());
    }

    #[test]
    fn exact_terminator_replaces_an_approximate_chart_endpoint() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let end = [0.011, 0.01, 0.0];
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart(4, &POINTS));
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, end));
        bytes.extend(uv(7, POINTS.len()));
        let carriers = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan");
        let CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(curve)) = &carriers
            .get(&9)
            .expect("composite decoded")
            .carrier
            .geometry
        else {
            panic!("expected a NURBS polyline");
        };
        assert_eq!(
            *curve.control_points().last().expect("points"),
            Point3::new(end[0] * LEN_TO_MM, end[1] * LEN_TO_MM, end[2] * LEN_TO_MM),
        );
    }

    #[test]
    fn missing_chart_sentinels_reject_the_chart() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        let mut bad = chart(4, &POINTS);
        let at = bad.len() - POINTS.len() * 24 - 16;
        bad[at..at + 8].copy_from_slice(&0.0f64.to_be_bytes());
        bytes.extend(bad);
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(uv(7, POINTS.len()));
        assert!(
            scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
                .expect("intersection scan")
                .is_empty()
        );
    }

    #[test]
    fn ring_composite_with_one_char_label_and_no_uv_record_decodes() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let ring = [
            [0.0, 0.0, 0.0],
            [0.01, 0.0, 0.0],
            [0.01, 0.01, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let mut bytes = composite(9, [2, 3, 4, 5, 5, 6]);
        bytes.extend(chart(4, &ring));
        let mut term = vec![0u8, 0x29];
        term.extend_from_slice(&1u32.to_be_bytes());
        term.extend_from_slice(&5u16.to_be_bytes());
        term.push(b'H');
        for value in ring[0] {
            term.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend(term);
        let carriers = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan");
        let CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(curve)) = &carriers
            .get(&9)
            .expect("ring composite decoded")
            .carrier
            .geometry
        else {
            panic!("expected a NURBS polyline");
        };
        assert_eq!(curve.control_points().len(), 4);
        assert_eq!(curve.control_points()[0], curve.control_points()[3]);
    }

    #[test]
    fn mismatched_optional_uv_count_does_not_reject_the_curve() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart(4, &POINTS));
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(uv(7, POINTS.len() + 2));
        let carriers = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan");
        assert!(carriers.contains_key(&9));
        assert!(carriers[&9].support_data.support_uv.is_none());
    }

    #[test]
    fn extended_chart_stride_is_selected_by_witnesses() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let chart_bytes = extended_chart(4, &POINTS, [0.5, 0.0, 0.0]);
        let (_, candidates) = chart_candidates(&ctx, &chart_bytes, 2, false)
            .expect("chart parse")
            .expect("chart candidates");
        assert_eq!(candidates.len(), 2);

        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart_bytes);
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, POINTS[2]));
        bytes.extend(uv(7, POINTS.len()));

        let carriers = scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
            .expect("intersection scan");
        let CurveGeometry::Solved(SolvedCurveGeometry::Nurbs(curve)) = &carriers
            .get(&9)
            .expect("extended chart decoded")
            .carrier
            .geometry
        else {
            panic!("expected a NURBS polyline");
        };
        assert_eq!(curve.control_points().len(), POINTS.len());
        assert_eq!(
            *curve.control_points().last().expect("points"),
            Point3::new(
                POINTS[2][0] * LEN_TO_MM,
                POINTS[2][1] * LEN_TO_MM,
                POINTS[2][2] * LEN_TO_MM,
            ),
        );
    }

    #[test]
    fn ambiguous_chart_stride_does_not_select_first_candidate() {
        let arena = cadmpeg_core::decode::DecodeArena::new();
        let (ctx, _) = cadmpeg_core::decode::DecodeContext::from_root_bytes(
            &[],
            &arena,
            &cadmpeg_core::decode::DecodePolicy::service(),
        )
        .unwrap();
        let end = POINTS[2];
        let mut chart_bytes = extended_chart(4, &[POINTS[0], end], [0.5, 0.0, 0.0]);
        let bare_endpoint = 2 + 6 + 52 + 24;
        for (index, value) in end.into_iter().enumerate() {
            chart_bytes[bare_endpoint + index * 8..bare_endpoint + (index + 1) * 8]
                .copy_from_slice(&value.to_be_bytes());
        }
        let (_, candidates) = chart_candidates(&ctx, &chart_bytes, 2, false)
            .expect("chart parse")
            .expect("chart candidates");
        assert_eq!(candidates.len(), 2);

        let mut bytes = composite(9, [2, 3, 4, 5, 6, 7]);
        bytes.extend(chart_bytes);
        bytes.extend(term(5, POINTS[0]));
        bytes.extend(term(6, end));
        bytes.extend(uv(7, 2));
        assert!(
            scan_intersection_carriers(&ctx, &bytes, false, &mut Vec::new())
                .expect("intersection scan")
                .is_empty()
        );
    }
}
