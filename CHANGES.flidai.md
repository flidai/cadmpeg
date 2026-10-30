# flidai measurement branch

Based on upstream `v0.6.7`. Modified files retain the upstream Apache-2.0 license.
None of these changes is submitted upstream except the first.

- `crates/cadmpeg-asm/src/nurbs/{blend,core}.rs`, `docs/formats/asm.md`: keep a
  variable-blend current NURBS cache when the radius law omits its calibration
  enum; the exact construction is withheld. Cherry-picked from upstream pull
  request branch `fix/variable-blend-cache-recovery` (`163ff27e`). Drop this
  commit once that pull request is in a release.
- `crates/cadmpeg-asm/src/brep/mod.rs` and tests: `DecodePurpose::Measure`
  transfers complete topology and geometry (including point-edge carriers, body
  kind and edge-range clamping) without attributes, passthrough records,
  residual counts or source annotations. Used by the measurement pass.
- `crates/cadmpeg-ir/src/eval.rs`, `eval/{basis,decode,rational}.rs` and
  `eval/precision_tests.rs`: opt-in `EvalPrecision::Fast` on the
  `*_with_precision` entry points (`curve_point_solved`, `pcurve_uv`,
  `nurbs_pcurve_uv`, `nurbs_curve_point_at`, `nurbs_surface_point`,
  `nurbs_surface_partials`, `analytic_surface_parameters_solved`). Fast uses the
  0.5 binary64 basis recurrence, ordered homogeneous sums over borrowed pole
  rows, rational first partials, analytic cross products, offsets and inverse
  parameters. Existing entry points keep exact evaluation. Fast can lose
  accuracy or finite values through cancellation or intermediate overflow; it
  is for rounded measurements, not exact predicates. Frozen 0.5 bit-for-bit
  tests cover curves, pcurves, surfaces and analytic carriers.
- `crates/cadmpeg-asm/src/nurbs/toks.rs`: `SubtypeTable` memoizes each
  definition's balanced scope length over its immutable tokens, so repeated
  reference lookups do not rescan delimiters.

## Dropped from the 0.6.1 fork

- ZIP 9.0.5 and Zstandard 0.14.3: upstream has them.
- Upper-V-bound variable-blend cache (`e0ff4727`) and terminal 3D-curve cache
  (`6a7aca52`): upstream recovers both independently.
- Skip of oversized optional constructions before Measure admission
  (`9b8b2be3`): 0.6.7 leaves an oversized support unrecovered instead of
  refusing the decode, so the owned face cache survives in every purpose. The
  scenario is kept as a test.
- Scoped-vector capacity fit and reference-free record shortcuts (`8dbc4268`)
  and the work-accounting shortcuts of the Fast speed-ups: 0.6.7 removed the
  scoped-storage and collection/work-unit accounting they optimized; the
  remaining admission check is a plain budget compare.
