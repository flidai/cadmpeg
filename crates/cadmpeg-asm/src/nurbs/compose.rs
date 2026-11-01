// SPDX-License-Identifier: Apache-2.0
//! Joining ranges of NURBS curves end to end into one NURBS curve.

use cadmpeg_ir::geometry::nurbs::bezier::homogeneous_spans;
use cadmpeg_ir::geometry::nurbs::NurbsCurve;
use cadmpeg_ir::math::Point3;

/// Weighted control point `(w*x, w*y, w*z, w)`.
type Control = [f64; 4];

/// Largest distance, in document length units, between the end of one part and
/// the start of the next.
const MAX_JOINT_GAP: f64 = 1.0e-3;

/// Largest distance of a weight from one that still reads as a polynomial pole.
const UNIT_WEIGHT_SLACK: f64 = 1.0e-12;

/// A range of one curve, in the curve's own parameter.
pub(super) struct CurvePart {
    /// The curve the part is cut from.
    pub curve: NurbsCurve,
    /// Lower and upper parameter kept; an absent bound is the domain end.
    pub range: [Option<f64>; 2],
}

fn homogeneous(curve: &NurbsCurve) -> Option<Vec<Control>> {
    let weights = curve.weights();
    curve
        .control_points()
        .into_iter()
        .enumerate()
        .map(|(index, point)| {
            let weight = weights.as_ref().map_or(1.0, |all| all[index].get());
            let point = point.get();
            Some([weight * point.x, weight * point.y, weight * point.z, weight])
        })
        .collect()
}

fn lerp(first: &Control, second: &Control, fraction: f64) -> Control {
    std::array::from_fn(|axis| (1.0 - fraction) * first[axis] + fraction * second[axis])
}

/// Split a Bezier polygon at a fraction of its parameter interval.
fn split(controls: &[Control], fraction: f64) -> (Vec<Control>, Vec<Control>) {
    let count = controls.len();
    let mut work = controls.to_vec();
    let mut left = vec![work[0]];
    let mut right = controls.to_vec();
    right[count - 1] = work[count - 1];
    for level in 1..count {
        for index in 0..count - level {
            work[index] = lerp(&work[index], &work[index + 1], fraction);
        }
        left.push(work[0]);
        right[count - 1 - level] = work[count - 1 - level];
    }
    (left, right)
}

/// The polygon of the Bezier sub-interval between two parameter fractions.
fn sub_interval(controls: &[Control], from: f64, to: f64) -> Vec<Control> {
    let rest = if from > 0.0 {
        split(controls, from).1
    } else {
        controls.to_vec()
    };
    if to >= 1.0 {
        return rest;
    }
    let local = ((to - from) / (1.0 - from)).clamp(0.0, 1.0);
    split(&rest, local).0
}

/// The same Bezier curve one degree higher.
fn elevate(controls: &[Control]) -> Option<Vec<Control>> {
    let degree = controls.len() - 1;
    let denominator = f64::from(u32::try_from(degree + 1).ok()?);
    let mut raised = vec![controls[0]];
    for index in 1..=degree {
        let fraction = f64::from(u32::try_from(index).ok()?) / denominator;
        raised.push(lerp(&controls[index], &controls[index - 1], fraction));
    }
    raised.push(controls[degree]);
    Some(raised)
}

fn position(control: &Control) -> Option<Point3> {
    (control[3] != 0.0).then(|| {
        Point3::new(
            control[0] / control[3],
            control[1] / control[3],
            control[2] / control[3],
        )
    })
}

/// Join the parts end to end as one curve. The first part starts at `start`,
/// or at its own lower bound when none is given, and each part runs on a
/// parameter that advances at the pace of its own curve. Refuses a part that
/// cannot be cut, and a pair of parts whose ends are further apart than the
/// joint allowance.
pub(super) fn join_curve_parts(parts: &[CurvePart], start: Option<f64>) -> Option<NurbsCurve> {
    // A span: its parameter interval on the joined curve and its polygon.
    let mut spans: Vec<([f64; 2], Vec<Control>)> = Vec::new();
    let mut offset = start;
    for part in parts {
        let degree = usize::try_from(part.curve.degree()).ok()?;
        let source = homogeneous(&part.curve)?;
        let cut = homogeneous_spans(degree, part.curve.knots().as_slice(), &source)?;
        let domain = [cut.first()?.domain[0], cut.last()?.domain[1]];
        let lower = part.range[0].unwrap_or(domain[0]).max(domain[0]);
        let upper = part.range[1].unwrap_or(domain[1]).min(domain[1]);
        if lower >= upper {
            return None;
        }
        let base = *offset.get_or_insert(lower);
        for span in &cut {
            let [first, last] = span.domain;
            let (kept_first, kept_last) = (first.max(lower), last.min(upper));
            if kept_first >= kept_last {
                continue;
            }
            let polygon = sub_interval(
                &span.controls,
                (kept_first - first) / (last - first),
                (kept_last - first) / (last - first),
            );
            spans.push((
                [base + (kept_first - lower), base + (kept_last - lower)],
                polygon,
            ));
        }
        offset = Some(base + (upper - lower));
    }
    let degree = spans.iter().map(|(_, polygon)| polygon.len() - 1).max()?;
    for (_, polygon) in &mut spans {
        while polygon.len() - 1 < degree {
            *polygon = elevate(polygon)?;
        }
    }
    let mut knots: Vec<f64> = std::iter::repeat_n(spans.first()?.0[0], degree + 1).collect();
    let mut controls: Vec<Control> = Vec::new();
    for (index, (interval, polygon)) in spans.iter().enumerate() {
        let mut polygon = polygon.clone();
        if let Some(previous) = controls.last() {
            let gap = position(previous)
                .zip(position(&polygon[0]))
                .map(|(from, to)| Point3::distance(from, to))?;
            if gap.is_nan() || gap > MAX_JOINT_GAP {
                return None;
            }
            // A shared control carries one weight, so the polygon is scaled to
            // the weight already there, which leaves its curve unchanged.
            let scale = previous[3] / polygon[0][3];
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }
            for control in &mut polygon {
                for value in control.iter_mut() {
                    *value *= scale;
                }
            }
            polygon[0] = *previous;
            controls.pop();
        }
        controls.extend(polygon);
        let closing = if index + 1 == spans.len() {
            degree + 1
        } else {
            degree
        };
        knots.extend(std::iter::repeat_n(interval[1], closing));
    }
    let rational = controls
        .iter()
        .any(|control| (control[3] - 1.0).abs() > UNIT_WEIGHT_SLACK);
    let points: Vec<Point3> = controls.iter().map(position).collect::<Option<_>>()?;
    let weights = rational.then(|| controls.iter().map(|control| control[3]).collect());
    NurbsCurve::from_lanes(u32::try_from(degree).ok()?, knots, points, weights, false).ok()
}

