use std::hint::black_box;
use std::time::Instant;

fn scale_value(peak: i64) -> i32 {
    if peak == 0 {
        0
    } else {
        31 - (64 - (peak as u64).leading_zeros() as i32)
    }
}

fn peak(src: &[i32], start: usize, stop: usize) -> i64 {
    let mut peak: i64 = 0;
    for s in src.iter().take(stop).skip(start) {
        let v = *s as i64;
        peak = peak.max(if v < 0 { -v } else { v });
    }
    peak
}

fn old(dest: &mut [i64], src: &[i32], start: usize, stop: usize) -> i32 {
    let scale = scale_value(peak(src, start, stop));
    for (d, s) in dest.iter_mut().zip(src.iter()).take(stop).skip(start) {
        *d = (*s as i64) << scale;
    }
    scale
}

fn new(dest: &mut [i64], src: &[i32], start: usize, stop: usize) -> i32 {
    let scale = scale_value(peak(src, start, stop));
    for (d, s) in dest[start..stop].iter_mut().zip(&src[start..stop]) {
        *d = (*s as i64) << scale;
    }
    scale
}

fn main() {
    let mut src = vec![0i32; 1024];
    for (i, v) in src.iter_mut().enumerate() {
        *v = (((i as i32).wrapping_mul(2654435761u32 as i32)) >> 5) & 0x0fff_ffff;
    }
    let mut dest = vec![0i64; 1024];
    let iters = 1_000_000u64;
    for (name, f) in [("old", old as fn(&mut [i64], &[i32], usize, usize) -> i32), ("new", new)] {
        for _ in 0..10 {
            for q in 0..4 {
                f(&mut dest, &src, q * 256, (q + 1) * 256);
            }
            black_box(&dest);
        }
        let t = Instant::now();
        for _ in 0..iters {
            for q in 0..4 {
                f(&mut dest, &src, q * 256, (q + 1) * 256);
            }
            black_box(&dest);
        }
        let ns = t.elapsed().as_nanos() as f64 / iters as f64;
        println!("{name}: {ns:.1} ns/frame (4 quarters x 256)");
    }
}
