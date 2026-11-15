//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Package: ferrite-aac (package license FDK-AAC);
//! see rust/NOTICE for the full license text.
//!
//! Phase 2.4.2 test-only SIMD kernel benchmark matrix and production-weighted
//! aggregate (approved plan §5, §5.1, §7 items 2 and 8).
//!
//! Compiled only under `cfg(test)`; no production path reaches this module and
//! no public API is added. The arch benchmark tests in `quant_neon.rs` /
//! `quant_avx2.rs` pass their backend's kernel entry and call [`run`].
//!
//! Measurement discipline: ≥1,000 mixed warm-up passes per cell; ≥15 outer
//! repetitions with alternating scalar/SIMD order; `black_box` on inputs and
//! outputs; median + IQR over repetitions; raw per-repetition observations
//! printed. Correctness (complete output buffers + all returned values,
//! sentinel-prefilled where the callee may leave an early-out tail) is
//! verified once per cell *before* any timing.
//!
//! Run with `--release`: the production quantizer envelope assumes release
//! shift semantics, and debug assertions would abort timing-only cells.
//!
//! These are diagnostic kernel measurements, not the formal end-to-end
//! performance acceptance campaign (plan §6, §8). No acceptance decision is
//! made here.
//!
//! Phase 2.4.2b build isolation: the timing matrix refuses to run in a
//! `--cfg fallback_census` observation build (see [`assert_timing_build_allowed`]),
//! because that build compiles intra-kernel fallback instrumentation into
//! every timed arm. `BENCH_CONFIG` self-identifies the build
//! (`fallback_census=true/false`) before any timing record. The arch/OS
//! labels are descriptive metadata, not proof of native hardware execution;
//! native execution evidence is documented separately in the performance
//! baseline.
//!
//! `inv_quantize` coverage: there is no standalone benchmark (it has no
//! independent production call path). It is timed and asserted through the
//! dist (`calc_sfb_dist_sum_*` → `inv_quantize4/8`) and energy
//! (`calc_sfb_quant_energy_and_dist_sums_*` → `inv_quantize4/8`) arms below.

use super::q31::calc_ld_data;
use super::quant::{
    calc_sfb_dist_scalar, calc_sfb_quant_energy_and_dist_scalar, kernel_census, quantize_line,
    quantize_lines_scalar, selected_backend, Backend, MAX_QUANT,
};
use super::quant_tables::{QUANT_K0, QUANT_K1, QUANT_TABLE_Q};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

use kernel_census::Kernel;

/// Matrix sizes required by the approved plan §5.
const SIZES: [usize; 11] = [1, 4, 7, 8, 9, 14, 16, 17, 32, 33, 64];
/// Quantize + dist matrix gains (residue-complete, plan §5).
const MATRIX_QD_GAINS: [i32; 9] = [-72, -71, -70, -69, -32, -8, 0, 32, 64];
/// Energy matrix gains (all four mantissa rows, plan §5).
const MATRIX_ENERGY_GAINS: [i32; 10] = [-72, -71, -70, -69, 0, 1, 2, 3, 32, 64];

/// Outer measurement repetitions (plan §7 item 2: ≥15).
const REPS: usize = 15;
/// Mixed warm-up passes per cell (plan §7 item 1: ≥1,000).
const WARMUP_PASSES: usize = 1_000;
/// Timed calls per repetition. Chosen so even a 1-line call is measurable by
/// `Instant` (batch ≥ a few µs) while the full matrix stays practical.
const ITERS: usize = 1_024;

/// Prefill proving unwritten lanes are detected on early-out.
const SENTINEL: i16 = 0x5555u16 as i16;

/// SIMD backend under measurement. The three fields are the exact production
/// kernel entries (`quantize_lines_*`, `calc_sfb_dist_sum_*`,
/// `calc_sfb_quant_energy_and_dist_sums_*`); the harness applies the same
/// post-processing the production wrapper does. Calling the kernels directly
/// (plan §4.3) keeps the timed arm free of the test-only census tap and of the
/// per-call runtime feature check, so the comparison is arithmetic-for-
/// arithmetic; correctness is still asserted against the full production
/// output shape.
pub(crate) type QuantizeFn = unsafe fn(&[i32], &mut [i16], i32, i32, i32);
pub(crate) type DistFn = unsafe fn(&[i32], &mut [i16], i32, i32, i32) -> Option<i32>;
pub(crate) type EnergyFn = unsafe fn(&[i32], &[i16], i32) -> Option<(i32, i32)>;

