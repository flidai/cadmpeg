# flidai converter branch `flidai/ipt-step-r4`

Based on upstream `v0.6.7`. Modified files retain the upstream Apache-2.0 license. Each change is
a fix proposed upstream from a `fix/*` branch of github.com/flidai/cadmpeg; drop it once the
upstream pull request is in a release.

- `crates/cadmpeg-codec-step` writer, `docs/formats/step.md`: write a torus whose tube radius
  exceeds its major radius as a revolved tube (`fix/self-crossing-torus-revolution`).
- `crates/cadmpeg-codec-step` writer: bound the face of a closed sphere or torus
  (`fix/closed-surface-bounds`).
- `crates/cadmpeg-ir`: reverse spring curve contexts with the curve (`fix/spring-support-reversal`).
- `crates/cadmpeg-asm`, `crates/cadmpeg-ir`, `docs/formats/asm.md`: keep the construction a
  referenced intcurve support names, widen the support check by its fit tolerance, and negate the
  revision-form offset when its first boolean is set (`fix/support-procedural-surface`).
- `crates/cadmpeg-asm`, `docs/formats/asm.md`: keep variable-blend caches when a radius law omits
  calibration (`fix/variable-blend-uncalibrated-radius`, `163ff27e`).
- `crates/cadmpeg-asm`, `docs/formats/asm.md`: keep the parameter of the base curve of a
  surface-offset curve: its intcurve sense and its analytic ellipse form
  (`fix/offset-curve-base-sense`).
- `crates/cadmpeg-codec-step` writer, `docs/formats/step.md`: state the opposite face `same_sense`
  for a face on a sphere with a negative radius or on a torus with a negative tube radius, whose
  `SPHERICAL_SURFACE` and `TOROIDAL_SURFACE` records hold the magnitude (`fix/signed-radius-face-sense`).
- `crates/cadmpeg-ir`: let a coedge use curve span one period of a periodic NURBS from any start
  (`fix/periodic-use-curve-range`).
- `crates/cadmpeg-asm`, `crates/cadmpeg-ir`, `docs/formats/asm.md`: keep the cache of an intcurve
  support whose named construction is not read as a surface of unknown fit, and test no support
  side against it (`fix/undecoded-support-cache`).
- `crates/cadmpeg-ir`: place a finding on a record of a native namespace the document carries, so a
  malformed source record of another format does not refuse an export (`fix/native-finding-scope`).

## Round 5, fork-only (not proposed upstream)

These select by a measured quantity, which the upstream source policy forbids for decoders and
encoders. They stay on the fork.

- `crates/cadmpeg-measure`, `crates/cadmpeg-codec-step` writer: name the exterior shell of a
  region by the sign of the volume its oriented faces enclose (`c/r5-outer-shell`). A lump chains
  its shells in an order that carries no role. The writer takes the shell with the largest
  positive signed volume as the outer shell when the first shell does not enclose a positive
  volume, and writes a later shell that encloses a positive volume without reversing its base
  faces.
- `crates/cadmpeg-codec-step` writer: state the face sense of a face on the outer sheet
  (`rho = R + r cos v > 0`) of a torus wider than its major radius, with a negative tube radius,
  against the carrier sense. The sheet is read from the torus equation at the face's vertices
  (`c/r5-apple-sheet`). Faces on the inner sheet are left alone: the rule was right on some
  reference solids and wrong on others.
