// SPDX-License-Identifier: FDK-AAC
//
// Part of Ferrite, a Third-Party Modified Version of the
// Fraunhofer FDK AAC Codec Library for Android.
// See rust/NOTICE and rust/LICENSE-FDK-AAC for the complete license text,
// modification notice, and date.
//! Ferrite FPE A-5 / N-2 cross-architecture `log2`/Ld measurement harness.
//!
//! Test-only. No production code is modified by this module: every production
//! kernel is called read-only with caller-owned scratch, and every boundary
//! the harness probes is an exact citation of an existing source expression.
//! No tolerance is invented here; the existing stored/energy contracts are
//! reused for reporting context only, and no cross-architecture acceptance
//! rule is encoded (recommendations belong to the owner, not to asserts).
//!
//! # The three layers
//!
//! - **Layer A** (`layer_a_*`): identical-input `f64::log2` comparison. A
//!   fixed bit-pattern corpus (`LAYER_A_INPUTS`, `u64` literals so both
//!   architectures evaluate bitwise-identical inputs) is fed through the
//!   exact production expressions (`x.log2()`, `x.log2()/64.0`,
//!   [`sfb::ld_data`]). Rows join on `(call_site, input_bits)`.
//! - **Layer B** (`layer_b_*`): end-to-end corpus capture. The shared
//!   front-end (`BlockSwitch` + `ChannelTransform`, identical to the
//!   differential harness in `super`) produces true MDCT spectra for real
//!   fixtures; the production Q31 and FPE energy/Ld kernels are then run
//!   read-only and every kernel input/output is recorded per
//!   frame/channel/window/band, **including** the TNS-long recompute,
//!   MS-Ld, and grouped short-block production results (see `LayerBRow`
//!   provenance notes). Rows join on the stable observation key.
//! - **Layer C** (`layer_c_*`): deterministic boundary probes around the
//!   exact source-defined expressions (TNS guard, `shiftBits` loop,
//!   `max_nrg_for_shift` selection, sentinels, `q31_of_ld`, threshold
//!   floor). Synthetic inputs only, always flagged `synthetic=true` and
//!   never mixed with corpus rows. Rows join on `probe_id` + `input`
//!   (the human-readable input description — NOT `input_bits` alone, which
//!   recurs across probes by design).
//!
//! # Measured environments (scope of every architecture claim)
//!
//! All cross-architecture conclusions from this harness are scoped to
//! exactly the two executed environments: **native macOS ARM64 on Apple
//! Silicon** and **macOS x86-64 under Rosetta 2 on the same host**. They do
//! NOT establish native x86-64 hardware behaviour, Linux/glibc behaviour,
//! or general x86-64 cross-platform equivalence; bare-metal or Linux
//! measurement is a possible follow-up, not claimed here.
//!
//! Subnormal inputs produced finite non-zero outputs on both tested
//! environments, which supports (but does not prove by register inspection)
//! that flush-to-zero did not alter those tested operations. This is NOT
//! presented as a recording of MXCSR or FPCR.
//!
//! # Cross-architecture workflow (x86-64 second run)
//!
//! 1. On each machine, from `rust/`: `cargo test --lib xarch_log2` (or the
//!    narrower `cargo test --lib xarch_log2_layer_a` etc.). Captures are
//!    written to `rust/target/xarch-<layer>-<arch>.jsonl` (override with
//!    `FPE_XARCH_OUT_DIR`). The first line of each file is the `env`
//!    record; every following line is one observation row.
//! 2. Copy the ARM64 and x86-64 files side by side and join on the stable
//!    key (`call_site` + `input_bits` for A; the observation key for B;
//!    `probe_id` + `input` description for C). The in-test comparators
//!    (`compare_layer_a`, `classify_b_pair` + `b_field_diffs`) implement
//!    exactly that join; they are unit-tested with self-comparison and
//!    synthetic-perturbation cases so a real second-arch file can be
//!    compared without new code.
//! 3. Do NOT simulate x86-64 results. If only one architecture has run,
//!    the cross-architecture comparison is pending (see report §10/§12).
//!
//! # What this harness does NOT do
//!
//! - No `f32` arithmetic, no fast-math, no FMA change, no SIMD.
//! - No new tolerance, no `log2` ULP limit, no correct-rounding claim.
//! - `q31_of_ld` (`stream_driver.rs`, private) is mirrored in-test from its
//!   cited source expression and labelled `MIRROR`; the mirror is a
//!   diagnostic, never a production replacement. Short-block grouped
//!   energies/Ld are captured through the actual production
//!   `psy::finish_channel` (read-only); grouped threshold Ld uses
//!   init-only thresholds and is labelled a simplified-threshold
//!   diagnostic (see `LayerBRow`).
//! - Short-window Ld / threshold Ld / grouped MS-Ld integer paths
//!   (`calc_ld_data`) and the short-TNS no-Ld-update branch are exercised
//!   as negative controls and never modified.

use crate::aac_enc::block_switch::{BlockSwitch, WindowSequence};
use crate::aac_enc::energy as q31_energy;
use crate::aac_enc::fpe::energy as fpe_energy;
use crate::aac_enc::fpe::output as fpe_output;
use crate::aac_enc::psy::{self, lowpass_and_zero_check, PsyTables, MDCT_SCALE_LONG, MDCT_SCALE_SHORT};
use crate::aac_enc::q31::{calc_ld_data, shift_int};
use crate::aac_enc::sfb;
use crate::aac_enc::testdata;
use crate::aac_enc::thresholds::{init_threshold, PreEchoState};
use crate::aac_enc::transform::ChannelTransform;
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Environment metadata
// ---------------------------------------------------------------------------

/// Minimal architecture/execution metadata recorded with every capture.
/// Fields that cannot be observed from inside the test (notably the exact
/// `rustc -Vv` string, MXCSR/FPCR contents, libc/libm versions) are left to
/// the report: the harness records everything it *can* observe and names
/// what it cannot (see `ENV_LIMITATIONS`).
#[derive(Debug, Clone)]
struct EnvMeta {
    arch: &'static str,
    os: &'static str,
    family: &'static str,
    pointer_width: u32,
    endian: &'static str,
    pkg_version: &'static str,
    rustflags: String,
    target_var: String,
    trial_tmp: String,
}

fn collect_env() -> EnvMeta {
    EnvMeta {
        arch: std::env::consts::ARCH,
        os: std::env::consts::OS,
        family: std::env::consts::FAMILY,
        pointer_width: (usize::BITS),
        endian: if cfg!(target_endian = "little") {
            "little"
        } else {
            "big"
        },
        pkg_version: env!("CARGO_PKG_VERSION"),
        rustflags: std::env::var("RUSTFLAGS").unwrap_or_else(|_| "<unset>".to_string()),
        target_var: std::env::var("TARGET").unwrap_or_else(|_| "<unset-in-test>".to_string()),
        trial_tmp: std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| "<unknown>".to_string()),
    }
}

/// What this harness revision explicitly does NOT observe (must appear in
/// the report alongside the recorded fields above).
const ENV_LIMITATIONS: &str = "not directly observed by this harness revision: rustc -Vv string (record via shell, see report); x86 MXCSR incl. FTZ/DAZ and ARM64 FPCR flush-to-zero (no inline-asm probe in this revision); libc/libm versions; build flags beyond RUSTFLAGS/CARGO_* env passthrough";

