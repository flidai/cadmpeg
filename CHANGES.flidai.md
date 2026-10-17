# Changes from cadmpeg v0.5.5

- 2026-10-17: flidai added `DecodePurpose::Measure` in cadmpeg-asm to retain
  Model topology and geometry while omitting attributes, passthrough records,
  residual record counts and annotations.
- 2026-10-17: flidai changed cadmpeg-ir B-spline basis evaluation to update
  levels in place, with inline storage through degree 15 and checked heap
  storage for higher degrees. The floating-point recurrence is unchanged.