pub(crate) struct SimdArm {
    pub backend: Backend,
    pub group: usize,
    pub quantize: QuantizeFn,
    pub dist: DistFn,
    pub energy: EnergyFn,
}

/// Production wrapper post-processing for the dist sum.
fn dist_final(sum: Option<i32>) -> i32 {
    match sum {
        Some(xfsf) => calc_ld_data(xfsf),
        None => 0,
    }
}

/// Production wrapper post-processing for the energy/dist sums.
fn energy_final(sums: Option<(i32, i32)>) -> (i32, i32) {
    match sums {
        Some((energy, distortion)) => (
            calc_ld_data(energy).wrapping_add(0x0400_0000),
            calc_ld_data(distortion),
        ),
        None => (0, 0),
    }
}

/// One benchmark cell key. Joint — dimensions are never recombined.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Key {
    kernel: Kernel,
    n: usize,
    gain: i32,
    dz: Option<bool>,
    /// `Some(line)` = deliberate `MAX_QUANT` early-out at that line.
    early_pos: Option<usize>,
    /// `Some(name)` = synthetic edge class (never enters the aggregate).
    synthetic: Option<&'static str>,
}

impl Key {
    fn residue(self) -> i32 {
        self.gain.wrapping_neg() & 3
    }
}

struct Cell {
    key: Key,
    matrix: bool,
    production: bool,
    spec: Vec<i32>,
    quant: Vec<i16>,
    skip: Option<&'static str>,
}

struct Measurement {
    key: Key,
    scalar_raw: Vec<f64>,
    simd_raw: Vec<f64>,
    scalar_med: f64,
    simd_med: f64,
    scalar_iqr: f64,
    simd_iqr: f64,
    early_pos: Option<usize>,
    classes: String,
}

/// Phase 2.4.2b: mechanical build isolation for the timing matrix.
///
/// A `--cfg fallback_census` build compiles the Phase 2.4.2a intra-kernel
/// fallback tallies and the per-call census write into every kernel. Timing
/// that build would measure the instrumentation, so the diagnostic matrix
/// refuses to run in it — before any capture or timing begins. `cfg!` is a
/// compile-time constant, so normal builds carry no runtime branch and their
/// execution is unchanged. The fallback-census capture tests
/// (`simd_quant_census_corpus`, `fallback_census_*`) do not pass through this
/// guard and remain usable in the observation build.
fn assert_timing_build_allowed() {
    if cfg!(fallback_census) {
        panic!(
            "quant_bench: timing matrix refused: build carries --cfg fallback_census \
             (intra-kernel fallback instrumentation would contaminate every timed arm). \
             Rebuild without --cfg fallback_census for timing; the fallback-census build \
             is for the capture tests only (simd_quant_census_corpus, fallback_census_*)."
        );
    }
}

