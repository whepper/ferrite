use std::hint::black_box;
use std::time::Instant;

const NO_SCF: i32 = i32::MIN;

#[inline(never)]
fn scan_old(scf: &[i32]) -> (i32, i32) {
    let mut smin = i32::MAX;
    let mut smax = NO_SCF;
    for &v in scf {
        if v != NO_SCF {
            smin = smin.min(v);
            smax = smax.max(v);
        }
    }
    (smin, smax)
}

#[inline(never)]
fn scan_new(scf: &[i32]) -> (i32, i32) {
    let mut smin = i32::MAX;
    let mut smax = NO_SCF;
    for &v in scf {
        smin = smin.min(if v == NO_SCF { i32::MAX } else { v });
        smax = smax.max(v);
    }
    (smin, smax)
}

fn run(name: &str, f: fn(&[i32]) -> (i32, i32), data: &[i32], rounds: usize) -> u128 {
    let mut acc = 0i64;
    let start = Instant::now();
    for _ in 0..rounds {
        let (a, b) = f(black_box(data));
        acc = acc.wrapping_add(black_box(a) as i64).wrapping_add(black_box(b) as i64);
    }
    let elapsed = start.elapsed().as_nanos();
    println!("{name}: {elapsed} ns total, {:.3} ns/scan, acc={acc}", elapsed as f64 / rounds as f64);
    elapsed
}

fn main() {
    let n = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20_000_000);
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut data = vec![0i32; 49];
    for v in data.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let x = ((state >> 33) % 90) as i32 - 50;
        *v = if x % 7 == 0 { NO_SCF } else { x };
    }
    println!("old={:?} new={:?}", scan_old(&data), scan_new(&data));
    run("old-warm", scan_old, &data, 1_000_000);
    run("new-warm", scan_new, &data, 1_000_000);
    let a = run("old", scan_old, &data, n);
    let b = run("new", scan_new, &data, n);
    println!("new {:.3}x", a as f64 / b as f64);
}
