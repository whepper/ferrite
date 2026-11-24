# FPE M1 — Python↔Rust fixture resynthesis cross-check (FPE-1.4 evidence)

## Purpose

The established Ferrite pattern keeps synthetic fixtures as a deterministic
Python generator (`tools/enc-oracle/make_fixtures.py`) plus a formula-identical
Rust resynthesis (`rust/src/aac_enc/testdata.rs::fixture_samples`) used by
tests. For the existing fixtures the equivalence is anchored against committed
oracle ring vectors. For the 6 NEW FPE fixtures, oracle vector anchoring is
blocked on C-reference re-provisioning (R-12), so a direct sample-exact
cross-check between the Python-generated WAVs and the Rust resynthesis was
performed as development verification.

## Method

1. Generated the fixtures 3× with `make_fpe_fixtures.py` into separate temp
   dirs; verified byte-identical output (SHA-256, see `fixture-stability.txt`).
2. Added a TEMPORARY `#[cfg(test)]` module to `testdata.rs` (code below) that
   reads each generated WAV (canonical 44-byte PCM header) and compares every
   sample against `fixture_samples(name)`.
3. Ran it against run1's WAVs; all 6 fixtures matched sample-for-sample.
4. REMOVED the temporary module afterwards (it depends on external artifacts
   and is not a permanent Tier-A test). The final `testdata.rs` diff contains
   only the 6 resynthesis arms — no cross-check code remains in the tree.

## Exact command

```sh
cd rust
FPE_FIXTURE_DIR=/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/fpe-m1/fixtures-run1 \
  cargo test --locked --lib fpe_fixture_resynthesis_matches_python_wavs -- --nocapture
```

## Recorded output (verbatim)

```
running 1 test
CROSSCHECK_OK fpe-near-silence-ramp-stereo (48000 samples/ch)
CROSSCHECK_OK fpe-fullscale-square-stereo (24000 samples/ch)
CROSSCHECK_OK fpe-pow2-boundary-stereo (48000 samples/ch)
CROSSCHECK_OK fpe-max-headroom-stereo (24000 samples/ch)
CROSSCHECK_OK fpe-allzero-stereo (12000 samples/ch)
CROSSCHECK_OK fpe-mixed-pair-stereo (48000 samples/ch)
test aac_enc::testdata::fpe_fixture_crosscheck_temp::fpe_fixture_resynthesis_matches_python_wavs ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 401 filtered out; finished in 0.05s
```

## Temporary harness code (used once, then removed — NOT part of the M1 diff)

```rust
#[cfg(test)]
mod fpe_fixture_crosscheck_temp {
    use super::*;

    // TEMPORARY development cross-check (removed after M1 verification):
    // asserts the Rust resynthesis of every FPE edge fixture matches the
    // Python-generated WAV sample-for-sample. Anchors the make_fpe_fixtures.py
    // <-> testdata.rs formula equivalence without the C oracle (R-12 blocked).
    fn wav_samples(path: &str) -> Vec<Vec<i16>> {
        let bytes = std::fs::read(path).expect("wav readable");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let nch = u16::from_le_bytes([bytes[22], bytes[23]]) as usize;
        let sr = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
        assert_eq!(sr, 48000);
        let bits = u16::from_le_bytes([bytes[34], bytes[35]]);
        assert_eq!(bits, 16);
        // canonical 44-byte header from Python wave
        let data = &bytes[44..];
        assert_eq!(data.len() % (2 * nch), 0);
        let n = data.len() / (2 * nch);
        let mut out = vec![Vec::with_capacity(n); nch];
        for i in 0..n {
            for c in 0..nch {
                let o = (i * nch + c) * 2;
                out[c].push(i16::from_le_bytes([data[o], data[o + 1]]));
            }
        }
        out
    }

    #[test]
    fn fpe_fixture_resynthesis_matches_python_wavs() {
        let dir = std::env::var("FPE_FIXTURE_DIR").expect("set FPE_FIXTURE_DIR");
        let cases = [
            ("fpe-near-silence-ramp-stereo", "fpe-near-silence-ramp-stereo-48k.wav"),
            ("fpe-fullscale-square-stereo", "fpe-fullscale-square-stereo-48k.wav"),
            ("fpe-pow2-boundary-stereo", "fpe-pow2-boundary-stereo-48k.wav"),
            ("fpe-max-headroom-stereo", "fpe-max-headroom-stereo-48k.wav"),
            ("fpe-allzero-stereo", "fpe-allzero-stereo-48k.wav"),
            ("fpe-mixed-pair-stereo", "fpe-mixed-pair-stereo-48k.wav"),
        ];
        for (name, file) in cases {
            let rust = fixture_samples(name);
            let py = wav_samples(&format!("{dir}/{file}"));
            assert_eq!(rust.len(), py.len(), "{name}: channel count");
            for c in 0..rust.len() {
                assert_eq!(rust[c].len(), py[c].len(), "{name} ch{c}: sample count");
                for (i, (&r, &p)) in rust[c].iter().zip(py[c].iter()).enumerate() {
                    assert_eq!(r, p, "{name} ch{c} sample {i}: rust {r} vs python {p}");
                }
            }
            println!("CROSSCHECK_OK {name} ({} samples/ch)", rust[0].len());
        }
    }
}
```

## Reviewer reproduction

The run1 WAVs are included under `fixtures-run1/`. A reviewer with the repo at
the M1 working-tree state can re-add the module above to `testdata.rs`, run the
command with `FPE_FIXTURE_DIR` pointing at `fixtures-run1/`, and remove it again.
Permanent oracle-vector anchoring remains blocked on reference re-provisioning
and is carried as an explicit M1 gap (not claimed resolved).