/// Run the full Phase 2.4.2 matrix for one backend.
///
/// Phase 2.4.2b: refuses immediately in a `--cfg fallback_census` build
/// (see [`assert_timing_build_allowed`]); normal builds are unaffected.
pub(crate) fn run(arm: SimdArm) {
    assert_timing_build_allowed();
    assert_eq!(
        selected_backend(),
        arm.backend,
        "benchmark backend must match the production dispatch of this machine"
    );

    let captured = super::stream_driver::capture_simd_quant_distribution();

    let mut cells: BTreeMap<Key, Cell> = BTreeMap::new();
    add_matrix_cells(&mut cells);
    add_production_cells(&mut cells, &captured);
    add_synthetic_cells(&mut cells, arm.group);

    println!(
        "BENCH_CONFIG arch={} os={} profile={} fallback_census={} backend={} group={} reps={REPS} warmup={WARMUP_PASSES} iters={ITERS} sizes={:?} matrix_qd_gains={:?} matrix_energy_gains={:?} cells={} captured_cells={} harness=quant_bench corpus=M58_FIXTURES fixtures=19 vbr=5 mode=encoder-only",
        std::env::consts::ARCH,
        std::env::consts::OS,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        cfg!(fallback_census),
        arm.backend.tag(),
        arm.group,
        SIZES,
        MATRIX_QD_GAINS,
        MATRIX_ENERGY_GAINS,
        cells.len(),
        captured.len(),
    );

    let mut measured: BTreeMap<Key, Measurement> = BTreeMap::new();
    for cell in cells.values() {
        if let Some(reason) = cell.skip {
            println!(
                "BENCH_SKIP backend={} kernel={} n={} gain={} residue={} dz={} class={} reason={reason}",
                arm.backend.tag(),
                cell.key.kernel.tag(),
                cell.key.n,
                cell.key.gain,
                cell.key.residue(),
                dz_tag(cell.key.dz),
                class_tag(cell),
            );
            continue;
        }
        let m = run_cell(cell, &arm);
        print_measurement(&m, arm.backend);
        measured.insert(m.key, m);
    }

    print_aggregates(&captured, &measured, arm.backend);
}

fn add_matrix_cells(cells: &mut BTreeMap<Key, Cell>) {
    for &n in &SIZES {
        for &gain in &MATRIX_QD_GAINS {
            for &dz in &[false, true] {
                for kernel in [Kernel::Quantize, Kernel::Dist] {
                    insert(
                        cells,
                        Key {
                            kernel,
                            n,
                            gain,
                            dz: Some(dz),
                            early_pos: None,
                            synthetic: None,
                        },
                        true,
                        false,
                    );
                }
            }
        }
        for &gain in &MATRIX_ENERGY_GAINS {
            insert(
                cells,
                Key {
                    kernel: Kernel::Energy,
                    n,
                    gain,
                    dz: None,
                    early_pos: None,
                    synthetic: None,
                },
                true,
                false,
            );
        }
    }
}

fn add_production_cells(cells: &mut BTreeMap<Key, Cell>, captured: &[(kernel_census::Cell, u64)]) {
    for (c, _) in captured {
        insert(
            cells,
            Key {
                kernel: c.kernel,
                n: c.n,
                gain: c.gain,
                dz: c.dz,
                early_pos: if c.early_out { c.early_pos } else { None },
                synthetic: None,
            },
            false,
            true,
        );
    }
}

fn add_synthetic_cells(cells: &mut BTreeMap<Key, Cell>, group: usize) {
    for &n in &SIZES {
        for class in ["zero", "extremes", "loud", "unit", "pow16"] {
            for (kernel, gain) in [
                (Kernel::Quantize, 0),
                (Kernel::Dist, 0),
                (Kernel::Energy, -32),
            ] {
                insert(
                    cells,
                    Key {
                        kernel,
                        n,
                        gain,
                        dz: if kernel == Kernel::Energy {
                            None
                        } else {
                            Some(false)
                        },
                        early_pos: None,
                        synthetic: Some(class),
                    },
                    false,
                    false,
                );
            }
        }
    }
    // Energy quantizer boundaries (plan §5): exactly the caller-supplied
    // quant values that exercise the valid edge and the early-out.
    for &n in &SIZES {
        for boundary in ["b8191", "b8192", "bm8192", "bint_min", "b0"] {
            insert(
                cells,
                Key {
                    kernel: Kernel::Energy,
                    n,
                    gain: -32,
                    dz: None,
                    early_pos: None,
                    synthetic: Some(boundary),
                },
                false,
                false,
            );
        }
    }
    // Dist early-outs at group positions 0, 1, 2 and the last full group,
    // kept separate from clean cells (plan §5).
    for &n in &[8usize, 9, 16, 17, 32, 33, 64] {
        let groups = n / group;
        let mut positions: Vec<usize> = vec![0, 1, 2];
        if groups > 3 {
            positions.push(groups - 1);
        }
        positions.sort_unstable();
        positions.dedup();
        for group_idx in positions {
            let line = group_idx * group;
            if line >= n {
                continue;
            }
            for &gain in &[-73i32, -72] {
                insert(
                    cells,
                    Key {
                        kernel: Kernel::Dist,
                        n,
                        gain,
                        dz: Some(false),
                        early_pos: Some(line),
                        synthetic: Some("early_out"),
                    },
                    false,
                    false,
                );
            }
        }
    }
}