fn fnv1a64_hex(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Corpus identity pin for the Layer-B fixtures (FNV-1a over the raw i16
/// bytes; no new dependency). Not a security hash: a change-detection pin.
fn fixture_pin(name: &str) -> String {
    let pcm = testdata::fixture_samples(name);
    let mut h: u64 = 0xcbf29ce484222325;
    for ch in &pcm {
        for &s in ch {
            for &b in &s.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    }
    format!("{name}:ch={}:sr={}:fnv1a64={h:016x}", pcm.len(), testdata::fixture_sample_rate(name))
}

// ---------------------------------------------------------------------------
// Small helpers: bits, JSON, nextafter-equivalents
// ---------------------------------------------------------------------------

fn f64_to_json_num(v: f64) -> String {
    if v.is_nan() {
        "\"NaN\"".to_string()
    } else if v == f64::INFINITY {
        "\"+inf\"".to_string()
    } else if v == f64::NEG_INFINITY {
        "\"-inf\"".to_string()
    } else {
        format!("{v:.17e}")
    }
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// `nextUp` by bit manipulation (portable, no libm dependency so probe
/// generation itself cannot inherit the library variation under test).
/// NaN passes through; +inf stays +inf.
fn next_up_bits(x: f64) -> f64 {
    if x.is_nan() || x == f64::INFINITY {
        return x;
    }
    if x == 0.0 {
        // Both +0.0 and -0.0 go to the smallest positive subnormal.
        return f64::from_bits(1);
    }
    let b = x.to_bits();
    if x > 0.0 {
        f64::from_bits(b + 1)
    } else {
        f64::from_bits(b - 1)
    }
}

/// `nextDown` by bit manipulation. NaN passes through; -inf stays -inf.
fn next_down_bits(x: f64) -> f64 {
    if x.is_nan() || x == f64::NEG_INFINITY {
        return x;
    }
    if x == 0.0 {
        return f64::from_bits(0x8000_0000_0000_0001);
    }
    let b = x.to_bits();
    if x > 0.0 {
        f64::from_bits(b - 1)
    } else {
        f64::from_bits(b + 1)
    }
}

fn capture_path(layer: &str) -> std::path::PathBuf {
    let dir = std::env::var("FPE_XARCH_OUT_DIR").unwrap_or_else(|_| {
        format!("{}/target", env!("CARGO_MANIFEST_DIR"))
    });
    let arch = std::env::consts::ARCH;
    std::path::PathBuf::from(dir).join(format!("xarch-{layer}-{arch}.jsonl"))
}

fn write_lines(path: &std::path::PathBuf, lines: &[String]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("capture dir creatable");
    }
    let mut text = String::new();
    for l in lines {
        text.push_str(l);
        text.push('\n');
    }
    std::fs::write(path, text).expect("capture file writable");
}

// ---------------------------------------------------------------------------
// ULP distance (diagnostic only — never an acceptance criterion)
// ---------------------------------------------------------------------------

/// Classification of a pairwise float comparison. NaN is its own class and
/// must be excluded from normal statistics (callers enforce this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UlpClass {
    /// Bit-identical (includes +0 vs +0, -0 vs -0, +inf vs +inf).
    Exact,
    /// Numerically equal but different bits: only reachable for +0.0 vs
    /// -0.0 among non-NaNs.
    SignedZero,
    /// Both finite, different: ULP distance as ordered-integers.
    FiniteUlp(u64),
    /// +inf vs -inf.
    InfSign,
    /// One side infinite, the other finite (includes +inf vs finite and
    /// -inf vs finite; +0/-0 are finite here).
    FiniteVsInfinite,
    /// Either side NaN (payloads ignored; NaNs never get an ULP number).
    Nan,
}

fn classify_pair(a: f64, b: f64) -> UlpClass {
    if a.is_nan() || b.is_nan() {
        return UlpClass::Nan;
    }
    if a.to_bits() == b.to_bits() {
        return UlpClass::Exact;
    }
    if a == b {
        // Non-NaN, numerically equal, bitwise different: only ±0.
        debug_assert!(a == 0.0 && b == 0.0);
        return UlpClass::SignedZero;
    }
    let ai = a.is_infinite();
    let bi = b.is_infinite();
    if ai && bi {
        return UlpClass::InfSign; // bits differ => opposite signs
    }
    if ai || bi {
        return UlpClass::FiniteVsInfinite;
    }
    // Both finite and different.
    UlpClass::FiniteUlp(ordered_ulp_distance(a, b))
}

/// Ordered-integer ULP distance for two distinct finite non-NaN values.
fn ordered_ulp_distance(a: f64, b: f64) -> u64 {
    debug_assert!(!a.is_nan() && !b.is_nan() && !a.is_infinite() && !b.is_infinite());
    fn ordered(bits: u64) -> i64 {
        // Map sign-magnitude to lexicographically ordered two's complement:
        // negatives bit-flip, non-negatives xor the sign bit.
        if bits & 0x8000_0000_0000_0000 != 0 {
            !(bits as i64)
        } else {
            (bits ^ 0x8000_0000_0000_0000) as i64
        }
    }
    ordered(a.to_bits()).wrapping_sub(ordered(b.to_bits())).unsigned_abs()
}

// ---------------------------------------------------------------------------
// Existing numerical contracts (reporting context only)
// ---------------------------------------------------------------------------

/// Stored Q31 integer tolerance (`docs/fpe/05` §9.3, `07` §5): never
/// applied to floating-point Ld values (callers enforce the type split).
fn q31_tol(want: i32) -> i64 {
    32 + (want as i64).abs() / 500
}

/// Energy-domain tolerance: `32 + E/500` (float inversion of stored ints).
fn energy_tol(e_ref: f64) -> f64 {
    32.0 + e_ref.abs() / 500.0
}

/// `ld_to_e` inversion used by the existing Ld comparisons (`psy.rs`
/// tests): sentinel `-1.0` (and below) inverts to `0.0`, else
/// `2^(Ld*64+31)`.
fn ld_to_e(ld: f64) -> f64 {
    if ld <= -1.0 {
        0.0
    } else {
        2f64.powf(ld * 64.0 + 31.0)
    }
}

// ---------------------------------------------------------------------------
// Layer A — identical-input log2
// ---------------------------------------------------------------------------

/// Stable call-site identifiers and the exact source expression each row's
/// `log2_raw`/`log2_scaled`/`ld` columns correspond to. All eight reduce to
/// the same primitive (`x.log2()`, optionally `/64.0`, optionally with the
/// `-1.0` sentinel and a `scale0` offset); the per-site offsets live in
/// Layers B/C, while Layer A isolates the primitive itself.
const LAYER_A_CALL_SITES: &[(&str, &str)] = &[
    ("CS-ld_data", "rust/src/aac_enc/sfb.rs:184 `value.log2()/64.0`, `<=0 -> -1.0`"),
    ("CS-q31-log2frac", "rust/src/aac_enc/energy.rs:415 `(e as f64/2^31).log2()/64.0`"),
    ("CS-q31-rescale-ld", "rust/src/aac_enc/energy.rs:199 `log2/64 - scale0*2/64`"),
    ("CS-q31-tns-long-base", "rust/src/aac_enc/energy.rs:283 `log2frac(pre[b])`"),
    ("CS-q31-grouped-ld", "rust/src/aac_enc/psy.rs:1539 `(v as f64/2^31).log2()/64.0`"),
    ("CS-fpe-rescale-base", "rust/src/aac_enc/fpe/energy.rs:147 `e_raw_f.log2()/64 - scale0*2/64`"),
    ("CS-fpe-tns-long-base", "rust/src/aac_enc/fpe/energy.rs:259 `e_raw_f.log2()/64.0`"),
    ("CS-q31-ms-ld", "rust/src/aac_enc/energy.rs:365 `(v as f64/2^31).log2()/64.0`"),
];

/// Fixed identical-input corpus: `(label, bits)`. Bit literals so both
/// architectures evaluate bitwise-identical inputs; the join key is the
/// bits, never the printed value.
fn layer_a_inputs() -> Vec<(&'static str, u64)> {
    let mut v: Vec<(&str, u64)> = vec![
        ("pos_zero", 0x0000_0000_0000_0000),
        ("neg_zero", 0x8000_0000_0000_0000),
        ("one", 0x3FF0_0000_0000_0000),
        ("neg_one", 0xBFF0_0000_0000_0000),
        ("two", 0x4000_0000_0000_0000),
        ("half", 0x3FE0_0000_0000_0000),
        ("three", 0x4008_0000_0000_0000),
        ("e_5div256", 0x3F94_0000_0000_0000), // 5/256 exact
        ("e_25div256", 0x3FB9_0000_0000_0000), // 25/256 exact
        ("e_three_halves", 0x3FF8_0000_0000_0000), // 1.5
        ("min_subnormal", 0x0000_0000_0000_0001),
        ("neg_min_subnormal", 0x8000_0000_0000_0001),
        ("max_subnormal", 0x000F_FFFF_FFFF_FFFF),
        ("min_normal", 0x0010_0000_0000_0000),
        ("max_finite", 0x7FEF_FFFF_FFFF_FFFF),
        ("min_finite_neg", 0xFFEF_FFFF_FFFF_FFFF),
        ("pos_inf", 0x7FF0_0000_0000_0000),
        ("neg_inf", 0xFFF0_0000_0000_0000),
        ("qnan", 0x7FF8_0000_0000_0000),
        ("qnan_payload", 0x7FF8_0000_0000_1234),
        ("neg_qnan", 0xFFF8_0000_0000_0000),
        ("snan_bits", 0x7FF0_0000_0000_0001),
        ("one_next_up", 0x3FF0_0000_0000_0001),
        ("one_next_down", 0x3FEF_FFFF_FFFF_FFFF),
        ("tiny_denorm_mid", 0x0000_0000_0001_0000),
        ("large_pow2_2p40", 0x4270_0000_0000_0000), // 2^40
        ("small_pow2_2m30", 0x3C1E_0000_0000_0000), // ~2^-30
        ("frac_0p1", 0x3FB9_9999_9999_999A),
        ("frac_pi", 0x4009_21FB_5444_2D18),
        ("sentinel_boundary_above", 0x3F70_0000_0000_0000), // 2^-8-ish
    ];
    // Systematic powers of two across the normal exponent range (sampled:
    // every 97th exponent from 2, plus extremes) to cover exponent-path
    // variation. e=1 is skipped: it would duplicate `min_normal` exactly.
    for (i, e) in (2u32..=2046).step_by(97).enumerate() {
        let bits = (e as u64) << 52;
        // Use a static label scheme without allocation: reuse a small set.
        let label: &'static str = match i % 8 {
            0 => "pow2_e_sample0",
            1 => "pow2_e_sample1",
            2 => "pow2_e_sample2",
            3 => "pow2_e_sample3",
            4 => "pow2_e_sample4",
            5 => "pow2_e_sample5",
            6 => "pow2_e_sample6",
            _ => "pow2_e_sample7",
        };
        let _ = label;
        v.push((label_for_pow2_sample(i), bits));
        let _ = i;
    }
    v
}

fn label_for_pow2_sample(i: usize) -> &'static str {
    match i % 8 {
        0 => "pow2_e_sample0",
        1 => "pow2_e_sample1",
        2 => "pow2_e_sample2",
        3 => "pow2_e_sample3",
        4 => "pow2_e_sample4",
        5 => "pow2_e_sample5",
        6 => "pow2_e_sample6",
        _ => "pow2_e_sample7",
    }
}

/// One Layer-A observation. `input_bits` + `call_site` is the stable join
/// key; `log2_*` is the raw platform primitive, `ld_*` the production
/// `ld_data` path (sentinel included). `domain` classifies production
/// reachability (F-7): production `log2` inputs are band energies, i.e.
/// sums of squares scaled by exact powers of two — always `>= 0`. Negative
/// finite inputs are therefore production-unreachable (kept as
/// exceptional-value probes); NaN/infinite classes are likewise separated.
#[derive(Debug, Clone)]
struct LayerARow {
    call_site: &'static str,
    input_label: &'static str,
    input_bits: u64,
    domain: &'static str,
    log2_bits: u64,
    ld_bits: u64,
}

/// Production-reachability class of a Layer-A input bit pattern.
fn classify_input_domain(bits: u64) -> &'static str {
    let x = f64::from_bits(bits);
    if x.is_nan() {
        "nan-input"
    } else if x.is_infinite() {
        "infinite"
    } else if x < 0.0 {
        // Includes negative finite AND -0.0 is NOT < 0.0, so -0.0 falls
        // through to zero-sentinel. Negative finite energies cannot arise
        // from sums of squares: unreachable in production.
        "negative-finite-unreachable"
    } else if x == 0.0 {
        "zero-sentinel-path"
    } else {
        "positive-finite"
    }
}

impl LayerARow {
    fn input_value(&self) -> f64 {
        f64::from_bits(self.input_bits)
    }
    fn log2_value(&self) -> f64 {
        f64::from_bits(self.log2_bits)
    }
    fn ld_value(&self) -> f64 {
        f64::from_bits(self.ld_bits)
    }
}

/// Evaluate the production expressions for one input. This is the platform
/// `log2` under test — no reimplementation, no transformation of inputs.
fn layer_a_eval(input_bits: u64) -> (u64, u64) {
    let x = f64::from_bits(input_bits);
    let log2_raw = x.log2();
    let ld = sfb::ld_data(x);
    (log2_raw.to_bits(), ld.to_bits())
}

fn layer_a_capture() -> Vec<LayerARow> {
    let mut rows = Vec::new();
    for (site, _) in LAYER_A_CALL_SITES {
        for (label, bits) in layer_a_inputs() {
            let (log2_bits, ld_bits) = layer_a_eval(bits);
            rows.push(LayerARow {
                call_site: site,
                input_label: label,
                input_bits: bits,
                domain: classify_input_domain(bits),
                log2_bits,
                ld_bits,
            });
        }
    }
    rows
}

/// Summary of a Layer-A self- or cross-architecture comparison. ULP stats
/// are diagnostic; `nan_rows` (NaN inputs) and `nan_output_rows` (finite
/// inputs whose `log2` is NaN on either side, e.g. negative-domain) are
/// counted separately and excluded from the numeric aggregates per the
/// task's NaN rule — they trigger investigation, never acceptance stats.
#[derive(Debug)]
struct LayerAComparison {
    rows: usize,
    exact_log2: usize,
    exact_ld: usize,
    nan_rows: usize,
    nan_output_rows: usize,
    nan_output_keys: Vec<String>,
    max_abs_log2: f64,
    max_abs_ld: f64,
    p50_abs_log2: f64,
    p90_abs_log2: f64,
    max_ulp_log2: u64,
    differing_keys: Vec<String>,
}

fn abs_diff_finite(a: f64, b: f64) -> Option<f64> {
    if a.is_nan() || b.is_nan() || a.is_infinite() || b.is_infinite() {
        None
    } else {
        Some((a - b).abs())
    }
}

fn percentile_sorted(mut xs: Vec<f64>, p: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    let idx = ((p / 100.0) * ((xs.len() - 1) as f64)).round() as usize;
    xs[idx.min(xs.len() - 1)]
}

fn compare_layer_a(a: &[LayerARow], b: &[LayerARow]) -> LayerAComparison {
    let mut map_b: BTreeMap<(String, u64), &LayerARow> = BTreeMap::new();
    for r in b {
        map_b.insert((r.call_site.to_string(), r.input_bits), r);
    }
    let mut exact_log2 = 0usize;
    let mut exact_ld = 0usize;
    let mut nan_rows = 0usize;
    let mut nan_output_rows = 0usize;
    let mut nan_output_keys: Vec<String> = Vec::new();
    let mut abs_log2: Vec<f64> = Vec::new();
    let mut abs_ld: Vec<f64> = Vec::new();
    let mut max_ulp: u64 = 0;
    let mut differing_keys: Vec<String> = Vec::new();
    let mut rows = 0usize;
    for ra in a {
        let key = (ra.call_site.to_string(), ra.input_bits);
        let rb = map_b.get(&key).expect("join partner must exist");
        rows += 1;
        if ra.input_value().is_nan() {
            nan_rows += 1;
            continue;
        }
        // NaN OUTPUT separation (task §1/§3): finite inputs whose log2 is
        // NaN on either side (negative-domain) are counted separately and
        // excluded from the log2 aggregates; Ld (sentinel path) is still
        // compared normally.
        let la_nan = ra.log2_value().is_nan() || rb.log2_value().is_nan();
        if la_nan {
            nan_output_rows += 1;
            nan_output_keys.push(format!("{}:{:#018x}", ra.call_site, ra.input_bits));
        } else {
            if ra.log2_bits == rb.log2_bits {
                exact_log2 += 1;
            } else {
                differing_keys.push(format!("{}:{:#018x}", ra.call_site, ra.input_bits));
            }
            if let Some(d) = abs_diff_finite(ra.log2_value(), rb.log2_value()) {
                abs_log2.push(d);
            }
            if let UlpClass::FiniteUlp(u) = classify_pair(ra.log2_value(), rb.log2_value()) {
                max_ulp = max_ulp.max(u);
            }
        }
        if ra.ld_bits == rb.ld_bits {
            exact_ld += 1;
        } else if !la_nan {
            differing_keys.push(format!("{}:{:#018x}:ld", ra.call_site, ra.input_bits));
        } else {
            // NaN-output rows with differing Ld are still surfaced, tagged.
            differing_keys.push(format!("{}:{:#018x}:ld-nanout", ra.call_site, ra.input_bits));
        }
        if let Some(d) = abs_diff_finite(ra.ld_value(), rb.ld_value()) {
            abs_ld.push(d);
        }
    }
    LayerAComparison {
        rows,
        exact_log2,
        exact_ld,
        nan_rows,
        nan_output_rows,
        nan_output_keys,
        max_abs_log2: abs_log2.iter().cloned().fold(0.0f64, f64::max),
        max_abs_ld: abs_ld.iter().cloned().fold(0.0f64, f64::max),
        p50_abs_log2: percentile_sorted(abs_log2.clone(), 50.0),
        p90_abs_log2: percentile_sorted(abs_log2, 90.0),
        max_ulp_log2: max_ulp,
        differing_keys,
    }
}

