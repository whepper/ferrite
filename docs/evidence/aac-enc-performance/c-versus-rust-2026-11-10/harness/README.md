# Benchmark harness

These files are measurement tooling only. They are not production encoder
code and are not part of any library build.

## C harness — `c_bench_corpus.c`

Drives the public libAACenc API with the stripped `aac-enc.c` parameter
sequence: AAC-LC, VBR mode 5, ADTS, afterburner on, WAV channel order.

Timed: `aacEncOpen`, all `aacEncoder_SetParam` calls, the `NULL` initialize
call, `aacEncInfo`, all per-frame `aacEncEncode` calls, and the final flush.
Untimed: WAV load/parse, output-buffer allocation, `aacEncClose`, ADTS walk,
hashing, output file write, process startup.

Build (from the evidence directory; `$SRC` is the FDK C source root and `$T`
the benchmark working directory):

```sh
clang -O3 -DNDEBUG -arch arm64 \
  -I "$SRC/cpp" -I "$SRC/cpp/libAACenc/include" -I "$SRC/cpp/libSYS/include" \
  -I "$SRC/cpp/libFDK/include" -I "$SRC/cpp/libMpegTPEnc/include" -I "$SRC" \
  harness/c_bench_corpus.c "$SRC/wavreader.c" \
  -L "$T/build" -lfdk-aac -Wl,-rpath,"$T/build" \
  -o bin/c_bench_corpus
```

Usage: `c_bench_corpus manifest.tsv outdir reps warmup`.

Manifest lines are `stem<TAB>wav_path<TAB>expected_bytes<TAB>expected_hash`;
only the first two columns are used by the C harness. Rows:

```
CFG  stem sr ch samples frameLength nDelay maxOutBufBytes confSize
WARM stem rep init_ns encode_ns close_ns frames bytes hash
ROW  stem rep init_ns encode_ns close_ns frames bytes hash
IN   stem input_pcm_fnv1a64
DONE workload_count
```

`init_ns + encode_ns` is the measured total; `close_ns` is outside it.

## Rust harness — `bench_corpus.rs`

A Cargo example using only the public production API
(`EncoderConfig::new` + `Encoder::new` + `push` + `flush`). Same corpus,
1,024-frame chunks, fresh encoder per repetition, same timed/untimed
boundaries. `EncoderConfig::new` is inside the timed initialization to match
the C parameter-setup calls; `drop` is outside.

```sh
cargo build --release --locked --features aac_enc --example bench_corpus
bench_corpus manifest.tsv outdir reps warmup chunk_frames
```

Rows are `ROW stem rep sr ch samples frames bytes init_ns encode_ns hash`;
`init_ns + encode_ns` is the measured total. The harness asserts the
published byte total, frame count, and FNV-1a ADTS hash for every workload,
so a wrong input corpus or a production drift fails loudly.

## Fixture materializer — `fixture_dump.rs`

A test-only module that writes the 20-workload corpus from the published
`testdata::fixture_samples` generator. It is compiled only in the temporary
HEAD copy used for measurement.

```sh
FIXTURE_OUT="$T/corpus" cargo test --release --locked --features aac_enc --lib \
  aac_enc::fixture_dump::fixture_dump_tests::dump_all_benchmark_fixtures -- --nocapture
```

## Series drivers

- `run_measurements.sh` — 3 standalone C, 3 standalone Rust, 8 alternating
  pairs, 2 post C, 2 post Rust; 15 reps + 1 warm-up; 1,024-frame chunks.
- `run_portable.sh` — 8 alternating pairs of the `-mcpu=generic` /
  `-C target-cpu=generic` builds; 10 reps + 1 warm-up.

Both drivers check the expected 300/200 `ROW` lines and 20 warm-up rows and
fail on any harness error.

## Decode validation

All 40 final outputs (20 C, 20 Rust) were decoded with the reference
`aac-dec` from the same C build:

```sh
"$T/build/aac-dec" "$T/out/c-post-1/<stem>.c.aac" "$T/decoded/<stem>.c.wav"
"$T/build/aac-dec" "$T/out/r-post-1/<stem>.rust.aac" "$T/decoded/<stem>.rust.wav"
```

Every invocation returned 0 with empty stderr and a parseable WAV; decoded
sample counts are identical between C and Rust for all 20 workloads. The
tabulated results are in `../decode-validation.tsv`.