fn insert(cells: &mut BTreeMap<Key, Cell>, mut key: Key, matrix: bool, production: bool) {
    if let Some(cell) = cells.get_mut(&key) {
        cell.matrix |= matrix;
        cell.production |= production;
        return;
    }
    let (mut spec, mut quant, mut skip) = build_inputs(&key);
    // A gain whose quantizer shift puts every non-zero line above MAX_QUANT
    // has no clean behaviour to time. Measure the only reachable behaviour
    // (early-out at line 0) and label it as such rather than dropping the
    // required matrix cell.
    if skip.is_some()
        && key.kernel == Kernel::Dist
        && key.synthetic.is_none()
        && key.early_pos.is_none()
    {
        if let Some(early) = early_out_spec(key.n, key.gain, key.dz.unwrap(), 0) {
            key.early_pos = Some(0);
            spec = early;
            quant.clear();
            skip = None;
        }
    }
    cells.insert(
        key,
        Cell {
            key,
            matrix,
            production,
            spec,
            quant,
            skip,
        },
    );
}

fn build_inputs(key: &Key) -> (Vec<i32>, Vec<i16>, Option<&'static str>) {
    let n = key.n;
    let synthetic = key.synthetic;
    match key.kernel {
        Kernel::Quantize => (synth_spec(n, synthetic, key.gain), Vec::new(), None),
        Kernel::Dist => {
            if let Some(pos) = key.early_pos {
                match early_out_spec(n, key.gain, key.dz.unwrap(), pos) {
                    Some(spec) => (spec, Vec::new(), None),
                    None => (
                        Vec::new(),
                        Vec::new(),
                        Some("no spectrum produces the requested early-out"),
                    ),
                }
            } else if synthetic.is_some() {
                (synth_spec(n, synthetic, key.gain), Vec::new(), None)
            } else {
                match clean_dist_spec(n, key.gain, key.dz.unwrap()) {
                    Some(spec) => (spec, Vec::new(), None),
                    None => (
                        Vec::new(),
                        Vec::new(),
                        Some("no clean spectrum inside the totalShift envelope"),
                    ),
                }
            }
        }
        Kernel::Energy => {
            if synthetic.is_some() {
                let spec = synth_spec(n, synthetic, key.gain);
                let quant = synth_quant(n, synthetic);
                (spec, quant, None)
            } else {
                (
                    lcg_spec(n, seed(key), 8),
                    uniform_quant(n, seed(key).wrapping_add(1)),
                    None,
                )
            }
        }
    }
}

fn seed(key: &Key) -> u32 {
    (key.n as u32)
        .wrapping_mul(2_654_435_761)
        .wrapping_add(key.gain as u32)
        .wrapping_add(key.dz.unwrap_or(false) as u32)
        .wrapping_add(key.early_pos.unwrap_or(0) as u32)
}

fn lcg_spec(n: usize, seed: u32, shift: u32) -> Vec<i32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            ((s as i32) >> shift.min(31)).wrapping_add(1)
        })
        .collect()
}

fn uniform_quant(n: usize, seed: u32) -> Vec<i16> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            ((s % 8191) as i16) - 4095
        })
        .collect()
}

fn clean_dist_spec(n: usize, gain: i32, dz: bool) -> Option<Vec<i32>> {
    for shift in (2..=30).step_by(2) {
        let spec = lcg_spec(
            n,
            seed(&Key {
                kernel: Kernel::Dist,
                n,
                gain,
                dz: Some(dz),
                early_pos: None,
                synthetic: None,
            }),
            shift,
        );
        let mut q = vec![0i16; n];
        let _ = calc_sfb_dist_scalar(&spec, gain, dz, &mut q);
        let clean = !q.iter().any(|&x| (x as i32).abs() > MAX_QUANT);
        if clean {
            return Some(spec);
        }
    }
    None
}

