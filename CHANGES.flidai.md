# flidai measurement branch

Based on upstream `v0.6.1`. Modified files retain the upstream Apache-2.0 license.

- `crates/cadmpeg-asm/src/brep/mod.rs` and its tests: port `DecodePurpose::Measure`
  from `fd20d2f804b3a5c5cd60c1d643316aadd3954657`. Retain Model geometry,
  topology, body classification and edge-range clamping; omit report-only data.
- Upstream 0.6.1 already has degree-15 inline basis storage and admitted heap
  fallback, superseding the storage change in
  `b5006de6e0e4974d42e949a09a12d541d3c55ae1`.
- `crates/cadmpeg-ir/src/eval.rs`, `eval/{basis,decode,rational}.rs` and their
  tests: add opt-in `EvalPrecision::Fast` to measurement entry points. Use the
  0.5 binary64 basis recurrence, ordered homogeneous sums, rational first
  partials, analytic cross products, offsets and inverse parameters. Direct
  NURBS pcurve point sampling borrows weights and omits unused derivatives. Existing entry points
  keep exact evaluation. Inline storage and resource refusal remain active.
  Fast can lose accuracy or finite values through cancellation or intermediate
  overflow; it is intended for rounded measurements, not exact predicates.
- Fast surface sums traverse the active net directly and admit its work in one
  charge, sharing the ordered accumulator with curve sums. Standard evaluation
  skips no-op basis work accounting; admitted evaluation retains its limits.
- `crates/cadmpeg-asm/src/nurbs/{blend,core,proc_surface}.rs`: recover a
  variable-blend current NURBS cache when its optional upper V bound is present.
  Retain that bound while parsing, but withhold exact construction emission
  because the construction carrier cannot represent it. Synthetic tests cover
  current/stale caches and named/compact aliases. Separate upstream bugfix
  candidate; not submitted upstream.
- Fast curve sums use the same direct ordered accumulator as surface sums.
  Standard evaluation avoids per-lane no-op admission; admitted evaluation
  still charges every traversal. Inline local surface helpers avoid copying
  unused derivative state for point-only callers.
