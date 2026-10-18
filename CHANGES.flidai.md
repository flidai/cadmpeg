# flidai measurement branch

Based on upstream `v0.6.1`. Modified files retain the upstream Apache-2.0 license.

- `crates/cadmpeg-asm/src/brep/mod.rs` and its tests: port `DecodePurpose::Measure`
  from `fd20d2f804b3a5c5cd60c1d643316aadd3954657`. Retain Model geometry,
  topology, body classification and edge-range clamping; omit report-only data.
- Upstream 0.6.1 already has degree-15 inline basis storage and admitted heap
  fallback, superseding the storage change in
  `b5006de6e0e4974d42e949a09a12d541d3c55ae1`.
- `crates/cadmpeg-ir/src/eval.rs`, `eval/{basis,decode,rational}.rs` and their
