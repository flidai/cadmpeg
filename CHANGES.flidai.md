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
- Scoped vector reservations that fit existing capacity keep item and sticky
  refusal checks without changing storage ownership. Reference-free records
  retain scan-work admission without allocating a traversal stack. Tests cover
  growth, zero-sized items, collection refusal and scan-work refusal.
- Measure checks optional construction support populations before pole
  admission. An oversized support skips only that optional construction,
  records `measurement-construction-oversized-support`, and retains a valid
  owned face cache. Model admission and all resource limits stay unchanged.
  Synthetic tests cover a complete oversized grid, valid cache, aliases and
  real refusal propagation. Separate upstream candidate; not submitted.
- Variable-blend current-cache selection accepts balanced typed supports
  without their own surface cache and Boolean-leading radius laws without a
  calibration enum. These layouts remain ineligible for exact construction
  emission. Current/stale, named/compact alias, reference-depth and scan-work
  refusal tests protect cache ownership and admission. Separate upstream
  bugfix candidate; not submitted.
- Variable-blend cache selection accepts a terminal 3D curve only when it
  consumes the complete remaining scope; the exact construction still requires
  a pcurve. Current/stale and named/compact alias tests reject extra tail fields.
  Resource refusals propagate without trying another layout. Separate upstream
  bugfix candidate; not submitted.
- Subtype definitions memoize their balanced scope length over immutable tokens.
  Repeated reference admission keeps the same work and depth charges while
  avoiding repeated delimiter scans. Inline cache storage remains accounted
  by the definition vector reservation. Tests cover nested/malformed scopes,
  Send/Sync and independent-context refusals after warming the cache.
- Fast surface traversal selects polynomial or rational rows once, then bounds
  each active row once before ordered accumulation. This avoids repeated grid
  dispatch and pole lookup without changing work admission or arithmetic.
  Frozen 0.5 point/partial tests cover both grid forms.
- `Cargo.toml`, `Cargo.lock` and `crates/cadmpeg-fuzz/{Cargo.toml,Cargo.lock}`:
  move ZIP readers and fixture writers to ZIP 9.0.5. Keep workspace deflate
  and Zstandard support; the fuzz workspace retains its deflate-only feature
  selection. No decoder or measurement arithmetic changes.
- `crates/cadmpeg-container/src/archive.rs` and
  `crates/cadmpeg-protein/src/lib.rs`,
  `crates/cadmpeg-codec-f3d/src/integration_tests/optional_payloads.rs`: handle
  ZIP 9's fallible decoded-name APIs. Invalid indexed names remain errors.
- `crates/cadmpeg-codec-f3d/src/{materials.rs,writer/patch/mod.rs}`:
  propagate decoded-name failures when rewriting ZIP members.
- ZIP fixture readers in `crates/cadmpeg-codec-f3d/src/{decode/tests/{container,
  header_recovery}.rs,test_support/assembly_test.rs}`,
  `crates/cadmpeg-codec-freecad/src/{container/tests.rs,element_map/tests.rs,
  golden_tests.rs,test_support/test_archive.rs,writer/tests/targets.rs}` and
  `crates/cadmpeg/tests/cli/convert.rs` use ZIP 9's decoded-name results.
- Both lockfiles select patched Zstandard 0.14.3 and remove the ZIP 8
  dependency on Zstandard 0.13.
