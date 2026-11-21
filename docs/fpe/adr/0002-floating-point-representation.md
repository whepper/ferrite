# ADR-0002 — Floating-point representation (f64)

Status: Proposed. Date: 2026-11-16.

## Context

FPE must replace Q31 fixed-point value arithmetic with floating point. The
choice of precision affects fidelity, stability, determinism, and performance.
The assignment requires analysis, not an automatic f32/f64 pick. Ferrite's
spectrum is already f64; Ld outputs are f64; block switching and MDCT are f32.

## Decision

**f64 for all FPE value arithmetic** (energy sums, thresholds, spreading, PCM
floor, pre-echo, MS energies, Ld). f32 remains only where already used (block
switching, MDCT) — FPE does not touch those. **No mixed precision inside FPE
kernels.** Boundary quantization is round-to-nearest f64→i32.

Rationale (full analysis `05` §2):
- f64's 53-bit mantissa ≫ Q31's 32-bit precision, so float rounding (~2^-53)
  is far below C's truncation noise (~2^-32 per square). FPE's deviation from C
  is then dominated by C's own truncation — the cleanest fidelity position.
- f32's 24-bit mantissa would round energy accumulation **coarser** than C,
  introducing new noise that could flip decisions the Q31 engine keeps exact.
- f64 avoids denormal/FTZ concerns (realistic signals never reach ~1e-308);
  f32 denormals (~1e-38) are reachable by quiet scaled bands.
- f64 matches the existing f64 spectrum/Ld lanes (no narrow/re-widen rounding).
- Determinism: plain `*`/`+` (no FMA contraction), fixed order; ARM64 reference
  target. **No correct-rounding / 1-ULP claim for `log2`** — its accuracy is
  implementation-defined and **measured** (`05` §2); the Ld acceptance bound is
  empirical (ARM64 + x86-64), not assumed.
- Internal domain: the **Q31-fraction domain** (`v_f = v_st/2^31`), derived from
  source (`05` §1); this eliminates Q-format scaffolding and matches the
  existing f64 Ld lane and `SfbConfig` f64 accessors.

## Consequences

- FPE kernels are scalar f64 in Stage 1 (no SIMD; psy ≈2.4% self time, so the
  narrower f64 SIMD width is immaterial). Stage 2 may vectorize.
- **Physical-spectrum precision (FPE-3.1R1).** The precision advantage of f64
  applies to the **physical spectrum information** and the subsequent f64
  arithmetic, not merely to the accumulation operation. Q31 maps the spectrum
  onto a 31-bit grid before it squares, discarding sub-LSB information; FPE
  does not. Q31↔FPE energy differences therefore include a Q31 input-mapping
  term in addition to truncation and boundary rounding (`05` §9). This is an
  intended consequence of the representation choice, not an FPE defect.
- A measured f32-vs-f64 error experiment (A-4) remains outstanding (FPE-3.3);
  f32, even if competitive, remains a Stage-2 optimization.
- Cross-arch `log2` differences covered by the **measured** Ld bound (`05` §2),
  not by a correct-rounding assumption.

## Alternatives rejected

- **f32 throughout**: coarser-than-C accumulation noise; denormal risk; extra
  narrowing of the f64 spectrum.
- **Mixed f32 accumulators + f64 correction**: complexity without benefit;
  the analysis shows f64 alone suffices.
- **f64 everywhere including block switch/MDCT**: would change validated f32
  front-end behaviour (out of scope; non-goal).