#[cfg(test)]
mod tests {
    use super::{join_curve_parts, CurvePart};
    use cadmpeg_ir::geometry::nurbs::NurbsCurve;
    use cadmpeg_ir::math::Point3;

    fn line(from: [f64; 3], to: [f64; 3], lower: f64, upper: f64) -> NurbsCurve {
        NurbsCurve::from_lanes(
            1,
            vec![lower, lower, upper, upper],
            vec![
                Point3::new(from[0], from[1], from[2]),
                Point3::new(to[0], to[1], to[2]),
            ],
            None,
            false,
        )
        .expect("line")
    }

    fn quadratic() -> NurbsCurve {
        NurbsCurve::from_lanes(
            2,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![
                Point3::new(2.0, 0.0, 0.0),
                Point3::new(3.0, 1.0, 0.0),
                Point3::new(4.0, 0.0, 0.0),
            ],
            None,
            false,
        )
        .expect("quadratic")
    }

    #[test]
    fn two_lines_join_into_one_curve_on_consecutive_parameters() {
        let joined = join_curve_parts(
            &[
                CurvePart {
                    curve: line([0.0; 3], [1.0, 0.0, 0.0], 10.0, 11.0),
                    range: [None, None],
                },
                CurvePart {
                    curve: line([1.0, 0.0, 0.0], [2.0, 0.0, 0.0], 0.0, 4.0),
                    range: [None, None],
                },
            ],
            Some(5.0),
        )
        .expect("joined");
        assert_eq!(joined.degree(), 1);
        assert_eq!(joined.knots().as_slice(), [5.0, 5.0, 6.0, 10.0, 10.0]);
        let xs: Vec<f64> = joined.control_points().iter().map(|p| p.get().x).collect();
        assert_eq!(xs, [0.0, 1.0, 2.0]);
    }

    #[test]
    fn a_range_cuts_the_part_and_a_lower_degree_part_is_raised() {
        let joined = join_curve_parts(
            &[
                CurvePart {
                    curve: line([0.0; 3], [2.0, 0.0, 0.0], 0.0, 2.0),
                    range: [Some(0.5), None],
                },
                CurvePart {
                    curve: quadratic(),
                    range: [None, Some(0.5)],
                },
            ],
            None,
        )
        .expect("joined");
        assert_eq!(joined.degree(), 2);
        // The first part keeps [0.5, 2] of a unit-pace line: 1.5 long, from
        // x = 0.5 to x = 2, and the second keeps the first half of the
        // quadratic after it.
        let knots = joined.knots().as_slice();
        assert_eq!(knots.first().copied(), Some(0.5));
        assert_eq!(knots.last().copied(), Some(2.5));
        let points = joined.control_points();
        let first = points.first().expect("first").get();
        assert!((first.x - 0.5).abs() < 1.0e-12);
        let last = points.last().expect("last").get();
        // The quadratic at one half: (2 + 2*3 + 4) / 4 = 3, height 0.5.
        assert!((last.x - 3.0).abs() < 1.0e-12);
        assert!((last.y - 0.5).abs() < 1.0e-12);
    }

    #[test]
    fn parts_that_do_not_meet_are_refused() {
        let joined = join_curve_parts(
            &[
                CurvePart {
                    curve: line([0.0; 3], [1.0, 0.0, 0.0], 0.0, 1.0),
                    range: [None, None],
                },
                CurvePart {
                    curve: line([1.5, 0.0, 0.0], [2.0, 0.0, 0.0], 0.0, 1.0),
                    range: [None, None],
                },
            ],
            None,
        );
        assert!(joined.is_none());
    }
}