fn layer_a_to_jsonl(rows: &[LayerARow], env: &EnvMeta, pins: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "{{\"kind\":\"env\",\"arch\":\"{}\",\"os\":\"{}\",\"family\":\"{}\",\"pointer_width\":{},\"endian\":\"{}\",\"pkg\":\"{}\",\"rustflags\":\"{}\",\"target_var\":\"{}\",\"limitations\":\"{}\",\"pins\":[{}]}}",
        json_escape(env.arch),
        json_escape(env.os),
        json_escape(env.family),
        env.pointer_width,
        env.endian,
        json_escape(env.pkg_version),
        json_escape(&env.rustflags),
        json_escape(&env.target_var),
        json_escape(ENV_LIMITATIONS),
        pins.iter().map(|p| format!("\"{}\"", json_escape(p))).collect::<Vec<_>>().join(","),
    ));
    for r in rows {
        out.push(format!(
            "{{\"kind\":\"row\",\"layer\":\"A\",\"call_site\":\"{}\",\"label\":\"{}\",\"domain\":\"{}\",\"input_bits\":\"{:#018x}\",\"input\":{},\"log2_bits\":\"{:#018x}\",\"log2\":{},\"ld_bits\":\"{:#018x}\",\"ld\":{}}}",
            json_escape(r.call_site),
            json_escape(r.input_label),
            json_escape(r.domain),
            r.input_bits,
            f64_to_json_num(r.input_value()),
            r.log2_bits,
            f64_to_json_num(r.log2_value()),
            r.ld_bits,
            f64_to_json_num(r.ld_value()),
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Layer B — end-to-end corpus capture
// ---------------------------------------------------------------------------

/// Layer-B fixtures: sine/quiet-sine carry real tonal/low-energy content;
/// onset-stereo frames 0-1 are SILENT (the onset transient sits at sample
/// 24000 ≈ frame 23, outside the captured window), so onset contributes
/// sentinel-path coverage, NOT short-transition coverage. Short-window
/// rows in this capture come from sine-stereo frame 1 (block-switch onset
/// at the fixture start). Two frames per fixture keep the narrow suite
/// fast. (F-6: corrected attribution; no corpus expansion.)
const LAYER_B_FIXTURES: &[&str] = &["sine-stereo", "onset-stereo", "quiet-sine-stereo"];
const LAYER_B_FRAMES: u32 = 2;

/// One Layer-B observation: every production value that can turn a `log2`
/// difference into an encoder-visible difference, plus the guards around
/// it. `energy_q31` is the upstream integer input to the log path;
/// `log2_*` the primitive; `ld_q31/ld_fpe` the two Ld lanes;
/// `q31_of_ld_mirror` the documented mirror of the private
/// `stream_driver.rs:220 q31_of_ld` (labelled, never production).
///
/// Provenance of the retained diagnostics (F-1) — every field is one of:
/// - **PROD**: direct output of a production kernel called read-only;
/// - **MIRROR**: in-test evaluation of a cited private production
///   expression (labelled, never production);
/// - **SYNTH**: synthetic diagnostic derived from recorded values (labelled).
#[derive(Debug, Clone)]
struct LayerBRow {
    fixture: &'static str,
    frame: u32,
    channel: usize,
    window: usize,
    band: usize,
    short: bool,
    scale0: i32,
    final_shift: i32,
    shift_bits: i32,
    energy_q31: i32,
    energy_f: f64,
    /// `log2_*_bits` encode the primitive's input domain: when `sentinel`
    /// is true the production kernels take the `-1.0` path and never
    /// evaluate `log2`; the harness records canonical `f64::NAN` bits for
    /// "not evaluated" (distinct from a platform NaN, which Layer B never
    /// produces — see Layer A for true NaN inputs).
    log2_q_bits: u64,
    log2_f_bits: u64,
    ld_q31_bits: u64,
    ld_fpe_bits: u64,
    q31_of_ld_mirror: i32, // MIRROR of private `stream_driver.rs:220`.
    /// SYNTH (F-4): the TNS-long guard predicate
    /// (`energy.rs:284: base >= -0.5 + scaleDiff/2`) evaluated on the
    /// RESCALE-stage base values. `rescale_ld_into` does NOT execute this
    /// guard; these fields only record how close rescale energies sit to
    /// the clamp boundary that WOULD apply in the long recompute. The
    /// actual production guard outcome on long inputs is observable via
    /// `ld_long_bits` (clamped to the sentinel path when the guard fails).
    synth_guard_q: bool,
    synth_guard_f: bool,
    sentinel: bool,
    /// SYNTH: distance of the rescale base to the TNS-long guard threshold
    /// (context only, see `synth_guard_*`).
    dist_to_tns_guard: f64,
    dist_to_sentinel_ld: f64,
    // --- Retained TNS-long diagnostics (F-1; PROD, unfiltered spectrum) ---
    /// PROD: `true` on long frames, where the kernels below ran.
    tns_long_observed: bool,
    /// PROD: `band_energy_long_into` stored energy (`energy.rs:249`).
    /// Pure-integer path; `0` when unobserved.
    e_long_q31: i32,
    /// PROD: `band_energy_long_into` guarded Ld. Canonical NaN bits when
    /// unobserved (never a platform NaN: the kernel returns `-1.0..0.0`).
    ld_long_bits: u64,
    /// PROD: FPE `tns_recompute_energy_into` stored energy
    /// (`fpe/energy.rs:205`). `0` when unobserved.
    e_tns_fpe_stored: i32,
    /// PROD: FPE TNS-long Ld. Canonical NaN bits when unobserved.
    ld_tns_fpe_bits: u64,
    // --- Retained MS diagnostics (F-1; PROD integer lane, long ch0) ---
    /// PROD: `true` on long stereo frames for ch0, where `calc_ms_energies`
    /// (`energy.rs:321`) ran with `calc_ld = true`.
    ms_observed: bool,
    /// PROD: mid/side Ld from `calc_ms_energies`. Canonical NaN bits when
    /// unobserved. (Float FPE MS energies remain outstanding per FPE-7.1.)
    ms_mid_ld_bits: u64,
    ms_side_ld_bits: u64,
    // --- Retained grouped short-block results (F-1) ---
    /// PROD: `true` on short frames, where `psy::finish_channel` ran
    /// read-only on corpus-derived inputs (see `layer_b_capture`).
    grouped_observed: bool,
    /// Index of the group containing `window` (`-1` when unobserved).
    group_idx: i32,
    /// PROD: grouped stored energy for (`group_idx`, `band`) — exact
    /// integer regroup of the post-shift stored energies. `0` when
    /// unobserved.
    grp_e_stored: i32,
    /// PROD: grouped energy Ld for (`group_idx`, `band`)
    /// (`psy.rs:1532` formula on the production regroup). Canonical NaN
    /// bits when unobserved.
    grp_ld_bits: u64,
    /// DIAGNOSTIC: grouped threshold Ld with the `-33/64` floor, computed
    /// from init-only thresholds (not the clipped/spread/floored production
    /// thresholds — see `layer_b_capture`). Canonical NaN bits when
    /// unobserved. The floor formula itself is pinned by C-06 probes.
    grp_thr_ld_bits: u64,
}

impl LayerBRow {
    fn key(&self) -> String {
        format!("{}:f{}:ch{}:w{}:b{}", self.fixture, self.frame, self.channel, self.window, self.band)
    }
    fn log2_q(&self) -> f64 {
        f64::from_bits(self.log2_q_bits)
    }
    fn log2_f(&self) -> f64 {
        f64::from_bits(self.log2_f_bits)
    }
    fn ld_q31(&self) -> f64 {
        f64::from_bits(self.ld_q31_bits)
    }
    fn ld_fpe(&self) -> f64 {
        f64::from_bits(self.ld_fpe_bits)
    }
}

/// Documented mirror of the private `q31_of_ld` (`stream_driver.rs:220`):
/// `if ld <= -1.0 { i32::MIN } else { (ld*2^31).round() as i32 }`.
/// Test-only diagnostic; the production function is unchanged.
fn q31_of_ld_mirror(ld: f64) -> i32 {
    if ld <= -1.0 {
        i32::MIN
    } else {
        (ld * 2_147_483_648.0).round() as i32
    }
}

/// Documented mirror of the private `grouped_ld` (`psy.rs:1532`):
/// `v<=0 -> ld_data(0.0)=-1.0` else `(v as f64/2^31).log2()/64.0`.
fn grouped_ld_mirror(stored: &[i32]) -> Vec<f64> {
    stored
        .iter()
        .map(|&v| {
            if v <= 0 {
                sfb::ld_data(0.0)
            } else {
                (v as f64 / 2_147_483_648.0).log2() / 64.0
            }
        })
        .collect()
}

fn build_tables(sample_rate: u32, channels: usize) -> PsyTables {
    let (ch_br, tns_br) = if channels == 1 { (112000, 112000) } else { (96000, 192000) };
    let ((lcfg, _), (scfg, _)) = testdata::load_cfg(sample_rate, ch_br);
    PsyTables::for_rate(sample_rate, ch_br, tns_br, 19293, lcfg.min_snr_ld, scfg.min_snr_ld)
        .expect("tables build")
}

/// Map a short-window index to its group via the block-switch grouping.
/// Returns `None` for inconsistent layouts (defensive; never hit on the
/// captured corpus — the coverage test asserts `grouped_observed` on all
/// short rows).
fn group_of_window(lens: &[u8; 4], no_groups: usize, w: usize) -> Option<usize> {
    let mut base = 0usize;
    for g in 0..no_groups {
        let n = lens[g] as usize;
        if w >= base && w < base + n {
            return Some(g);
        }
        base += n;
    }
    None
}

/// Run the shared front-end for `frame` and capture rescale + TNS-long +
/// MS + grouped kernel I/O read-only. Spectra come from the true MDCT
/// path. Provenance notes:
/// - TNS-long recompute kernels run on the UNFILTERED-by-TNS spectrum (no
///   detect/encode state is driven): read-only diagnostic invocations of
///   the production kernels on corpus-derived inputs, not the
///   post-filter values. Post-TNS-filtered spectra are unobserved (report
///   limitation).
/// - Grouped short-block values come from the actual production
///   `psy::finish_channel` (read-only) fed with corpus-derived post-shift
///   stored energies (production-exact regroup inputs). Thresholds are
///   init-only (`init_threshold`, not the clipped/spread/floored
///   production thresholds), so grouped threshold Ld is a
///   simplified-threshold diagnostic; grouped energies/Ld are
///   production-faithful.
fn layer_b_capture() -> Vec<LayerBRow> {
    let mut rows = Vec::new();
    for &fixture in LAYER_B_FIXTURES {
        let pcm = testdata::fixture_samples(fixture);
        let sr = testdata::fixture_sample_rate(fixture);
        let channels = pcm.len();
        let tables = build_tables(sr, channels);
        let mut switches: Vec<BlockSwitch> = (0..channels).map(|_| BlockSwitch::new()).collect();
        let mut transforms: Vec<ChannelTransform> = (0..channels).map(|_| ChannelTransform::new()).collect();
        for frame in 0..LAYER_B_FRAMES {
            // Fresh 1024-sample blocks, zero-padded past the fixture end.
            let mut fresh: Vec<[i16; 1024]> = (0..channels)
                .map(|c| {
                    let mut b = [0i16; 1024];
                    for (i, v) in b.iter_mut().enumerate() {
                        let t = frame as usize * 1024 + i;
                        *v = if t < pcm[c].len() { pcm[c][t] } else { 0 };
                    }
                    b
                })
                .collect();
            for (sw, fr) in switches.iter_mut().zip(fresh.iter()) {
                sw.process(fr, false);
            }
            if channels == 2 {
                let (l, r) = switches.split_at_mut(1);
                BlockSwitch::synchronize_pair(&mut l[0], &mut r[0]);
            } else {
                switches[0].synchronize_mono();
            }
            let mut specs: Vec<[f64; 1024]> = Vec::with_capacity(channels);
            let mut seqs: Vec<WindowSequence> = Vec::with_capacity(channels);
            let mut groupings: Vec<(u8, [u8; 4])> = Vec::with_capacity(channels);
            for (ch, (tr, sw)) in transforms.iter_mut().zip(switches.iter()).enumerate() {
                let out = tr.process(&fresh[ch], sw.sequence(), sw.shape()).expect("mdct");
                let mut spec = [0.0f64; 1024];
                for (d, s) in spec.iter_mut().zip(out.iter()) {
                    *d = *s as f64;
                }
                specs.push(spec);
                seqs.push(sw.sequence());
                groupings.push(sw.grouping());
            }
            // Per-channel rescale capture (mirrors `psy.rs` rescale stage
            // order: lowpass, Q31 map, maxScale, energies, maxNrg, shifts,
            // Ld, stored re-shift; float FPE lane alongside).
            for ch in 0..channels {
                let short = seqs[ch] == WindowSequence::Short;
                let conf = if short { &tables.short_cfg } else { &tables.long_cfg };
                let (n_win, wlen, m) = if short {
                    (8usize, 128usize, MDCT_SCALE_SHORT)
                } else {
                    (1usize, 1024usize, MDCT_SCALE_LONG)
                };
                let mut spec = specs[ch];
                lowpass_and_zero_check(&mut spec, n_win, wlen, conf.lowpass_line());
                let offsets = conf.offsets();
                let active = conf.sfb_active();
                // Q31 lane per window.
                let mut q_windows: Vec<Vec<i32>> = Vec::with_capacity(n_win);
                let mut maxsc_windows: Vec<Vec<i32>> = Vec::with_capacity(n_win);
                let mut eraw_windows: Vec<Vec<i32>> = Vec::with_capacity(n_win);
                for w in 0..n_win {
                    let win = &spec[w * wlen..(w + 1) * wlen];
                    let q = q31_energy::map_spectrum_to_q31(win, m);
                    let mut maxsc = Vec::new();
                    q31_energy::max_scale_spec_into(&q, offsets, active, &mut maxsc);
                    let mut eraw = Vec::new();
                    q31_energy::check_band_energy_into(&q, &maxsc, offsets, active, &mut eraw);
                    q_windows.push(q);
                    maxsc_windows.push(maxsc);
                    eraw_windows.push(eraw);
                }
                let min_spec_shift = maxsc_windows
                    .iter()
                    .flat_map(|v| v.iter())
                    .cloned()
                    .min()
                    .unwrap_or(m);
                let mut max_nrg = 0i32;
                for w in 0..n_win {
                    let mn = q31_energy::max_nrg_for_shift(
                        &eraw_windows[w],
                        &maxsc_windows[w],
                        min_spec_shift,
                    );
                    max_nrg = max_nrg.max(mn);
                }
                let (_, final_shift) =
                    q31_energy::rescale_shifts(max_nrg, min_spec_shift, short, m);
                // Ld + FPE lane + TNS-long + MS diagnostics, then grouped.
                // Per-window production outputs are retained in vectors
                // first (F-1: no computed diagnostic is discarded); rows
                // are pushed afterwards with full provenance flags.
                let mut ld_q_windows: Vec<Vec<f64>> = Vec::with_capacity(n_win);
                let mut e_f_windows: Vec<Vec<f64>> = Vec::with_capacity(n_win);
                let mut ld_fpe_windows: Vec<Vec<f64>> = Vec::with_capacity(n_win);
                let mut shift_windows: Vec<i32> = Vec::with_capacity(n_win);
                let mut e_long_windows: Vec<Vec<i32>> = Vec::with_capacity(n_win);
                let mut ld_long_windows: Vec<Vec<f64>> = Vec::with_capacity(n_win);
                let mut e_tns_fpe_windows: Vec<Vec<i32>> = Vec::with_capacity(n_win);
                let mut ld_tns_fpe_windows: Vec<Vec<f64>> = Vec::with_capacity(n_win);
                let mut ms_mid_windows: Vec<Vec<f64>> = Vec::with_capacity(n_win);
                let mut ms_side_windows: Vec<Vec<f64>> = Vec::with_capacity(n_win);
                for w in 0..n_win {
                    let win = &spec[w * wlen..(w + 1) * wlen];
                    let mut ld_q = Vec::new();
                    q31_energy::rescale_ld_into(
                        &eraw_windows[w],
                        &maxsc_windows[w],
                        final_shift,
                        &mut ld_q,
                    );
                    let mut e_f = Vec::new();
                    let mut ld_fpe = Vec::new();
                    fpe_energy::rescale_band_energy_into(
                        win,
                        offsets,
                        active,
                        m,
                        final_shift,
                        &maxsc_windows[w],
                        &mut e_f,
                        &mut ld_fpe,
                    );
                    // TNS-long diagnostic on the unfiltered spectrum
                    // (read-only production invocation; see fn docs).
                    let mut pre = Vec::new();
                    let mut e_long = Vec::new();
                    let mut ld_long = Vec::new();
                    let shift_bits = if short {
                        0
                    } else {
                        q31_energy::band_energy_long_into(
                            &q_windows[w],
                            &maxsc_windows[w],
                            offsets,
                            active,
                            &mut pre,
                            &mut e_long,
                            &mut ld_long,
                        )
                    };
                    if short {
                        e_long = vec![0i32; active];
                        ld_long = vec![f64::NAN; active];
                    }
                    let mut e_tns_fpe = vec![0i32; active];
                    let mut ld_tns_fpe = vec![f64::NAN; active];
                    if !short {
                        fpe_energy::tns_recompute_energy_into(
                            win,
                            offsets,
                            active,
                            m,
                            &maxsc_windows[w],
                            shift_bits,
                            false,
                            &mut e_tns_fpe,
                            &mut ld_tns_fpe,
                        );
                    }
                    // MS diagnostic on long frames (integer lane only;
                    // float FPE MS energies are outstanding per FPE-7.1).
                    // Production `calc_ms_energies` with `calc_ld = true`.
                    let mut mid_ld = vec![f64::NAN; active];
                    let mut side_ld = vec![f64::NAN; active];
                    if !short && channels == 2 && ch == 0 {
                        let qw0 = &q_windows[w];
                        // Right channel Q31 window for the MS pair.
                        let mut spec_r = specs[1];
                        lowpass_and_zero_check(&mut spec_r, n_win, wlen, conf.lowpass_line());
                        let qr =
                            q31_energy::map_spectrum_to_q31(&spec_r[w * wlen..(w + 1) * wlen], m);
                        let mut msc_r = Vec::new();
                        q31_energy::max_scale_spec_into(&qr, offsets, active, &mut msc_r);
                        let (_, _, mld, sld) = q31_energy::calc_ms_energies(
                            qw0,
                            &qr,
                            &maxsc_windows[w],
                            &msc_r,
                            offsets,
                            active,
                            true,
                        );
                        mid_ld = mld;
                        side_ld = sld;
                    }
                    ld_q_windows.push(ld_q);
                    e_f_windows.push(e_f);
                    ld_fpe_windows.push(ld_fpe);
                    shift_windows.push(shift_bits);
                    e_long_windows.push(e_long);
                    ld_long_windows.push(ld_long);
                    e_tns_fpe_windows.push(e_tns_fpe);
                    ld_tns_fpe_windows.push(ld_tns_fpe);
                    ms_mid_windows.push(mid_ld);
                    ms_side_windows.push(side_ld);
                }
                // Grouped short-block production results via read-only
                // `psy::finish_channel` on corpus-derived inputs.
                // `en` = post-shift stored energies (production regroup
                // inputs, exact); `thr` = init-only thresholds, so the
                // grouped THRESHOLD Ld is a simplified-threshold
                // diagnostic (labelled); grouped ENERGIES/Ld are
                // production-faithful (`ms` zeros do not affect them;
                // `tns_shift = 0`: TNS not driven — stated limitation).
                let grouped_out = if short {
                    let mut en_post: Vec<Vec<i32>> = Vec::with_capacity(n_win);
                    let mut thr_init: Vec<Vec<i32>> = Vec::with_capacity(n_win);
                    for w in 0..n_win {
                        let mut stored = Vec::with_capacity(active);
                        for b in 0..active {
                            let scale0 = (maxsc_windows[w][b] - 4).max(0);
                            let scale = ((scale0 - final_shift) << 1).min(31);
                            stored.push(shift_int(eraw_windows[w][b], -scale));
                        }
                        thr_init.push(stored.iter().map(|&e| init_threshold(e)).collect());
                        en_post.push(stored);
                    }
                    let spread_post = en_post.clone();
                    let ms_zeros: Vec<Vec<i32>> = vec![vec![0i32; active]; n_win];
                    let preecho = PreEchoState::new(tables.long_cfg.pcm_quant_q31());
                    let out = psy::finish_channel(
                        &preecho,
                        &thr_init,
                        &en_post,
                        &ld_q_windows,
                        &spread_post,
                        &maxsc_windows,
                        &spec,
                        conf,
                        true,
                        active,
                        m,
                        final_shift,
                        0,
                        groupings[ch],
                        &ms_zeros,
                        &tables.min_snr_short,
                    );
                    out.grouped
                } else {
                    None
                };
                let sfb_cnt = conf.sfb_count();
                let (no_groups, lens) = groupings[ch];
                for w in 0..n_win {
                    let ld_q = &ld_q_windows[w];
                    let e_f = &e_f_windows[w];
                    let ld_fpe = &ld_fpe_windows[w];
                    let shift_bits = shift_windows[w];
                    let e_long = &e_long_windows[w];
                    let ld_long = &ld_long_windows[w];
                    let e_tns_fpe = &e_tns_fpe_windows[w];
                    let ld_tns_fpe = &ld_tns_fpe_windows[w];
                    let mid_ld = &ms_mid_windows[w];
                    let side_ld = &ms_side_windows[w];
                    let tns_long_observed = !short;
                    let ms_observed = !short && channels == 2 && ch == 0;
                    // Grouped production base for this window's group
                    // (short frames only).
                    let (grouped_observed, grp_flat) = match (&grouped_out, short) {
                        (Some(_), true) => match group_of_window(&lens, no_groups as usize, w) {
                            Some(gi) => (true, Some(gi * sfb_cnt)),
                            None => (false, None),
                        },
                        _ => (false, None),
                    };
                    for b in 0..active {
                        let scale0 = (maxsc_windows[w][b] - 4).max(0);
                        let e_raw = eraw_windows[w][b];
                        // Upstream energy input bits as seen by log2.
                        let log2_q = if e_raw <= 0 {
                            f64::NAN
                        } else {
                            (e_raw as f64 / 2_147_483_648.0).log2()
                        };
                        let e_raw_f = e_f[b] * 2f64.powi(2 * (scale0 - final_shift));
                        let log2_f = if e_raw_f <= 0.0 {
                            f64::NAN
                        } else {
                            e_raw_f.log2()
                        };
                        // SYNTH guard diagnostic (F-4): TNS-long predicate on
                        // rescale-stage bases; rescale_ld_into has no guard.
                        let scale_diff = (maxsc_windows[w][b] - 4) as f64 * 2.0 / 64.0;
                        let thresh = -0.5 + scale_diff / 2.0;
                        let base_q = if e_raw <= 0 {
                            -1.0
                        } else {
                            (e_raw as f64 / 2_147_483_648.0).log2() / 64.0
                        };
                        let base_f = if e_raw_f <= 0.0 {
                            -1.0
                        } else {
                            e_raw_f.log2() / 64.0
                        };
                        let sgq = base_q >= thresh;
                        let sgf = base_f >= thresh;
                        let sentinel = e_raw <= 0;
                        let (grp_g, grp_e, grp_ld, grp_thr) = match (grouped_observed, grp_flat) {
                            (true, Some(flat)) => {
                                let g = grouped_out.as_ref().expect("grouped present");
                                (
                                    (flat / sfb_cnt) as i32,
                                    g.energies[flat + b],
                                    g.energy_ld[flat + b],
                                    g.threshold_ld[flat + b],
                                )
                            }
                            _ => (-1, 0, f64::NAN, f64::NAN),
                        };
                        rows.push(LayerBRow {
                            fixture,
                            frame,
                            channel: ch,
                            window: w,
                            band: b,
                            short,
                            scale0,
                            final_shift,
                            shift_bits,
                            energy_q31: e_raw,
                            energy_f: e_f[b],
                            log2_q_bits: log2_q.to_bits(),
                            log2_f_bits: log2_f.to_bits(),
                            ld_q31_bits: ld_q[b].to_bits(),
                            ld_fpe_bits: ld_fpe[b].to_bits(),
                            q31_of_ld_mirror: q31_of_ld_mirror(ld_fpe[b]),
                            synth_guard_q: sgq,
                            synth_guard_f: sgf,
                            sentinel,
                            dist_to_tns_guard: (base_f - thresh).abs(),
                            dist_to_sentinel_ld: (ld_fpe[b] - (-1.0)).abs(),
                            tns_long_observed,
                            e_long_q31: e_long[b],
                            ld_long_bits: ld_long[b].to_bits(),
                            e_tns_fpe_stored: e_tns_fpe[b],
                            ld_tns_fpe_bits: ld_tns_fpe[b].to_bits(),
                            ms_observed,
                            ms_mid_ld_bits: mid_ld[b].to_bits(),
                            ms_side_ld_bits: side_ld[b].to_bits(),
                            grouped_observed,
                            group_idx: grp_g,
                            grp_e_stored: grp_e,
                            grp_ld_bits: grp_ld.to_bits(),
                            grp_thr_ld_bits: grp_thr.to_bits(),
                        });
                    }
                }
            }
            let _ = &mut fresh;
        }
    }
    rows
}

/// Classification of a Layer-B pair (same key, two architectures).
/// The order matters: an upstream energy difference masks everything
/// downstream and must never be reported as pure-`log2` variation.
/// `ExtendedDiagnosticDiff` covers the retained F-1 diagnostics (TNS-long,
/// MS, grouped) and the integer representation lane: with identical
/// upstream inputs these are expected identical, so any difference is
/// investigated, never normalized away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BPairClass {
    UpstreamInputDiff,
    PureLog2Diff,
    LdGuardDiff,
    IntConversionDiff,
    ExtendedDiagnosticDiff,
    Identical,
}

fn classify_b_pair(a: &LayerBRow, b: &LayerBRow) -> BPairClass {
    assert_eq!(a.key(), b.key(), "join key must match");
    if a.energy_q31 != b.energy_q31
        || a.energy_f.to_bits() != b.energy_f.to_bits()
    {
        return BPairClass::UpstreamInputDiff;
    }
    if a.log2_q_bits != b.log2_q_bits || a.log2_f_bits != b.log2_f_bits {
        return BPairClass::PureLog2Diff;
    }
    if a.ld_q31_bits != b.ld_q31_bits
        || a.ld_fpe_bits != b.ld_fpe_bits
        || a.synth_guard_q != b.synth_guard_q
        || a.synth_guard_f != b.synth_guard_f
    {
        return BPairClass::LdGuardDiff;
    }
    if a.q31_of_ld_mirror != b.q31_of_ld_mirror {
        return BPairClass::IntConversionDiff;
    }
    if a.scale0 != b.scale0
        || a.final_shift != b.final_shift
        || a.shift_bits != b.shift_bits
        || a.tns_long_observed != b.tns_long_observed
        || a.e_long_q31 != b.e_long_q31
        || a.ld_long_bits != b.ld_long_bits
        || a.e_tns_fpe_stored != b.e_tns_fpe_stored
        || a.ld_tns_fpe_bits != b.ld_tns_fpe_bits
        || a.ms_observed != b.ms_observed
        || a.ms_mid_ld_bits != b.ms_mid_ld_bits
        || a.ms_side_ld_bits != b.ms_side_ld_bits
        || a.grouped_observed != b.grouped_observed
        || a.group_idx != b.group_idx
        || a.grp_e_stored != b.grp_e_stored
        || a.grp_ld_bits != b.grp_ld_bits
        || a.grp_thr_ld_bits != b.grp_thr_ld_bits
    {
        return BPairClass::ExtendedDiagnosticDiff;
    }
    BPairClass::Identical
}

/// Every recorded evidence field that differs between two same-key rows,
/// regardless of the primary classification (F-1 §5). The primary class
/// still reports the FIRST causal difference in pipeline order, but this
/// list keeps raw downstream differences visible even when an upstream
/// difference determines the primary class.
fn b_field_diffs(a: &LayerBRow, b: &LayerBRow) -> Vec<&'static str> {
    assert_eq!(a.key(), b.key(), "join key must match");
    let mut out = Vec::new();
    if a.energy_q31 != b.energy_q31 { out.push("energy_q31"); }
    if a.energy_f.to_bits() != b.energy_f.to_bits() { out.push("energy_f"); }
    if a.scale0 != b.scale0 { out.push("scale0"); }
    if a.final_shift != b.final_shift { out.push("final_shift"); }
    if a.shift_bits != b.shift_bits { out.push("shift_bits"); }
    if a.log2_q_bits != b.log2_q_bits { out.push("log2_q"); }
    if a.log2_f_bits != b.log2_f_bits { out.push("log2_f"); }
    if a.ld_q31_bits != b.ld_q31_bits { out.push("ld_q31"); }
    if a.ld_fpe_bits != b.ld_fpe_bits { out.push("ld_fpe"); }
    if a.q31_of_ld_mirror != b.q31_of_ld_mirror { out.push("q31mirror"); }
    if a.synth_guard_q != b.synth_guard_q { out.push("synth_guard_q"); }
    if a.synth_guard_f != b.synth_guard_f { out.push("synth_guard_f"); }
    if a.tns_long_observed != b.tns_long_observed { out.push("tns_long_observed"); }
    if a.e_long_q31 != b.e_long_q31 { out.push("e_long_q31"); }
    if a.ld_long_bits != b.ld_long_bits { out.push("ld_long"); }
    if a.e_tns_fpe_stored != b.e_tns_fpe_stored { out.push("e_tns_fpe"); }
    if a.ld_tns_fpe_bits != b.ld_tns_fpe_bits { out.push("ld_tns_fpe"); }
    if a.ms_observed != b.ms_observed { out.push("ms_observed"); }
    if a.ms_mid_ld_bits != b.ms_mid_ld_bits { out.push("ms_mid_ld"); }
    if a.ms_side_ld_bits != b.ms_side_ld_bits { out.push("ms_side_ld"); }
    if a.grouped_observed != b.grouped_observed { out.push("grouped_observed"); }
    if a.group_idx != b.group_idx { out.push("group_idx"); }
    if a.grp_e_stored != b.grp_e_stored { out.push("grp_e_stored"); }
    if a.grp_ld_bits != b.grp_ld_bits { out.push("grp_ld"); }
    if a.grp_thr_ld_bits != b.grp_thr_ld_bits { out.push("grp_thr_ld"); }
    out
}

fn layer_b_to_jsonl(rows: &[LayerBRow], env: &EnvMeta, pins: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "{{\"kind\":\"env\",\"arch\":\"{}\",\"os\":\"{}\",\"layer\":\"B\",\"fixtures\":[{}],\"limitations\":\"{}\"}}",
        json_escape(env.arch),
        json_escape(env.os),
        pins.iter().map(|p| format!("\"{}\"", json_escape(p))).collect::<Vec<_>>().join(","),
        json_escape("TNS-filtered spectra not driven (unfiltered-spectrum diagnostic for long recompute); float FPE MS energies outstanding (FPE-7.1); q31_of_ld is an in-test mirror of a private fn; long-frame thresholds simplified (init-only) so long threshold Ld is not captured — covered by C-06 floor probes"),
    ));
    for r in rows {
        out.push(format!(
            "{{\"kind\":\"row\",\"layer\":\"B\",\"key\":\"{}\",\"fixture\":\"{}\",\"frame\":{},\"ch\":{},\"win\":{},\"band\":{},\"short\":{},\"scale0\":{},\"f\":{},\"shiftBits\":{},\"e_q31\":{},\"e_f\":{},\"e_f_bits\":\"{:#018x}\",\"log2q_bits\":\"{:#018x}\",\"log2f_bits\":\"{:#018x}\",\"ldq\":{},\"ldq_bits\":\"{:#018x}\",\"ldf\":{},\"ldf_bits\":\"{:#018x}\",\"q31mirror\":{},\"sgq\":{},\"sgf\":{},\"sentinel\":{},\"d_tnsguard\":{:.6e},\"d_sent\":{:.6e},\"tns_obs\":{},\"e_long\":{},\"ld_long_bits\":\"{:#018x}\",\"ld_long\":{},\"e_tnsfpe\":{},\"ld_tnsfpe_bits\":\"{:#018x}\",\"ld_tnsfpe\":{},\"ms_obs\":{},\"ms_mid_bits\":\"{:#018x}\",\"ms_side_bits\":\"{:#018x}\",\"grp_obs\":{},\"grp\":{},\"grp_e\":{},\"grp_ld_bits\":\"{:#018x}\",\"grp_ld\":{},\"grp_thr_bits\":\"{:#018x}\"}}",
            json_escape(&r.key()),
            json_escape(r.fixture),
            r.frame, r.channel, r.window, r.band, r.short,
            r.scale0, r.final_shift, r.shift_bits,
            r.energy_q31,
            f64_to_json_num(r.energy_f), r.energy_f.to_bits(),
            r.log2_q_bits, r.log2_f_bits,
            f64_to_json_num(r.ld_q31()), r.ld_q31_bits,
            f64_to_json_num(r.ld_fpe()), r.ld_fpe_bits,
            r.q31_of_ld_mirror, r.synth_guard_q, r.synth_guard_f, r.sentinel,
            r.dist_to_tns_guard, r.dist_to_sentinel_ld,
            r.tns_long_observed, r.e_long_q31,
            r.ld_long_bits, f64_to_json_num(f64::from_bits(r.ld_long_bits)),
            r.e_tns_fpe_stored,
            r.ld_tns_fpe_bits, f64_to_json_num(f64::from_bits(r.ld_tns_fpe_bits)),
            r.ms_observed, r.ms_mid_ld_bits, r.ms_side_ld_bits,
            r.grouped_observed, r.group_idx, r.grp_e_stored,
            r.grp_ld_bits, f64_to_json_num(f64::from_bits(r.grp_ld_bits)),
            r.grp_thr_ld_bits,
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Layer C — deterministic boundary probes
// ---------------------------------------------------------------------------

/// One deterministic probe around an exact source-defined boundary.
/// `synthetic` is always true for Layer C (kept separate from corpus rows).
/// The stable cross-architecture join key is [`ProbeRow::key`]
/// (`probe_id` + `input` description): `input_bits` alone is NOT unique —
/// C-04 deliberately probes the same bit pattern under two labels
/// (`min_sub` vs `smallest_pos_above_zero`, both `0x1`).
#[derive(Debug, Clone)]
struct ProbeRow {
    probe_id: &'static str,
    boundary_expr: &'static str,
    source_loc: &'static str,
    input_desc: String,
    input_bits: u64,
    expected_side: &'static str,
    observed: String,
    observed_bits: u64,
    differs: bool,
}

impl ProbeRow {
    fn key(&self) -> String {
        format!("{}:{}", self.probe_id, self.input_desc)
    }
}

const PROBE_GUARD_EXPR: &str = "base >= (-0.5 + scaleDiff/2.0)";
const PROBE_GUARD_LOC: &str = "rust/src/aac_enc/energy.rs:284; rust/src/aac_enc/fpe/energy.rs:264";
const PROBE_SHIFT_EXPR: &str = "while run > 0.0 { run -= 2.0/64.0; shiftBits += 1 } from maxLd";
const PROBE_SHIFT_LOC: &str = "rust/src/aac_enc/energy.rs:292-302";
const PROBE_MAXNRG_EXPR: &str = "if ld > max_ld { max_ld = ld; nr = b }";
const PROBE_MAXNRG_LOC: &str = "rust/src/aac_enc/energy.rs:190-209";
const PROBE_SENTINEL_EXPR: &str = "value <= 0.0 -> -1.0 (ld_data); pre/e_raw_f <= 0 -> -1.0";
const PROBE_SENTINEL_LOC: &str = "rust/src/aac_enc/sfb.rs:185; rust/src/aac_enc/energy.rs:195,283; rust/src/aac_enc/fpe/energy.rs:143,256";
const PROBE_Q31OF_EXPR: &str = "if ld <= -1.0 { MIN } else { (ld*2^31).round() as i32 }";
const PROBE_Q31OF_LOC: &str = "rust/src/aac_enc/stream_driver.rs:220-226 (private; in-test mirror)";
const PROBE_THRFLOOR_EXPR: &str = "v.max(-33.0/64.0)";
const PROBE_THRFLOOR_LOC: &str = "rust/src/aac_enc/psy.rs:1546-1551 (private grouped_ld_clamped; in-test mirror)";

/// Production `shiftBits` loop, verbatim (`energy.rs:292-302`), executed in
/// test on a given `max_ld`. Used to pin expected sides of the probes.
fn shift_bits_of(max_ld: f64) -> i32 {
    if max_ld <= 0.0 {
        return 0;
    }
    let mut run = max_ld;
    let mut s = 0;
    while run > 0.0 {
        run -= 2.0 / 64.0;
        s += 1;
    }
    s
}

fn layer_c_probes() -> Vec<ProbeRow> {
    let mut rows = Vec::new();
    // C-01: TNS guard at several scaleDiff values; probe base at
    // threshold and ±1 ULP (bit-exact neighbours of the threshold).
    for &leading in &[0i32, 4, 8, -2] {
        let scale_diff = leading as f64 * 2.0 / 64.0;
        let thresh = -0.5 + scale_diff / 2.0;
        for (tag, base) in [
            ("at", thresh),
            ("above1ulp", next_up_bits(thresh)),
            ("below1ulp", next_down_bits(thresh)),
        ] {
            let pass = base >= thresh;
            rows.push(ProbeRow {
                probe_id: "C-01-tns-guard",
                boundary_expr: PROBE_GUARD_EXPR,
                source_loc: PROBE_GUARD_LOC,
                input_desc: format!("leading={leading} scaleDiff={scale_diff:.6} base@{tag}"),
                input_bits: base.to_bits(),
                expected_side: if pass { "pass" } else { "clamp-to-sentinel" },
                observed: (if pass { "pass" } else { "clamp-to-sentinel" }).to_string(),
                observed_bits: (pass as u8 as u64),
                differs: false,
            });
        }
    }
    // C-02: shiftBits at exact multiples of 2/64 and ±1 ULP.
    for k in 0..5 {
        let exact = k as f64 * 2.0 / 64.0;
        for (tag, v) in [("at", exact), ("above", next_up_bits(exact)), ("below", next_down_bits(exact))] {
            // Expected: the verbatim loop count (computed, cited).
            let want = shift_bits_of(v);
            // Observed: call the production long kernel's shift path
            // indirectly is not reachable for exact multiples via i32 pre,
            // so the observed side here is the verbatim-loop value itself
            // (self-consistency); cross-arch compares this integer.
            rows.push(ProbeRow {
                probe_id: "C-02-shiftbits",
                boundary_expr: PROBE_SHIFT_EXPR,
                source_loc: PROBE_SHIFT_LOC,
                input_desc: format!("maxLd=k*2/64 k={k} @{tag}"),
                input_bits: v.to_bits(),
                expected_side: if k == 0 && tag == "at" { "0" } else { "loop-count" },
                observed: format!("{want}"),
                observed_bits: want as u64,
                differs: false,
            });
        }
    }
    // C-03: max_nrg_for_shift near-tie selection via production kernel.
    {
        // Two bands, identical maxScale; energies chosen so Ld values are
        // adjacent representable floats (1 ULP apart after log2/64).
        let e0 = 1_000_000i32;
        let e1 = e0 + 1;
        let out0 = q31_energy::max_nrg_for_shift(&[e0, e1], &[10, 10], 10);
        let out1 = q31_energy::max_nrg_for_shift(&[e1, e0], &[10, 10], 10);
        let ld0 = ((e0 as f64) / 2_147_483_648.0).log2() / 64.0;
        let ld1 = ((e1 as f64) / 2_147_483_648.0).log2() / 64.0;
        rows.push(ProbeRow {
            probe_id: "C-03-maxnrg-tie",
            boundary_expr: PROBE_MAXNRG_EXPR,
            source_loc: PROBE_MAXNRG_LOC,
            input_desc: format!("tie e0={e0} e1={e1} ld0={ld0:.12e} ld1={ld1:.12e}"),
            input_bits: ((e0 as u64) << 32) | (e1 as u64 & 0xffff_ffff),
            expected_side: "higher-Ld band wins (strict >)",
            observed: format!("order0->maxNrg={out0} order1->maxNrg={out1}"),
            observed_bits: (out0 as u64).wrapping_add((out1 as u64) << 32),
            differs: false,
        });
        // Exact tie: identical energies -> first band wins (strict >).
        let tie = q31_energy::max_nrg_for_shift(&[e0, e0], &[10, 10], 10);
        rows.push(ProbeRow {
            probe_id: "C-03-maxnrg-tie",
            boundary_expr: PROBE_MAXNRG_EXPR,
            source_loc: PROBE_MAXNRG_LOC,
            input_desc: format!("exact tie e={e0} both bands"),
            input_bits: ((e0 as u64) << 32) | (e0 as u64 & 0xffff_ffff),
            expected_side: "first band wins",
            observed: format!("maxNrg={tie}"),
            observed_bits: tie as u64,
            differs: false,
        });
    }
    // C-04: sentinel boundaries via production ld_data.
    for (tag, x) in [
        ("pos_zero", 0.0f64),
        ("neg_zero", -0.0f64),
        ("min_sub", f64::from_bits(1)),
        ("neg_min_sub", f64::from_bits(0x8000_0000_0000_0001)),
        ("min_normal", f64::MIN_POSITIVE),
        ("neg_small", -1e-300f64),
        ("smallest_pos_above_zero", f64::from_bits(1)),
    ] {
        let ld = sfb::ld_data(x);
        let expect_sentinel = x <= 0.0;
        rows.push(ProbeRow {
            probe_id: "C-04-sentinel",
            boundary_expr: PROBE_SENTINEL_EXPR,
            source_loc: PROBE_SENTINEL_LOC,
            input_desc: format!("ld_data({tag})"),
            input_bits: x.to_bits(),
            expected_side: if expect_sentinel { "-1.0 sentinel" } else { "finite log2/64" },
            observed: format!("{ld:.17e}"),
            observed_bits: ld.to_bits(),
            differs: (ld == -1.0) != expect_sentinel,
        });
    }
    // C-05: q31_of_ld mirror boundaries (private fn; mirror cited).
    for (tag, ld) in [
        ("at", -1.0f64),
        ("above1ulp", next_up_bits(-1.0)),
        ("below1ulp", next_down_bits(-1.0)),
        ("zero", 0.0f64),
        ("half_lsb_up", 0.5f64 / 2_147_483_648.0),
        ("neg_half_lsb", -0.5f64 / 2_147_483_648.0),
    ] {
        let got = q31_of_ld_mirror(ld);
        let want_sentinel = ld <= -1.0;
        rows.push(ProbeRow {
            probe_id: "C-05-q31-of-ld",
            boundary_expr: PROBE_Q31OF_EXPR,
            source_loc: PROBE_Q31OF_LOC,
            input_desc: format!("q31_of_ld({tag})"),
            input_bits: ld.to_bits(),
            expected_side: if want_sentinel { "MIN sentinel" } else { "rounded int" },
            observed: format!("{got}"),
            observed_bits: got as u32 as u64,
            differs: (got == i32::MIN) != want_sentinel,
        });
    }
    // C-06: threshold floor mirror (-33/64).
    for (tag, v) in [
        ("at", sfb::THRESHOLD_LD_FLOOR),
        ("above", next_up_bits(sfb::THRESHOLD_LD_FLOOR)),
        ("below", next_down_bits(sfb::THRESHOLD_LD_FLOOR)),
    ] {
        let floored = v.max(sfb::THRESHOLD_LD_FLOOR);
        rows.push(ProbeRow {
            probe_id: "C-06-threshold-floor",
            boundary_expr: PROBE_THRFLOOR_EXPR,
            source_loc: PROBE_THRFLOOR_LOC,
            input_desc: format!("floor({tag})"),
            input_bits: v.to_bits(),
            expected_side: if v <= sfb::THRESHOLD_LD_FLOOR { "clamped to -33/64" } else { "passes through" },
            observed: format!("{floored:.17e}"),
            observed_bits: floored.to_bits(),
            differs: false,
        });
    }
    // C-07: grouped_ld mirror sentinel (v<=0 -> -1.0) via production ints.
    for v in [0i32, -5, 1, i32::MAX] {
        let got = grouped_ld_mirror(&[v])[0];
        rows.push(ProbeRow {
            probe_id: "C-07-grouped-ld",
            boundary_expr: "v<=0 -> -1.0 else (v/2^31).log2/64 [psy.rs:1532 mirror]",
            source_loc: "rust/src/aac_enc/psy.rs:1532-1543 (private; in-test mirror)",
            input_desc: format!("grouped_ld({v})"),
            input_bits: v as u32 as u64,
            expected_side: if v <= 0 { "-1.0 sentinel" } else { "finite" },
            observed: format!("{got:.17e}"),
            observed_bits: got.to_bits(),
            differs: (got == -1.0) != (v <= 0),
        });
    }
    rows
}

fn layer_c_to_jsonl(rows: &[ProbeRow], env: &EnvMeta) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "{{\"kind\":\"env\",\"arch\":\"{}\",\"os\":\"{}\",\"layer\":\"C\",\"synthetic_only\":true,\"limitations\":\"{}\"}}",
        json_escape(env.arch),
        json_escape(env.os),
        json_escape("probes are control-flow diagnostics, not R2.2 decision-safety evidence"),
    ));
    for r in rows {
        out.push(format!(
            "{{\"kind\":\"row\",\"layer\":\"C\",\"synthetic\":true,\"probe\":\"{}\",\"expr\":\"{}\",\"loc\":\"{}\",\"input\":\"{}\",\"input_bits\":\"{:#018x}\",\"expected\":\"{}\",\"observed\":\"{}\",\"observed_bits\":\"{:#018x}\",\"differs\":{}}}",
            json_escape(r.probe_id),
            json_escape(r.boundary_expr),
            json_escape(r.source_loc),
            json_escape(&r.input_desc),
            r.input_bits,
            json_escape(r.expected_side),
            json_escape(&r.observed),
            r.observed_bits,
            r.differs,
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Coverage matrix (call site x layer)
// ---------------------------------------------------------------------------

/// (call site, source location, Layer-A evidence, Layer-B evidence,
/// Layer-C evidence, negative-control note). Every "captured" claim names
/// the recorded row fields; mirrors and diagnostics are labelled as such.
const COVERAGE: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("CS-ld_data", "sfb.rs:184", "direct (log2_raw+ld, 52 inputs)", "rescale rows ld_q31/ld_fpe; sentinel rows", "C-04 sentinel (ld_data incl. -0.0)", ""),
    ("CS-q31-log2frac", "energy.rs:415", "direct", "rescale rows log2_q; TNS-long rows e_long/ld_long (PROD band_energy_long_into)", "C-01 guard inputs", ""),
    ("CS-q31-rescale-ld", "energy.rs:199", "direct", "rescale rows ld_q31; synth_guard_* diagnostic (NOT a production guard)", "C-01/C-02", "rescale has no TNS guard — see F-4 note"),
    ("CS-q31-tns-long-base", "energy.rs:283", "direct", "TNS-long rows e_long_q31/ld_long_bits (PROD, unfiltered spectrum)", "C-01/C-02", ""),
    ("CS-q31-grouped-ld", "psy.rs:1539", "direct", "short rows grp_e_stored/grp_ld_bits via PROD finish_channel; grp_thr_ld_bits init-threshold diagnostic", "C-07 mirror + C-06 floor", ""),
    ("CS-fpe-rescale-base", "fpe/energy.rs:147", "direct", "rescale rows log2_f/ld_fpe", "C-01", ""),
    ("CS-fpe-tns-long-base", "fpe/energy.rs:259", "direct", "TNS-long rows e_tns_fpe_stored/ld_tns_fpe_bits (PROD, unfiltered spectrum)", "C-01", ""),
    ("CS-q31-ms-ld", "energy.rs:365", "direct", "long ch0 rows ms_mid/ms_side_ld_bits via PROD calc_ms_energies", "—", "integer lane; FPE float MS outstanding (FPE-7.1)"),
    ("NC-short-ld-int", "q31.rs:380 calc_ld_data", "n/a (integer)", "n/a (integer)", "n/a", "negative control: exact integer path, no log2"),
    ("NC-short-tns-nold", "psy.rs:1191-1200 / FPE TNS short", "n/a", "short rows assert short-Ld untouched only via existing unit test", "n/a", "negative control"),
    ("NC-grouped-ms-int", "psy.rs:1483 calc_ld_data", "n/a (integer)", "grouped ms_* integer path not regrouped here (zeros input); covered by calc_ld_data NC", "n/a", "negative control: integer path"),
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod xarch_tests {
    use super::*;

    #[test]
    fn xarch_log2_layer_a_deterministic_repeatability() {
        let a = layer_a_capture();
        let b = layer_a_capture();
        assert_eq!(a.len(), b.len(), "capture length stable");
        assert!(!a.is_empty(), "non-empty capture");
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.call_site, y.call_site);
            assert_eq!(x.input_bits, y.input_bits);
            assert_eq!(x.log2_bits, y.log2_bits, "log2 deterministic for {:#x}", x.input_bits);
            assert_eq!(x.ld_bits, y.ld_bits, "ld deterministic");
        }
        // Join keys unique.
        let mut keys = std::collections::BTreeSet::new();
        for r in &a {
            assert!(keys.insert((r.call_site, r.input_bits)), "duplicate join key");
        }
    }

    #[test]
    fn xarch_log2_layer_a_exceptional_classification() {
        // NaN inputs: log2(NaN)=NaN, ld_data(NaN): NaN<=0 is false -> NaN/64=NaN.
        for bits in [0x7FF8_0000_0000_0000u64, 0x7FF0_0000_0000_0001, 0xFFF8_0000_0000_0000] {
            let x = f64::from_bits(bits);
            assert!(x.is_nan());
            let (lb, db) = layer_a_eval(bits);
            assert!(f64::from_bits(lb).is_nan(), "log2(NaN)=NaN");
            assert_eq!(classify_pair(x, x), UlpClass::Nan);
            let _ = db;
        }
        // Zeros and infinities.
        assert_eq!(classify_pair(0.0, -0.0), UlpClass::SignedZero);
        assert_eq!(classify_pair(0.0, 0.0), UlpClass::Exact);
        assert_eq!(classify_pair(f64::INFINITY, f64::NEG_INFINITY), UlpClass::InfSign);
        assert_eq!(
            classify_pair(1.0, f64::INFINITY),
            UlpClass::FiniteVsInfinite
        );
        // Self ULP is zero; 1-ULP neighbours differ by exactly 1.
        let a = 1.0f64;
        let b = next_up_bits(a);
        assert!(matches!(classify_pair(a, a), UlpClass::Exact));
        assert_eq!(classify_pair(a, b), UlpClass::FiniteUlp(1));
        assert_eq!(sfb::ld_data(0.0), -1.0);
        assert_eq!(sfb::ld_data(-3.0), -1.0);
    }

    #[test]
    fn xarch_log2_layer_a_self_comparison_is_clean() {
        let a = layer_a_capture();
        let cmp = compare_layer_a(&a, &a);
        // NaN inputs are excluded from numeric aggregates and counted.
        assert!(cmp.nan_rows > 0, "NaN inputs present and separated");
        // NaN outputs (log2 of negative-domain finite inputs) are
        // separated per the task's NaN rule, never in acceptance stats.
        assert!(cmp.nan_output_rows > 0, "NaN-output rows present and separated");
        // F-2: comparable rows are bit-identical; only the finite-finite
        // subset contributes to the abs/ULP aggregates (infinities from
        // log2(±0)/log2(+inf) have no ULP distance and are excluded from
        // max_abs by abs_diff_finite).
        assert_eq!(cmp.max_abs_log2, 0.0);
        assert_eq!(cmp.max_abs_ld, 0.0);
        assert_eq!(cmp.max_ulp_log2, 0);
        assert!(cmp.differing_keys.is_empty());
        assert!(cmp.nan_output_keys.iter().all(|k| !k.contains(":ld")));
        assert_eq!(cmp.exact_log2 + cmp.nan_rows + cmp.nan_output_rows, cmp.rows);
        assert_eq!(cmp.exact_ld + cmp.nan_rows, cmp.rows);
        let n_inf_out = a.iter().filter(|r| {
            !r.input_value().is_nan() && r.log2_value().is_infinite()
        }).count();
        assert_eq!(n_inf_out, 24, "infinite outputs: log2(±0), log2(+inf)");
    }

    #[test]
    fn xarch_log2_layer_a_perturbation_is_detected() {
        let a = layer_a_capture();
        let mut b = a.clone();
        // Flip one finite row by 1 ULP in log2; NaN rows stay untouched.
        let idx = b
            .iter()
            .position(|r| {
                let v = f64::from_bits(r.log2_bits);
                v.is_finite() && !f64::from_bits(r.input_bits).is_nan()
            })
            .expect("finite row exists");
        b[idx].log2_bits = next_up_bits(f64::from_bits(b[idx].log2_bits)).to_bits();
        let cmp = compare_layer_a(&a, &b);
        assert_eq!(cmp.differing_keys.len(), 1);
        assert!(cmp.max_abs_log2 > 0.0);
        assert_eq!(cmp.max_ulp_log2, 1);
    }

    #[test]
    fn xarch_log2_layer_a_capture_writes_jsonl() {
        let env = collect_env();
        let rows = layer_a_capture();
        let pins: Vec<String> = LAYER_B_FIXTURES.iter().map(|f| fixture_pin(f)).collect();
        let lines = layer_a_to_jsonl(&rows, &env, &pins);
        assert!(lines.len() > rows.len());
        assert!(lines[0].contains("\"kind\":\"env\""));
        let path = capture_path("layerA");
        write_lines(&path, &lines);
        // Coverage: every call site has rows for every input.
        let n_inputs = layer_a_inputs().len();
        for (site, _) in LAYER_A_CALL_SITES {
            let n = rows.iter().filter(|r| r.call_site == *site).count();
            assert_eq!(n, n_inputs, "call site {site} covered");
        }
        println!(
            "Layer A native capture: {} rows ({} sites x {} inputs: {} input-NaN + {} output-NaN separated; {} infinite-output, {} finite-finite comparable), env={}/{}, file={}",
            rows.len(),
            LAYER_A_CALL_SITES.len(),
            n_inputs,
            rows.iter().filter(|r| r.input_value().is_nan()).count(),
            rows.iter().filter(|r| !r.input_value().is_nan() && r.log2_value().is_nan()).count(),
            rows.iter().filter(|r| !r.input_value().is_nan() && r.log2_value().is_infinite()).count(),
            rows.iter().filter(|r| !r.input_value().is_nan() && f64::from_bits(r.log2_bits).is_finite()).count(),
            env.arch,
            env.os,
            path.display()
        );
    }

    #[test]
    fn xarch_log2_layer_b_capture_deterministic_and_joinable() {
        let a = layer_b_capture();
        let b = layer_b_capture();
        assert_eq!(a.len(), b.len(), "capture length stable");
        assert!(!a.is_empty(), "non-empty Layer-B capture");
        let mut keys = std::collections::BTreeSet::new();
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.key(), y.key());
            assert_eq!(x.energy_q31, y.energy_q31);
            assert_eq!(x.log2_q_bits, y.log2_q_bits);
            assert_eq!(x.log2_f_bits, y.log2_f_bits);
            assert_eq!(x.ld_q31_bits, y.ld_q31_bits);
            assert_eq!(x.ld_fpe_bits, y.ld_fpe_bits);
            // F-1: retained diagnostics are deterministic too.
            assert_eq!(x.e_long_q31, y.e_long_q31, "e_long deterministic");
            assert_eq!(x.ld_long_bits, y.ld_long_bits, "ld_long deterministic");
            assert_eq!(x.e_tns_fpe_stored, y.e_tns_fpe_stored);
            assert_eq!(x.ld_tns_fpe_bits, y.ld_tns_fpe_bits);
            assert_eq!(x.ms_mid_ld_bits, y.ms_mid_ld_bits);
            assert_eq!(x.ms_side_ld_bits, y.ms_side_ld_bits);
            assert_eq!(x.grp_e_stored, y.grp_e_stored);
            assert_eq!(x.grp_ld_bits, y.grp_ld_bits);
            assert_eq!(x.grp_thr_ld_bits, y.grp_thr_ld_bits);
            assert!(keys.insert(x.key()), "duplicate Layer-B key {}", x.key());
        }
        // Classifier: identical pairs classify Identical; each corruption
        // class is detected in priority order.
        let mut corrupt = a[0].clone();
        corrupt.energy_q31 = corrupt.energy_q31.wrapping_add(1);
        assert_eq!(classify_b_pair(&a[0], &corrupt), BPairClass::UpstreamInputDiff);
        let mut c2 = a[0].clone();
        // Keep energies identical, corrupt log2 only.
        c2.log2_f_bits ^= 1;
        // (guard against the degenerate case where xor lands on same bits)
        if c2.log2_f_bits != a[0].log2_f_bits {
            assert_eq!(classify_b_pair(&a[0], &c2), BPairClass::PureLog2Diff);
        }
        let mut c3 = a[0].clone();
        c3.ld_fpe_bits ^= 1;
        if c3.ld_fpe_bits != a[0].ld_fpe_bits
            && c3.log2_f_bits == a[0].log2_f_bits
            && c3.log2_q_bits == a[0].log2_q_bits
        {
            assert_eq!(classify_b_pair(&a[0], &c3), BPairClass::LdGuardDiff);
        }
        // Upstream masks downstream: corrupt both energy and log2, the
        // classification must still be upstream — but the raw field list
        // keeps the downstream difference VISIBLE (F-1 §5).
        let mut c4 = a[0].clone();
        c4.energy_q31 = c4.energy_q31.wrapping_add(7);
        c4.log2_f_bits ^= 0xFF;
        assert_eq!(classify_b_pair(&a[0], &c4), BPairClass::UpstreamInputDiff);
        let fields = b_field_diffs(&a[0], &c4);
        assert!(fields.contains(&"energy_q31"), "upstream field listed");
        assert!(fields.contains(&"log2_f"), "masked log2 diff still visible");
        assert_eq!(classify_b_pair(&a[0], &a[0]), BPairClass::Identical);
        assert!(b_field_diffs(&a[0], &a[0]).is_empty(), "no field diffs on self");
        // New diagnostics participate: corrupt only a retained field.
        let mut c5 = a[0].clone();
        c5.grp_e_stored = c5.grp_e_stored.wrapping_add(3);
        assert_eq!(classify_b_pair(&a[0], &c5), BPairClass::ExtendedDiagnosticDiff);
        assert!(b_field_diffs(&a[0], &c5).contains(&"grp_e_stored"));
        let mut c6 = a[0].clone();
        c6.ld_long_bits ^= 0x0F;
        if c6.ld_long_bits != a[0].ld_long_bits {
            assert_eq!(classify_b_pair(&a[0], &c6), BPairClass::ExtendedDiagnosticDiff);
            assert!(b_field_diffs(&a[0], &c6).contains(&"ld_long"));
        }
    }

    #[test]
    fn xarch_log2_layer_b_capture_report_and_write() {
        let env = collect_env();
        let rows = layer_b_capture();
        let pins: Vec<String> = LAYER_B_FIXTURES.iter().map(|f| fixture_pin(f)).collect();
        let lines = layer_b_to_jsonl(&rows, &env, &pins);
        let path = capture_path("layerB");
        write_lines(&path, &lines);
        // Per-fixture / short-long accounting (no silent omission).
        for f in LAYER_B_FIXTURES {
            assert!(rows.iter().any(|r| r.fixture == *f), "fixture {f} captured");
        }
        let n_short = rows.iter().filter(|r| r.short).count();
        let n_long = rows.iter().filter(|r| !r.short).count();
        let n_sent = rows.iter().filter(|r| r.sentinel).count();
        // F-1: retained-diagnostic presence accounting. Every long row
        // must carry the TNS-long production results; every short row the
        // grouped production results; long ch0 rows the MS results.
        let n_tns = rows.iter().filter(|r| r.tns_long_observed).count();
        let n_grp = rows.iter().filter(|r| r.grouped_observed).count();
        let n_ms = rows.iter().filter(|r| r.ms_observed).count();
        assert_eq!(n_tns, n_long, "all long rows carry TNS-long diagnostics");
        assert_eq!(n_grp, n_short, "all short rows carry grouped diagnostics");
        assert!(
            rows.iter().filter(|r| !r.short && r.ms_observed).count() > 0,
            "MS diagnostics present on long ch0 rows"
        );
        assert!(
            rows.iter().all(|r| r.ms_observed == (!r.short && r.channel == 0)),
            "ms_observed exactly on long ch0 rows"
        );
        // JSON serializes every new field (would fail if a field were
        // dropped from the evidence again).
        for key in [
            "\"tns_obs\":", "\"e_long\":", "\"ld_long_bits\":", "\"e_tnsfpe\":",
            "\"ms_obs\":", "\"ms_mid_bits\":", "\"grp_obs\":", "\"grp_e\":",
            "\"grp_ld_bits\":", "\"grp_thr_bits\":", "\"sgq\":", "\"d_tnsguard\":",
        ] {
            assert!(lines.iter().any(|l| l.contains(key)), "JSON carries {key}");
        }
        // Absolute Ld deltas Q31-vs-FPE per row are reported (not gated):
        // measure max over rows with both lanes finite.
        let mut max_abs_ld = 0.0f64;
        let mut n_finite = 0usize;
        for r in &rows {
            let (q, f) = (r.ld_q31(), r.ld_fpe());
            if q.is_finite() && f.is_finite() {
                n_finite += 1;
                max_abs_ld = max_abs_ld.max((q - f).abs());
            }
        }
        // Existing contract spot-check on stored ints (reporting only):
        // compare at the SAME shift stage — production post-shift stored
        // `shift_int(e_raw, -scale)` vs `fraction_to_stored(e_f)` — with
        // `q31_tol`. (Comparing raw pre-shift sums against the post-shift
        // fraction would be a stage mismatch, not a contract result.)
        let mut within_q31 = 0usize;
        for r in &rows {
            let scale = (((r.scale0 - r.final_shift) << 1).min(31));
            let stored_q31_post = shift_int(r.energy_q31, -scale);
            let stored_mirror = fpe_output::fraction_to_stored(r.energy_f);
            let d = (stored_mirror as i64 - stored_q31_post as i64).abs();
            if d <= q31_tol(stored_q31_post) {
                within_q31 += 1;
            }
        }
        println!(
            "Layer B native capture: {} rows (short={} long={} sentinel={} tns_long={} grouped={} ms={}), both-finite Ld rows={} max|dLd(Q31-FPE)|={:.3e}, stored within q31_tol {}/{}, file={}",
            rows.len(), n_short, n_long, n_sent, n_tns, n_grp, n_ms, n_finite, max_abs_ld, within_q31, rows.len(), path.display()
        );
        // Contract applicability note: q31_tol is an INTEGER tolerance and
        // is reported here only for stored ints; it is never applied to Ld
        // floats (see §8 of the report).
        let _ = energy_tol(0.0);
        let _ = ld_to_e(-1.0);
    }

    #[test]
    fn xarch_log2_layer_c_probes_deterministic_and_separate() {
        let p1 = layer_c_probes();
        let p2 = layer_c_probes();
        assert_eq!(p1.len(), p2.len());
        assert!(!p1.is_empty());
        for (a, b) in p1.iter().zip(p2.iter()) {
            assert_eq!(a.probe_id, b.probe_id);
            assert_eq!(a.input_bits, b.input_bits);
            assert_eq!(a.observed_bits, b.observed_bits);
            assert_eq!(a.differs, b.differs);
        }
        // Every cited boundary family is present.
        for id in [
            "C-01-tns-guard",
            "C-02-shiftbits",
            "C-03-maxnrg-tie",
            "C-04-sentinel",
            "C-05-q31-of-ld",
            "C-06-threshold-floor",
            "C-07-grouped-ld",
        ] {
            assert!(p1.iter().any(|r| r.probe_id == id), "probe {id} present");
        }
        // Sentinel probes agree with the source rule on native arch.
        assert!(p1.iter().filter(|r| r.probe_id == "C-04-sentinel").all(|r| !r.differs));
        // C-05: the `ld <= -1.0` source rule is exact, but the
        // `(ld*2^31).round()` mirror maps ld just ABOVE -1.0 (within half
        // an output LSB) back onto `i32::MIN` — a genuine rounding-boundary
        // spill, pinned here and reported, not hidden. All other C-05
        // probes follow the source rule exactly.
        for r in p1.iter().filter(|r| r.probe_id == "C-05-q31-of-ld") {
            if r.input_desc.contains("above1ulp") {
                assert!(r.differs, "above1ulp must pin the MIN rounding spill");
                assert_eq!(r.observed, "-2147483648");
            } else {
                assert!(!r.differs, "probe {} must follow the source rule", r.input_desc);
            }
        }
        assert!(p1.iter().filter(|r| r.probe_id == "C-07-grouped-ld").all(|r| !r.differs));
        // F-5: the cross-architecture join key (probe_id + input
        // description) is unique across all rows. input_bits alone is
        // NOT (C-04 probes `min_sub` and `smallest_pos_above_zero` share
        // 0x1 by design).
        let mut pkeys = std::collections::BTreeSet::new();
        for r in &p1 {
            assert!(pkeys.insert(r.key()), "duplicate Layer-C key {}", r.key());
        }
        // C-04 deliberately reuses one bit pattern under two labels, so
        // input_bits alone cannot be the join key — while probe keys are.
        {
            let c4: Vec<&ProbeRow> = p1.iter().filter(|r| r.probe_id == "C-04-sentinel").collect();
            let mut bits = std::collections::BTreeSet::new();
            let mut dup_bits = false;
            for r in &c4 {
                if !bits.insert(r.input_bits) {
                    dup_bits = true;
                }
            }
            assert!(dup_bits, "C-04 reuses input_bits by design (F-5 premise)");
        }
        let env = collect_env();
        let lines = layer_c_to_jsonl(&p1, &env);
        assert!(lines[0].contains("\"synthetic_only\":true"));
        let path = capture_path("layerC");
        write_lines(&path, &lines);
        println!(
            "Layer C native probes: {} synthetic rows (never mixed with corpus), file={}",
            p1.len(),
            path.display()
        );
    }

    #[test]
    fn xarch_log2_negative_controls_integer_paths() {
        // NC-INT: calc_ld_data is the exact integer path for short-window,
        // threshold and grouped-MS Ld. Deterministic, sentinel-correct, and
        // independent of f64::log2 (no float primitive involved).
        assert_eq!(calc_ld_data(0), i32::MIN);
        assert_eq!(calc_ld_data(-7), i32::MIN);
        let a = calc_ld_data(1);
        let b = calc_ld_data(1);
        assert_eq!(a, b, "integer path deterministic");
        assert_eq!(calc_ld_data(i32::MAX), calc_ld_data(i32::MAX));
        // Short TNS branch leaves Ld untouched (production quirk preserved).
        let spec = [0.125f64, 0.25, 0.5, 1.0, 0.0, 0.0, 0.0, 0.0];
        let offsets = [0u16, 4, 8];
        let before = vec![-1.0 + 1.0 * 2.0 / 64.0, 0.333];
        let mut ld = before.clone();
        let mut e = vec![0i32; 2];
        fpe_energy::tns_recompute_energy_into(&spec, &offsets, 2, 9, &[9, 9], 0, true, &mut e, &mut ld);
        assert_eq!(ld, before, "short TNS leaves Ld untouched (negative control)");
        // Threshold floor constant pins the source value.
        assert_eq!(sfb::THRESHOLD_LD_FLOOR, -33.0 / 64.0);
        assert_eq!(sfb::LD_ZERO, -1.0);
    }

    #[test]
    fn xarch_log2_instrumentation_does_not_change_production() {
        // Run production kernels, run the full harness capture, run the
        // kernels again: identical outputs prove the harness is read-only.
        let spec = [0.25f64, -0.5, 0.75, 1.0, 0.5, -0.25, 0.125, 0.0];
        let offsets = [0u16, 4, 8];
        let (m, f) = (12, 3);
        let maxsc = [6, 8];
        let before = {
            let q = q31_energy::map_spectrum_to_q31(&spec, m);
            let mut eraw = Vec::new();
            q31_energy::check_band_energy_into(&q, &maxsc, &offsets, 2, &mut eraw);
            let mut ld = Vec::new();
            q31_energy::rescale_ld_into(&eraw, &maxsc, f, &mut ld);
            (q, eraw, ld)
        };
        let _ = layer_a_capture();
        let _ = layer_c_probes();
        let after = {
            let q = q31_energy::map_spectrum_to_q31(&spec, m);
            let mut eraw = Vec::new();
            q31_energy::check_band_energy_into(&q, &maxsc, &offsets, 2, &mut eraw);
            let mut ld = Vec::new();
            q31_energy::rescale_ld_into(&eraw, &maxsc, f, &mut ld);
            (q, eraw, ld)
        };
        assert_eq!(before.0, after.0, "Q31 map unchanged");
        assert_eq!(before.1, after.1, "raw energies unchanged");
        assert_eq!(before.2, after.2, "Ld unchanged");
    }

    #[test]
    fn xarch_log2_coverage_matrix_complete() {
        // Every production call site from §4 is exercised by at least one
        // layer; negative controls are present and unmodified.
        assert_eq!(COVERAGE.len(), 11);
        let rows_a = layer_a_capture();
        for (site, _, _, _, _, _) in COVERAGE.iter().take(8) {
            assert!(
                rows_a.iter().any(|r| r.call_site == *site),
                "coverage: {site} has Layer-A rows"
            );
        }
        let rows_b = layer_b_capture();
        assert!(!rows_b.is_empty(), "Layer B covers rescale/TNS/MS paths");
        // F-1: the coverage matrix must describe RECORDED Layer-B
        // evidence, not discarded computations. Every claim below fails
        // if the corresponding field stops being captured.
        assert!(
            rows_b.iter().any(|r| r.tns_long_observed && r.ld_long_bits != f64::NAN.to_bits()),
            "TNS-long production Ld retained"
        );
        assert!(
            rows_b.iter().any(|r| r.tns_long_observed && r.e_long_q31 != 0),
            "TNS-long production stored energy retained"
        );
        assert!(
            rows_b.iter().any(|r| r.e_tns_fpe_stored != 0 || f64::from_bits(r.ld_tns_fpe_bits).is_finite()),
            "FPE TNS-long production values retained"
        );
        assert!(
            rows_b.iter().any(|r| r.ms_observed && f64::from_bits(r.ms_mid_ld_bits).is_finite()),
            "MS production Ld retained"
        );
        assert!(
            rows_b.iter().any(|r| r.grouped_observed && r.grp_e_stored != 0),
            "grouped production stored energy retained"
        );
        assert!(
            rows_b.iter().any(|r| r.grouped_observed && f64::from_bits(r.grp_ld_bits).is_finite()),
            "grouped production Ld retained"
        );
        let probes = layer_c_probes();
        assert!(!probes.is_empty(), "Layer C covers boundary families");
        // shift_int is the exact production re-shift used for stored-energy
        // recording context (representation lane, ADR-0005).
        assert_eq!(shift_int(8, -1), 4);
        let _ = psy::MDCT_SCALE_LONG;
    }

    #[test]
    fn xarch_log2_layer_a_output_classes_and_domains() {
        // F-2: comparable rows split into finite-finite, infinite-output,
        // NaN-input and NaN-output classes with exact pinned counts.
        // F-7: negative finite inputs are labelled production-unreachable.
        let rows = layer_a_capture();
        let mut finite_finite = 0usize;
        let mut output_inf = 0usize;
        let mut input_nan = 0usize;
        let mut output_nan = 0usize;
        for r in &rows {
            let iv = r.input_value();
            let lo = r.log2_value();
            if iv.is_nan() {
                input_nan += 1;
                assert_eq!(r.domain, "nan-input");
            } else if lo.is_nan() {
                output_nan += 1;
            } else if lo.is_infinite() {
                output_inf += 1;
            } else {
                finite_finite += 1;
            }
        }
        assert_eq!(input_nan, 32, "4 NaN inputs x 8 sites");
        assert_eq!(output_nan, 32, "4 negative-domain finite inputs x 8 sites");
        assert_eq!(output_inf, 24, "log2(±0)=-inf and log2(+inf)=+inf: 3 inputs x 8 sites");
        assert_eq!(finite_finite, 328, "remaining comparable outputs are finite-finite");
        assert_eq!(input_nan + output_nan + output_inf + finite_finite, rows.len());
        // F-7: reachability labels.
        for r in &rows {
            let iv = r.input_value();
            if iv.is_nan() {
                assert_eq!(r.domain, "nan-input");
            } else if iv.is_infinite() {
                assert_eq!(r.domain, "infinite");
            } else if iv < 0.0 {
                assert_eq!(r.domain, "negative-finite-unreachable", "negative finite {iv:e} unreachable");
            } else if iv == 0.0 {
                assert_eq!(r.domain, "zero-sentinel-path");
            } else {
                assert_eq!(r.domain, "positive-finite");
            }
        }
        assert_eq!(
            rows.iter().filter(|r| r.domain == "negative-finite-unreachable").count(),
            24,
            "3 negative finite inputs (neg_one, neg_min_subnormal, min_finite_neg) x 8 sites stay probed but labelled; neg_inf is infinite-domain"
        );
    }
}
