// Independent scratch verification of R2.1-B owner-review arithmetic.
// Lives OUTSIDE the repository; no repo file is read or modified by this program.
// Semantics transcribed from rust/src/aac_enc/q31.rs (fmult floor) and
// rust/src/aac_enc/intensity.rs (I2 sentinel, direction, is_scale rounding).

fn fmult(a: i32, b: i32) -> i32 {
    (((a as i64) * (b as i64)) >> 31) as i32
}

const R: i32 = 0x5999_9980; // FL2FXCONST_DBL(0.7f)
const REAL_SCALE_MAX: i32 = 0x3C00_0000;
const REAL_SCALE_MIN: i32 = -0x3C00_0000;
const HALF_LSB: i32 = 0x0040_0000;
const DIR_THRESH: i32 = 0x4000_0000 >> (1 + 6 - 2); // 33554432

fn i2_sentinel(e_l: i32, e_r: i32) -> bool {
    e_l >= fmult(R, e_r) && fmult(R, e_l) <= e_r
}

fn is_scale(real: i32) -> i32 {
    if real < 0 {
        ((real >> 1).wrapping_sub(HALF_LSB) >> 23).wrapping_add(1)
    } else {
        ((real >> 1).wrapping_add(HALF_LSB)) >> 23
    }
}

fn real_scale(eld_l: i32, eld_r: i32) -> i32 {
    eld_l.wrapping_sub(eld_r).clamp(REAL_SCALE_MIN, REAL_SCALE_MAX)
}

fn i5(last: i32, real: i32) -> i64 {
    let d = last.wrapping_sub(real);
    let ad = (if d < 0 { d.wrapping_neg() } else { d }) as i64;
    DIR_THRESH as i64 - ad
}

// MS first-pass margin, halved-shift i64 transcription (is_ms.rs:253).
fn ms_margin(eld_l: i32, eld_r: i32, tld_l: i32, tld_r: i32, em_ld: i32, es_ld: i32) -> i64 {
    let min_tld = tld_l.min(tld_r) as i64;
    let tmp = (eld_l as i64).max(tld_l as i64);
    let mut pnlr = (tld_l as i64 >> 1) - (tmp >> 1);
    pnlr += tld_r as i64 >> 1;
    let tmp = (eld_r as i64).max(tld_r as i64);
    pnlr -= tmp >> 1;
    let tmp = (em_ld as i64).max(min_tld);
    let mut pnms = min_tld - (tmp >> 1);
    let tmp = (es_ld as i64).max(min_tld);
    pnms -= tmp >> 1;
    pnms - pnlr - 1
}

fn b_closed(d_el: i64, d_er: i64, d_em: i64, d_es: i64, d_tl: i64, d_tr: i64) -> i64 {
    (d_el + d_er + d_em + d_es + 1) / 2 + d_tl + d_tr + 2 * d_tl.max(d_tr) + 3
}