fn early_out_spec(n: usize, gain: i32, dz: bool, pos: usize) -> Option<Vec<i32>> {
    if pos >= n {
        return None;
    }
    for loud in [
        i32::MAX,
        0x7000_0000,
        0x4000_0000,
        0x2000_0000,
        0x1000_0000,
        0x0800_0000,
        0x0400_0000,
    ] {
        let mut spec = vec![0i32; n];
        spec[pos] = loud;
        let mut q = vec![0i16; n];
        let _ = calc_sfb_dist_scalar(&spec, gain, dz, &mut q);
        if q.iter().position(|&x| (x as i32).abs() > MAX_QUANT) == Some(pos) {
            return Some(spec);
        }
    }
    None
}

fn synth_spec(n: usize, class: Option<&'static str>, _gain: i32) -> Vec<i32> {
    match class.unwrap_or("none") {
        "zero" => vec![0i32; n],
        "extremes" => (0..n)
            .map(|i| if i % 2 == 0 { i32::MIN } else { i32::MAX })
            .collect(),
        "loud" => vec![0x7000_0000i32; n],
        "unit" => (0..n).map(|i| if i % 2 == 0 { 1 } else { -1 }).collect(),
        "pow16" => (0..n)
            .map(|i| if i % 2 == 0 { 65536 } else { -65536 })
            .collect(),
        _ => uniform_quant(n, 0x5EED_2026)
            .into_iter()
            .map(i32::from)
            .collect(),
    }
}

fn synth_quant(n: usize, class: Option<&'static str>) -> Vec<i16> {
    let value: i32 = match class.unwrap_or("none") {
        "zero" | "b0" => 0,
        "extremes" | "bint_min" => i16::MIN as i32,
        "loud" | "b8191" => 8191,
        "unit" => 1,
        "pow16" | "b8192" => 8192,
        "bm8192" => -8192,
        _ => 0,
    };
    vec![value as i16; n]
}

