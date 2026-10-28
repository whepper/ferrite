use std::hint::black_box;
use std::time::Instant;

fn old(dest: &mut [i64], src: &[i32], start: usize, stop: usize, scale: i32) {
    for (d, s) in dest.iter_mut().zip(src.iter()).take(stop).skip(start) {
        *d = (*s as i64) << scale;
    }
}

fn new(dest: &mut [i64], src: &[i32], start: usize, stop: usize, scale: i32) {
    for (d, s) in dest[start..stop].iter_mut().zip(&src[start..stop]) {
        *d = (*s as i64) << scale;
    }
}

fn main() {
    let mut src = vec![0i32; 1024];
    for (i, v) in src.iter_mut().enumerate() {
        *v = ((i as i32) * 2654435761u32 as i32) >> 3;
    }
    let mut dest = vec![0i64; 1024];
    let iters = 2_000_000u64;
    for (name, f) in [("old", old as fn(&mut [i64], &[i32], usize, usize, i32)), ("new", new)] {
        for _ in 0..10 {
            f(&mut dest, &src, 0, 1024, 3);
            black_box(&dest);
        }
        let t = Instant::now();
        for _ in 0..iters {
            f(&mut dest, &src, 0, 1024, 3);
            black_box(&dest);
        }
        let ns = t.elapsed().as_nanos() as f64 / iters as f64;
        println!("{name}: {ns:.2} ns/frame");
    }
}
