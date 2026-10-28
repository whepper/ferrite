use std::collections::HashMap;
use std::time::Instant;

fn main() {
    let n = 182usize;
    let reps = 2_000_000usize;
    // keys resemble (band ptr, len, gain, dz) within one pass
    let keys: Vec<(usize, usize, i32, bool)> = (0..n)
        .map(|i| (0x10000 + i * 28, 7 + i % 20, 40 + (i as i32 % 60), false))
        .collect();
    let mut h: HashMap<(usize, usize, i32, bool), (u16, u8)> = HashMap::new();
    for (i, k) in keys.iter().enumerate() {
        h.insert(*k, (i as u16, 0));
    }
    // 82% hits, 18% "misses" drawn from the same table (worst case: all present)
    let idx: Vec<usize> = (0..reps).map(|i| (i * 37) % n).collect();
    let t0 = Instant::now();
    let mut acc = 0u64;
    for &i in &idx {
        let e = h.entry(keys[i]).or_insert((0, 0));
        acc = acc.wrapping_add(e.0 as u64);
    }
    let dt = t0.elapsed().as_nanos();
    println!("std_hashmap_entry: {:.1} ns/lookup acc={acc}", dt as f64 / reps as f64);

    // open-addressed 512 slots, cheap multiplicative hash on a packed key
    let mut tab = vec![(0u64, 0u32); 512];
    let pack = |k: &(usize, usize, i32, bool)| -> u64 {
        ((k.0 as u64) << 24) ^ ((k.1 as u64) << 16) ^ ((k.2 as u64) << 8) ^ (k.3 as u64)
    };
    for (i, k) in keys.iter().enumerate() {
        let mut p = (pack(k).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 55) as usize & 511;
        while tab[p].0 != 0 {
            p = (p + 1) & 511;
        }
        tab[p] = (pack(k) | 1, i as u32);
    }
    let t1 = Instant::now();
    let mut acc2 = 0u64;
    for &i in &idx {
        let key = pack(&keys[i]);
        let mut p = (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 55) as usize & 511;
        loop {
            let e = tab[p];
            if e.0 == key | 1 {
                acc2 = acc2.wrapping_add(e.1 as u64);
                break;
            }
            if e.0 == 0 {
                break;
            }
            p = (p + 1) & 511;
        }
    }
    let dt2 = t1.elapsed().as_nanos();
    println!("open_addr_512: {:.1} ns/lookup acc={acc2}", dt2 as f64 / reps as f64);
}