fn run_cell(cell: &Cell, arm: &SimdArm) -> Measurement {
    let key = cell.key;
    let n = key.n;
    let qs = key.gain.wrapping_neg();
    let quantizer = QUANT_TABLE_Q[(qs & 3) as usize];
    let shift = (qs >> 2) + 1;
    let k = if key.dz.unwrap_or(false) {
        QUANT_K1
    } else {
        QUANT_K0
    };
    let classes = class_tag(cell);
    let msg = |what: &str| {
        format!(
            "backend={} kernel={} n={} gain={} residue={} dz={} class={} check={what}",
            arm.backend.tag(),
            key.kernel.tag(),
            n,
            key.gain,
            key.residue(),
            dz_tag(key.dz),
            classes,
        )
    };

    let spec = &cell.spec;
    let (scalar_raw, simd_raw, early_pos) = match key.kernel {
        Kernel::Quantize => {
            let mut q_s = vec![SENTINEL; n];
            let mut q_m = vec![SENTINEL; n];
            quantize_lines_scalar(spec, &mut q_s, quantizer, shift, k);
            unsafe {
                (arm.quantize)(spec, &mut q_m, key.gain, shift, k);
            }
            assert_eq!(q_m, q_s, "{}", msg("quantize full buffer"));
            let q_prod: Vec<i16> = spec
                .iter()
                .map(|&s| quantize_line(key.gain, s, key.dz.unwrap()))
                .collect();
            assert_eq!(q_s, q_prod, "{}", msg("production gain mapping"));

            let mut scalar = || {
                let _ = black_box(spec);
                quantize_lines_scalar(
                    black_box(spec),
                    black_box(&mut q_s),
                    black_box(quantizer),
                    black_box(shift),
                    black_box(k),
                );
                let _ = black_box(&q_s);
            };
            let mut simd = || {
                let _ = black_box(spec);
                unsafe {
                    (arm.quantize)(
                        black_box(spec),
                        black_box(&mut q_m),
                        black_box(key.gain),
                        black_box(shift),
                        black_box(k),
                    );
                }
                let _ = black_box(&q_m);
            };
            let (s, m) = measure_pair(&mut scalar, &mut simd);
            (s, m, None)
        }
        Kernel::Dist => {
            let mut q_s = vec![SENTINEL; n];
            let mut q_m = vec![SENTINEL; n];
            let r_s = calc_sfb_dist_scalar(spec, key.gain, key.dz.unwrap(), &mut q_s);
            let r_m = dist_final(unsafe { (arm.dist)(spec, &mut q_m, key.gain, shift, k) });
            assert_eq!(
                (r_m, &q_m),
                (r_s, &q_s),
                "{}",
                msg("dist return + full buffer")
            );
            let early_pos = q_s.iter().position(|&q| (q as i32).abs() > MAX_QUANT);
            match key.early_pos {
                Some(p) => assert_eq!(
                    early_pos,
                    Some(p),
                    "{}",
                    msg("requested early-out position")
                ),
                None if key.synthetic.is_none() => assert!(
                    early_pos.is_none(),
                    "{}",
                    msg("matrix/production dist cell must be clean")
                ),
                None => {}
            }

            let mut scalar = || {
                let _ = black_box(spec);
                let r = calc_sfb_dist_scalar(
                    black_box(spec),
                    black_box(key.gain),
                    black_box(key.dz.unwrap()),
                    black_box(&mut q_s),
                );
                let _ = black_box(r);
                let _ = black_box(&q_s);
            };
            let mut simd = || {
                let _ = black_box(spec);
                let r = dist_final(unsafe {
                    (arm.dist)(
                        black_box(spec),
                        black_box(&mut q_m),
                        black_box(key.gain),
                        black_box(shift),
                        black_box(k),
                    )
                });
                let _ = black_box(r);
                let _ = black_box(&q_m);
            };
            let (s, m) = measure_pair(&mut scalar, &mut simd);
            (s, m, early_pos)
        }
        Kernel::Energy => {
            let quant = &cell.quant;
            let r_s = calc_sfb_quant_energy_and_dist_scalar(spec, quant, key.gain);
            let r_m = energy_final(unsafe { (arm.energy)(spec, quant, key.gain) });
            assert_eq!(r_m, r_s, "{}", msg("energy + dist return"));
            let early_pos = quant.iter().position(|&q| (q as i32).abs() > MAX_QUANT);
            match key.early_pos {
                Some(p) => assert_eq!(
                    early_pos,
                    Some(p),
                    "{}",
                    msg("requested energy early-out position")
                ),
                None if key.synthetic.is_none() => assert!(
                    early_pos.is_none(),
                    "{}",
                    msg("matrix/production energy cell must be clean")
                ),
                None => {}
            }

            let mut scalar = || {
                let _ = black_box(spec);
                let _ = black_box(quant);
                let r = calc_sfb_quant_energy_and_dist_scalar(
                    black_box(spec),
                    black_box(quant),
                    black_box(key.gain),
                );
                let _ = black_box(r);
            };
            let mut simd = || {
                let _ = black_box(spec);
                let _ = black_box(quant);
                let r = energy_final(unsafe {
                    (arm.energy)(black_box(spec), black_box(quant), black_box(key.gain))
                });
                let _ = black_box(r);
            };
            let (s, m) = measure_pair(&mut scalar, &mut simd);
            (s, m, early_pos)
        }
    };

    let (scalar_med, scalar_iqr) = summarize(&scalar_raw);
    let (simd_med, simd_iqr) = summarize(&simd_raw);
    Measurement {
        key,
        scalar_raw,
        simd_raw,
        scalar_med,
        simd_med,
        scalar_iqr,
        simd_iqr,
        early_pos,
        classes,
    }
}

fn measure_pair<S: FnMut(), M: FnMut()>(scalar: &mut S, simd: &mut M) -> (Vec<f64>, Vec<f64>) {
    for _ in 0..WARMUP_PASSES {
        scalar();
        simd();
    }
    let mut scalar_raw = Vec::with_capacity(REPS);
    let mut simd_raw = Vec::with_capacity(REPS);
    for rep in 0..REPS {
        // Alternate arm order per repetition (plan §5, §7 item 8).
        if rep % 2 == 0 {
            scalar_raw.push(timed(scalar));
            simd_raw.push(timed(simd));
        } else {
            simd_raw.push(timed(simd));
            scalar_raw.push(timed(scalar));
        }
    }
    (scalar_raw, simd_raw)
}

