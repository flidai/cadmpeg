// SPDX-License-Identifier: Apache-2.0
use super::*;

// The v0.5.5 recurrence is the bitwise oracle, including its level buffers.
fn allocating_basis(knots: &[f64], degree: usize, span: usize, t: f64) -> Vec<f64> {
    let mut values = vec![1.0];
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
    for j in 1..=degree {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        let mut next = vec![0.0; j + 1];
        for (r, &value) in values.iter().enumerate().take(j) {
            let denominator = right[r + 1] + left[j - r];
            let factor = if denominator == 0.0 {
                0.0
            } else {
                value / denominator
            };
            next[r] = saved + right[r + 1] * factor;
            saved = left[j - r] * factor;
        }
        next[j] = saved;
        values = next;
    }
    values
}

#[test]
fn basis_is_bit_identical_for_inline_and_heap_degrees() {
    for degree in 0..=40 {
        let count = degree + 5;
        for repeated in [false, true] {
            let knots: Vec<f64> = (0..=(count + degree))
                .map(|i| {
                    let i = if repeated { i / 3 } else { i };
                    f64::from(u32::try_from(i).unwrap()) * 0.37 - 2.0
                })
                .collect();
            for span in degree..count {
                for fraction in [0.0, 0.01, 0.25, 0.5, 0.99, 1.0] {
                    let t = knots[span] + fraction * (knots[span + 1] - knots[span]);
                    let expected = allocating_basis(&knots, degree, span, t);
                    let actual = bspline_basis(&knots, degree, span, t).unwrap();
                    assert_eq!(matches!(actual, BasisValues::Inline { .. }), degree < 16);
                    assert_eq!(
                        actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        "degree={degree}, span={span}, t={t}, repeated={repeated}",
                    );
                }
            }
        }
    }
}
