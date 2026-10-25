# Using the pinned FDK AAC reference checkout

`rust/tools/check-reference.sh` validates a local FDK AAC reference
checkout against the pinned revision in
`fdk-aac-reference-revision.txt` (same directory). It checks source
checkout identity and completeness only.

## Invocation

```sh
rust/tools/check-reference.sh --ref-dir PATH
FDK_REF_DIR=PATH rust/tools/check-reference.sh
```

`--ref-dir` wins when both are given. Paths may contain spaces; they are
quoted safely. No absolute machine-specific paths are committed — pass
your local checkout path at invocation time (or via `FDK_REF_DIR`).

A valid pinned source checkout must pass this script before it is built.
Building the library, building `enc_oracle`, decoder checks, and table
regeneration belong to later workflow steps, not to this script.

## Failure behavior

Every failure prints a diagnostic to stderr and exits nonzero:

| Condition | Diagnostic names |
|---|---|
| No `--ref-dir` and `FDK_REF_DIR` unset | the missing argument/variable |
| Nonexistent directory | the given path |
| Not a Git checkout (or git unavailable) | the resolved path |
| Missing/unreadable pin file | the pin file path |
| Malformed pin (no 40-hex `Pinned commit:` line) | the pin file path and what was found |
| Revision mismatch | expected SHA, actual SHA, and a recovery hint |
| Missing key source (`aacEnc_rom.cpp`, oracle sources, `CMakeLists.txt`, `wavreader.c`, `NOTICE`) | the relative path and checkout root |

Success prints `reference checkout OK: <dir> at <sha>` and exits 0.

## Reference-dependent tests

Ordinary `cargo build` / `cargo test` / `cargo package` never need the
reference checkout: Tier-B tests are skipped in default CI. They run only
when invoked by name, and a missing artifact fails loudly instead of
skipping silently.

| Variable | Artifact | How to supply |
|---|---|---|
| `PHASE1_ORACLE_DIR` | oracle `.s1.txt` dumps | `tools/enc-oracle/README.md` (“Fixtures and runs”) |
| `PHASE1_C_DIR` | C-encoded reference outputs (full-stream parity) | build reference lib + `aac-enc` |
| `M55_C_DIR` | C-encoded ADTS `<stem>.aac` (stream comparison) | encode M52 fixtures with reference `aac-enc` |
| `AAC_DEC_BIN` | prebuilt `aac-dec` binary | `cmake --build <bld> --target aac-dec` |
| `IS_PROBE_WAV` / `IS_C_ORACLE` | retained probe WAV / C oracle transcript | provision offline; unset means not requested |
| `M511_PROFILE` (+`M511_FIXTURE`, `M511_ITERS`) | none (profiling workload) | set `M511_PROFILE=1` to opt in |

`rust/tools/preflight-validation.sh` checks this setup outside of
`cargo test`: with no Tier-B variables set it reports “not requested”
and exits 0; otherwise it requires `FDK_REF_DIR`, validates the checkout
with `check-reference.sh`, and verifies each requested artifact exists.