fn timed(f: &mut impl FnMut()) -> f64 {
    let t = Instant::now();
    for _ in 0..ITERS {
        f();
    }
    t.elapsed().as_nanos() as f64 / ITERS as f64
}

/// Median and interquartile range over the outer repetitions. The IQR uses
/// the lower/upper halves excluding the median for odd counts (Tukey's
/// hinges); it is a descriptive dispersion, not a confidence interval.
fn summarize(raw: &[f64]) -> (f64, f64) {
    let mut sorted = raw.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = median_sorted(&sorted);
    let n = sorted.len();
    let q1 = median_sorted(&sorted[..n / 2]);
    let q3 = median_sorted(&sorted[n.div_ceil(2)..]);
    (median, q3 - q1)
}

fn median_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

fn print_measurement(m: &Measurement, backend: Backend) {
    let key = m.key;
    let n = key.n as f64;
    println!(
        "BENCH_CELL backend={} kernel={} n={} gain={} residue={} dz={} early_out={} early_pos={} class={} scalar_med_ns={:.3} simd_med_ns={:.3} scalar_iqr_ns={:.3} simd_iqr_ns={:.3} scalar_ns_per_line={:.4} simd_ns_per_line={:.4} simd_over_scalar={:.4} speedup={:.4}",
        backend.tag(),
        key.kernel.tag(),
        key.n,
        key.gain,
        key.residue(),
        dz_tag(key.dz),
        m.early_pos.is_some(),
        m.early_pos
            .map(|p| p.to_string())
            .unwrap_or_else(|| "none".into()),
        m.classes,
        m.scalar_med,
        m.simd_med,
        m.scalar_iqr,
        m.simd_iqr,
        m.scalar_med / n,
        m.simd_med / n,
        m.simd_med / m.scalar_med.max(f64::MIN_POSITIVE),
        m.scalar_med / m.simd_med.max(f64::MIN_POSITIVE),
    );
    println!(
        "BENCH_RAW backend={} kernel={} n={} gain={} dz={} class={} scalar={:?} simd={:?}",
        backend.tag(),
        key.kernel.tag(),
        key.n,
        key.gain,
        dz_tag(key.dz),
        m.classes,
        m.scalar_raw,
        m.simd_raw,
    );
}

