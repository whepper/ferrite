use std::time::Instant;

const CAP: usize = 8192;

#[inline(never)]
fn write_old(buf: &mut [u8], bits: &mut usize, value: u32, nbits: u8) {
    if nbits > 32 || nbits == 0 {
        return;
    }
    if nbits < 32 && value >= (1u32 << nbits) {
        return;
    }
    let need = nbits as usize;
    let free = buf.len() * 8 - *bits;
    if need > free {
        return;
    }
    let mut remaining = need;
    let mut shift = need;
    while remaining > 0 {
        let room = 8 - *bits % 8;
        let take = room.min(remaining);
        shift -= take;
        let chunk = ((value >> shift) & ((1u32 << take) - 1)) as u8;
        let byte_idx = *bits / 8;
        buf[byte_idx] |= chunk << (room - take);
        *bits += take;
        remaining -= take;
    }
}

#[inline(never)]
fn write_window(buf: &mut [u8], bits: &mut usize, value: u32, nbits: u8) {
    if nbits > 32 || nbits == 0 {
        return;
    }
    if nbits < 32 && value >= (1u32 << nbits) {
        return;
    }
    let need = nbits as usize;
    let free = buf.len() * 8 - *bits;
    if need > free {
        return;
    }
    let bit = *bits;
    let byte = bit >> 3;
    let shift = bit & 7;
    if byte + 8 <= buf.len() {
        let mut word = u64::from_be_bytes(buf[byte..byte + 8].try_into().unwrap());
        word |= (value as u64) << (64 - shift - need);
        buf[byte..byte + 8].copy_from_slice(&word.to_be_bytes());
    } else {
        let mut remaining = need;
        let mut shift = need;
        let mut byte_idx = byte;
        let room = 8 - (bit & 7);
        if room != 8 {
            let take = room.min(remaining);
            shift -= take;
            let chunk = ((value >> shift) & ((1u32 << take) - 1)) as u8;
            buf[byte_idx] |= chunk << (room - take);
            byte_idx += 1;
            remaining -= take;
        }
        for _ in 0..4 {
            if remaining < 8 {
                break;
            }
            shift -= 8;
            buf[byte_idx] = (value >> shift) as u8;
            byte_idx += 1;
            remaining -= 8;
        }
        if remaining > 0 {
            let chunk = (value & ((1u32 << remaining) - 1)) as u8;
            buf[byte_idx] = chunk << (8 - remaining);
        }
    }
    *bits = bit + need;
}

fn run(name: &str, f: fn(&mut [u8], &mut usize, u32, u8), rounds: usize) -> (u128, u64) {
    let mut buf = vec![0u8; CAP];
    let mut bits = 0usize;
    let mut state = 0x1234_5678_9abc_def0u64;
    let start = Instant::now();
    for _ in 0..rounds {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let nbits = 7 + ((state >> 33) % 8) as u8;
        let value = ((state >> 17) as u32) & ((1u32 << nbits) - 1);
        if bits + nbits as usize + 64 > CAP * 8 {
            bits = 0;
            buf.fill(0);
        }
        f(&mut buf, &mut bits, value, nbits);
    }
    let elapsed = start.elapsed().as_nanos();
    let checksum = buf.iter().fold(0u64, |a, &b| a.wrapping_mul(1099511628211) ^ u64::from(b)) ^ bits as u64;
    println!("{name}: {elapsed} ns total, {:.3} ns/write, bits={bits}, checksum={checksum:016x}", elapsed as f64 / rounds as f64);
    (elapsed, checksum)
}

fn main() {
    let n = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20_000_000);
    run("old-warm", write_old, 1_000_000);
    run("window-warm", write_window, 1_000_000);
    let (a, ca) = run("old", write_old, n);
    let (b, cb) = run("window", write_window, n);
    println!("window {:.3}x checksums {ca:016x} {cb:016x}", a as f64 / b as f64);
}