fn main() {
    println!("== I2 sentinel boundary (M1) ==");
    println!("fmult(0.7,1)={} fmult(0.7,2)={}", fmult(R, 1), fmult(R, 2));
    println!("sentinel(eL=1,eR=0)={} (expect true)", i2_sentinel(1, 0));
    println!("sentinel(eL=2,eR=0)={} (expect false)", i2_sentinel(2, 0));
    println!("sentinel(eL=1,eR=1)={} (expect true, dual-mono)", i2_sentinel(1, 1));

    println!("\n== F-01 quiet f16 b13 margins from owner-reported operands ==");
    let tld = -792142351i32;
    let em = -791899724i32;
    let es = i32::MIN; // both arms
    let mq = ms_margin(-792027707, -792027707, tld, tld, em, es);
    let mf = ms_margin(-791899688, -791899688, tld, tld, em, es);
    let d = 128019i64;
    let b = b_closed(d, d, 0, 0, 0, 0);
    println!("M_q={mq} (owner -6670), M_f={mf} (owner +121350)");
    println!("shift={} (owner 128020), B={b} (owner 128022), slack={}", (mq - mf).abs(), b - (mq - mf).abs());
    println!("use_q={} use_f={} (expect 0/1)", i32::from(mq >= 0), i32::from(mf >= 0));

    println!("\n== F-19 narrow f1 g1/b23: derived dEL from real/eldR deltas ==");
    // real_Q - real_F = (eld_lQ - eld_lF) - (eld_rQ - eld_rF)
    let dreal = 134967593i64 - 124907795;
    let deld_r = -973074615i64 - (-962276304);
    let deld_l = dreal + deld_r; // signed eld_lQ - eld_lF
    println!("dreal={dreal}, deld_r={deld_r}, derived eld_lQ-eld_lF={deld_l}");
    // post-IS: b23 IS-coded both arms -> eld_r = MIN both -> dER=0 for MS operands
    let b19 = b_closed(deld_l.abs(), 0, 0, 0, 0, 0);
    let shift19 = (-57524822i64 - (-57155566)).abs();
    println!("B(dEL={})={b19} (owner 369260), shift={shift19} (owner 369256), slack={}", deld_l.abs(), b19 - shift19);

    println!("\n== is_scale rounding (M4), owner 'units' = (real>>1 + HALF_LSB)/2^23 ==");
    for (name, real, want) in [
        ("F-19 Q", 134967593i32, 8), ("F-19 F", 124907795, 7),
        ("F-20 F", 120287460, 7), ("F-20 Q clamp", REAL_SCALE_MAX, 9),
        ("F-23 Q", 124162236, 7), ("F-23 F", 127749717, 8),
        ("F-24 Q", 145016039, 9), ("F-24 F", 125391875, 7),
        ("F-25 Q", 129448447, 8), ("F-25 F", 119477657, 7),
        ("F-26 F", 116075350, 7),
    ] {
        let units = ((real >> 1) + HALF_LSB) as f64 / 8388608.0;
        println!("{name}: real={real} is_scale={} (want {want}) rounded-units={units:.4}", is_scale(real));
    }
    println!("MAX clamp value = {REAL_SCALE_MAX} (owner 1006632960)");
    println!("one is_scale LSB in real units = {}", 1i64 << 24);

    println!("\n== F-20 direction margins (M3) ==");
    println!("Q: last=134967593 real=MAX -> i5={} (owner -838110935)", i5(134967593, REAL_SCALE_MAX));
    println!("F: last=124907795 real=120287460 -> i5={} (owner +28934097)", i5(124907795, 120287460));
    println!("F-26 Q: last=129448447 real=MAX -> i5={}", i5(129448447, REAL_SCALE_MAX));
    println!("F-26 F: last=119477657 real=116075350 -> i5={} (owner ~30M)", i5(119477657, 116075350));
    println!("direction threshold = {DIR_THRESH} (owner 33.5M)");

    println!("\n== F-20 Q MAX-clamp via wrapping subtraction with eldR=MIN ==");
    // any eld_l > -1140850688 clamps to MAX when eld_r = MIN
    let eld_l_example = -973074615i32;
    println!("real_scale({eld_l_example}, MIN) = {} (expect MAX {REAL_SCALE_MAX})", real_scale(eld_l_example, i32::MIN));
    println!("F-20 F finite: eld_l = 120287460 + (-1040183479) = {}", 120287460i64 + (-1040183479));
    println!("calc_ld_data(1) ideal = (log2(1)-31)/64*2^31 = {:.0} (owner finite Ld -1040183479, Taylor approx)", (-31.0f64 / 64.0) * 2147483648.0);

    println!("\n== Energy-ratio formulas (owner F-19/F-24) ==");
    for (name, deld) in [("F-19", 10.8e6f64), ("F-24", 19.6e6f64)] {
        let ratio_correct = (64.0 * deld / 2147483648.0).exp2();
        let ratio_owner_literal = (deld / 2147.5e6).exp2(); // owner writes 2^(10.8/2147.5) with deld in M
        println!("{name}: deld={deld:.0} -> 2^(64*deld/2^31)={ratio_correct:.4} ({} dB); owner-literal 2^(deld_M/2147.5)={ratio_owner_literal:.6}",
            10.0 * ratio_correct.log10());
    }
    println!("owner claims: F-19 ~1.25x ~1.0 dB; F-24 ~1.50x ~1.8 dB");

    println!("\n== F-07 carry consistency check ==");
    // owner: CarryUpdate i5=31646420 at b13, FPE real=132990385, origin (0,5)
    let implied_dev = DIR_THRESH as i64 - 31646420;
    println!("|last-real| implied by i5=31646420: {implied_dev}");
    println!("b5 FPE real=135174056 vs b13 real=132990385 dev={}", (135174056i64 - 132990385).abs());
    println!("(differ => carry must have been updated by an intermediate extended band; not a contradiction)");
}