fn print_aggregates(
    captured: &[(kernel_census::Cell, u64)],
    measured: &BTreeMap<Key, Measurement>,
    backend: Backend,
) {
    struct Agg {
        cells: u64,
        captured_calls: u64,
        covered_calls: u64,
        t_scalar: f64,
        t_simd: f64,
    }
    let mut agg: BTreeMap<Kernel, Agg> = BTreeMap::new();
    for kernel in [Kernel::Quantize, Kernel::Dist, Kernel::Energy] {
        agg.insert(
            kernel,
            Agg {
                cells: 0,
                captured_calls: 0,
                covered_calls: 0,
                t_scalar: 0.0,
                t_simd: 0.0,
            },
        );
    }
    let mut uncovered: Vec<(kernel_census::Cell, u64, &'static str)> = Vec::new();
    for (c, calls) in captured {
        let key = Key {
            kernel: c.kernel,
            n: c.n,
            gain: c.gain,
            dz: c.dz,
            early_pos: if c.early_out { c.early_pos } else { None },
            synthetic: None,
        };
        let a = agg.get_mut(&c.kernel).unwrap();
        a.captured_calls += *calls;
        match measured.get(&key) {
            Some(m) => {
                a.cells += 1;
                a.covered_calls += *calls;
                a.t_scalar += *calls as f64 * m.scalar_med;
                a.t_simd += *calls as f64 * m.simd_med;
                println!(
                    "BENCH_AGG_CELL backend={} kernel={} n={} gain={} residue={} dz={} early_out={} early_pos={} calls={} scalar_med_ns={:.3} simd_med_ns={:.3} scalar_contrib_ns={:.1} simd_contrib_ns={:.1}",
                    backend.tag(),
                    c.kernel.tag(),
                    c.n,
                    c.gain,
                    c.residue(),
                    dz_tag(c.dz),
                    c.early_out,
                    c.early_pos
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "none".into()),
                    calls,
                    m.scalar_med,
                    m.simd_med,
                    *calls as f64 * m.scalar_med,
                    *calls as f64 * m.simd_med,
                );
            }
            None => uncovered.push((*c, *calls, "cell not measured (see BENCH_SKIP)")),
        }
    }
    for (kernel, a) in &agg {
        let ratio = if a.t_scalar > 0.0 {
            a.t_simd / a.t_scalar
        } else {
            f64::NAN
        };
        let d = ratio.ln();
        println!(
            "BENCH_AGG backend={} kernel={} captured_cells={} measured_cells={} captured_calls={} covered_calls={} coverage={:.4} T_scalar_ns={:.1} T_simd_ns={:.1} time_ratio={:.6} d_log={:.6} time_change_pct={:.4} throughput_pct={:.4}",
            backend.tag(),
            kernel.tag(),
            captured.iter().filter(|(c, _)| c.kernel == *kernel).count(),
            a.cells,
            a.captured_calls,
            a.covered_calls,
            if a.captured_calls > 0 {
                a.covered_calls as f64 / a.captured_calls as f64
            } else {
                f64::NAN
            },
            a.t_scalar,
            a.t_simd,
            ratio,
            d,
            (ratio - 1.0) * 100.0,
            ((-d).exp() - 1.0) * 100.0,
        );
    }
    for (c, calls, reason) in uncovered {
        println!(
            "BENCH_UNCOVERED backend={} kernel={} n={} gain={} residue={} dz={} early_out={} early_pos={} calls={} reason={reason}",
            backend.tag(),
            c.kernel.tag(),
            c.n,
            c.gain,
            c.residue(),
            dz_tag(c.dz),
            c.early_out,
            c.early_pos
                .map(|p| p.to_string())
                .unwrap_or_else(|| "none".into()),
            calls,
        );
    }
}

fn class_tag(cell: &Cell) -> String {
    let mut out = String::new();
    if cell.matrix {
        out.push_str("matrix");
    }
    if cell.production {
        if !out.is_empty() {
            out.push('+');
        }
        out.push_str("production");
    }
    if let Some(synthetic) = cell.key.synthetic {
        if !out.is_empty() {
            out.push('+');
        }
        out.push_str("synthetic:");
        out.push_str(synthetic);
    }
    out
}

fn dz_tag(dz: Option<bool>) -> String {
    dz.map(|d| d.to_string()).unwrap_or_else(|| "none".into())
}

/// Phase 2.4.2b: focused checks for the timing-build guard. Exactly one of
/// these tests is compiled per build variant, and neither runs the timing
/// matrix: the normal build pins that the observation cfg is absent and the
/// guard passes; the `--cfg fallback_census` build pins that the guard
/// refuses with the documented message. End-to-end, the ignored
/// `*_kernel_microbench` tests execute the matrix in a normal build and fail
/// at this same guard in an observation build.
mod tests {
    /// Normal diagnostic build: no fallback instrumentation is compiled in,
    /// and the guard must pass silently so the matrix can run.
    #[cfg(not(fallback_census))]
    #[test]
    fn timing_guard_allows_normal_build() {
        const {
            assert!(
                !cfg!(fallback_census),
                "normal diagnostic build must not carry --cfg fallback_census"
            );
        }
        super::assert_timing_build_allowed();
    }

    /// Observation build: the guard must fail immediately and name the cfg.
    /// `run()` calls it before any capture or timing, so the timing matrix
    /// cannot execute in this build.
    #[cfg(fallback_census)]
    #[test]
    #[should_panic(expected = "timing matrix refused")]
    fn timing_guard_refuses_fallback_census_build() {
        super::assert_timing_build_allowed();
    }
}
