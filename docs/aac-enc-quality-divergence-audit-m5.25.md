# Rust AAC-LC VBR-5 Encoder — C-versus-Rust Quality and Divergence Audit (M5.25)

Diagnostic milestone **M5.25** (2026‑10‑16). This report maps what differs between the native Rust
AAC‑LC VBR‑5 encoder and the C FDK AAC reference, what those differences might mean, and what
evidence is needed next. **No encoder behaviour was changed and no discrepancy was fixed.**

**Code under audit**

| Item | Value |
|---|---|
| Repository | `/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac` |
| `HEAD` | `e4258d2a5b55581f5f2b27aa6c1a369bb2503e43` — does **not** contain the harness or the campaign |
| Rust implementation | `rust/src/aac_enc/**` (10 tracked files modified + untracked harness, uncommitted) |
| C implementation | `cpp/libAACenc/src/**` (production, unmodified in this project) |
| C reference binary | `…/T/opencode/phase0-20261015/bin/aac-enc`, md5 `850a8e6ed56641f453dc12b3c83946f5` (matches the M5.21 record) [derived‑here: md5 recomputed] |
| C build | CMake Release, `…/T/opencode/fdk-aac-build` (a second, older `aac-enc` there has md5 `e0900e…` and is **not** the reference used for the corpus) |
| Rust build | default `release` (opt‑level 3, LTO off, codegen‑units 16), rustc/cargo 1.99.0 |
| Encoder configuration (both) | AAC‑LC (AOT 2), VBR mode 5, afterburner on, ADTS, 1024‑sample frames, 16‑bit PCM |
| Fixtures | 18 (15 synthetic + 2 LibriSpeech speech + 1 CC0 music); corpus manifest `201b292db60d6bebb59a18891ce8d9e4ffda6f6f57b2f303f747107d87eb2998` |
| Preservation | `/Users/jeroen/backups/fdk-aac-m5.23-worktree-20261016T193606Z` (untouched) |

---

## 1. Executive summary

1. **Both encoders emit structurally identical streams and differ only in payload content.**
   Across all 18 fixtures the frame *count* is identical, every ADTS header field is identical in
   every frame, and both streams parse cleanly end‑to‑end. **16 of 18** files differ somewhere in
   the payload; the two byte‑identical files are `m2-silence-stereo-48k` (735 B) and
   `m2-edge-mono-48k` (415 B). [derived‑here, §4.F]
2. **The known impulse discrepancy is fully localized and none of the obvious explanations
   apply.** The 306‑vs‑308‑byte difference is **one frame** (`m2-impulse-mono-48k` frame 5 — the
   single SHORT/attack frame), C 178 B vs Rust 176 B; frames 0–4 and 6–10 are byte‑identical and
   the frame count is the same (11). It is therefore *not* a flush/termination, framing or
   ADTS difference. The committed C oracle vector for that frame is **byte‑identical to C's real
   payload**, while the C‑validation stages (S1–S8) are replayed bit‑exactly by Rust tests — so the
   difference is produced by the **inputs the real Rust encode derived for that frame**, not by the
   sectioning/Huffman/assembly code. Which upstream stage first diverges is **not yet established**
   (this is the recommended next milestone). [derived‑here + recorded, §4.A]
3. **The documented divergence inventory is precise and machine‑pinned**: 21 `KNOWN_MAXSCALE`
   (clz boundary ±1 flips, with a neutrality proof) and 40 `KNOWN_TNS` window decisions
   (gain‑only shifts and active‑filter coefficient flips), plus a relative TNS gain gate
   (`|Δ| ≤ 8 + |want|/1000`). [derived‑here: pin tables counted; recorded: spec §11–11.5]
4. **The highest‑impact open risk is the same in every document: active‑filter (TNS) spectrum
   differences of unverified quality impact** (`docs/aac-enc-migration-spec.md` §11.5), which the
   performance campaign (M5.12–M5.22) did not touch. [recorded]
5. **No quality metric exists anywhere in the project.** The planned metrics (per‑fixture
   decodability, bitrate, frame‑size histogram, delay‑compensated SNR/SDR) are specified in
   `docs/aac-enc-migration-spec.md` §6 and were explicitly deferred by the M4 plan
   (§10 exit criterion 5: "validity/decodability claims wait for the bitstream milestone").
   Decodability is attested (FFmpeg 18/18) but **no raw log is preserved**. [recorded + derived‑here]
6. **Eleven frames have both a committed C ground‑truth vector and a real C‑vs‑Rust payload
   difference** (click f11, impulse f5, onset f24/f25/f26, onset‑441 f22, sine f1/f5, sine‑441 f5,
   stepdown f2, sweep f10), and nine further oracle frames are **negative controls** (ground truth
   but no payload difference). This gives a bounded, decisive scope for attributing the
   divergence. [derived‑here, §7]
7. **Robustness findings are real but out‑of‑contract**: short‑window LFE and mixed‑window CPE
   panic; 960‑granule, multi‑element, CBR/ELD paths are rejected; PNS/intensity‑stereo books,
   retries and crash recovery never occur on the corpus and are asserted absent rather than
   exercised. [recorded + derived‑here, §3.12]
8. **Recommended next milestone: M5.26 — first‑divergence stage attribution on the 11 + 9 frozen
   oracle frames** (small, bounded, uses existing C vectors). The quality‑metric baseline is
   explicitly deferred to the milestone after it (§8).

---

## 2. Scope, source versions, and evidence methodology

### 2.1 What "the implementation" means here

`HEAD` is a pre‑harness checkpoint: it does **not** contain `rust/src/aac_enc/stream_driver.rs`
(the PCM→ADTS integration driver), `phase1b.rs`, or the `m7-*.wav` fixtures; those are untracked.
`docs/aac-enc-benchmark-plan.md` (M5.7) records the same state. The audited Rust implementation
is therefore the **worktree**: 10 tracked‑modified files plus the untracked driver. All Rust line
references below were checked against the current files. [derived‑here]

Two Rust layers must be distinguished throughout:

* **Encoder modules** (`config.rs`, `block_switch.rs`, `transform.rs`, `psy.rs`, `energy.rs`,
  `threshold_vbr.rs`, `pe.rs`, `quant.rs`, `scf.rs`, `bitcount.rs`, `rate.rs`, `bitenc.rs`,
  `frame.rs`, `adts.rs`, `tns.rs`, `pcm.rs`) — the production algorithms.
* **Test‑only integration driver** (`stream_driver.rs`, module doc `:1-6` "This is integration
  validation, not a production API"; `phase1b.rs`) — owns `StreamEncoder`, the corpus harness
  (`m58_run_once`, `:4038`), and the ADTS writer used for all corpus output.

### 2.2 Evidence sources

| Class | Source |
|---|---|
| Project documents | `docs/aac-enc-migration-spec.md` (§1.4 ADTS vectors, §5 numeric policy, §6 metric plan, §8 risks, §10 M2, §11–11.5 divergence inventory), `docs/aac-enc-m4-quant-ratecontrol-plan.md` (§7.3 comparison policy, §8 numeric differences, §10 risks, §12–18 S1–S7 records), `docs/aac-enc-benchmark-plan.md`, `docs/aac-enc-performance-campaign-m5.12-m5.22.md` |
| C ground truth (in repo) | `rust/src/aac_enc/testdata/`: 16 `mdct_*`, 13 `ring_*`, 47 `psy_*`, 47 `rxx_*`, 20 `ms_*`, 14 `seq_*`, and the `qc1`–`qc8` stage vectors (S1–S8) for the frozen oracle frames — 20 files for `qc1`/`qc2`/`qc4`–`qc8`, 19 for `qc3` — plus `qprobe_quant.txt`, `s7probe.txt`, `scfunit.txt`, `cfg_*`, `configs-all.txt` [derived‑here: inventory counted] |
| C outputs | `…/T/opencode/phase0-20261015/aac/*.aac` (15 synthetic fixtures), `…/T/opencode/m55/aac/*.aac` (3 m7 fixtures) |
| C oracle dumps | `…/T/opencode/phase0-20261015/oracle/*.s1.txt` (15 synthetic) |
| Rust outputs | `/var/folders/…/T/m52-rust-aac/m58-corpus/*.m58.aac` (18), hash‑pinned by `…/m522/{base,after}_hashes.txt` |
| Oracle tooling | `tools/enc-oracle/` (README, `enc_oracle.cpp`, `make_fixtures.py`, `extract_s*.py`, `validate_s*.py`, probes) |

### 2.3 New measurements produced by this audit (method stated; inputs read‑only)

All scripts live in `…/T/opencode/m525/`; nothing in the repository or in the M5.23 backup was
written. These are **newly derived** results, distinct from the recorded ones:

1. `adts_cmp.py` / `adts_localize.py` / `header_parity.py` — ADTS parse + field/­payload
   comparison of C vs Rust for all 18 fixtures (§4.F, §4.A).
2. `pins_and_impulse.py` — counted the `KNOWN_MAXSCALE`/`KNOWN_TNS` tables in `psy.rs` and
   compared the committed S8 vector of the impulse attack frame with the real C and real Rust
   frame payloads (§4.A).
3. `inputs.py` — C input WAV headers/PCM sanity (rate, channels, sample count, dioticity, peak).
4. `ring_full.py` — attempted to match committed `ring_*` vectors to WAV PCM slices; **failed by
   design** (see §6.2) and is reported as an inconclusive probe.
5. `intersect.py` — the oracle‑frame ∩ diverging‑frame set (§7).
6. Flush‑law arithmetic `frames == ceil((S + 1600)/1024)` checked against all observed frame
   counts (§4.D).

---

## 3. Divergence inventory

Classification: **C1** confirmed correctness/compliance concern · **C2** plausible quality risk ·
**C3** understood implementation difference · **C4** robustness/out‑of‑contract · **C5**
performance‑only · **C6** insufficient evidence. A finding may carry several labels.
`R:` = how it is reproduced (**verified** = this audit re‑checked it against artifacts/source;
**recorded** = documented, not re‑verified here).

### 3.1 Input handling, formats, channel configuration

**D‑001 — Encoder configuration parity.** *Observed:* both sides encode AAC‑LC/VBR‑5/
afterburner‑on/ADTS/1024 frames. *C:* `aac-enc.c:46-48` (`aot=2, afterburner=1, vbr=5`),
`:119-130` (params), `:33` (usage string). *Rust:* `config.rs:99` ("LC only, VBR mode 5 only,
afterburner always on, ADTS/RAW transport only"), `config.rs:123` (`FRAME_LENGTH = 1024`).
*Evidence:* source; M5.21 size agreement (music 251 279 vs 250 926 B, speech 127 971 vs
127 900 B). *R:* verified. *Impact:* none (parity). *Confidence:* high. **C3.**

**D‑002 — Accepted configuration surface.** *Observed:* C accepts mono→5.1 channel modes
(`aac-enc.c:87-97`), any WAV sample rate, and (through the API) 960‑granule frames. Rust accepts
12 sample rates (`config.rs:125-127`: 8 000…96 000), 1–8 channels at config level
(`config.rs:193-195` → `channel_map.rs:151-212, :227-232`), and **rejects** any frame length
≠ 1024 (`config.rs:196-198`; tests `:334-341` assert 960/512 →
`ConfigError::UnsupportedFrameLength`); ADTS framing additionally refuses `channel_configuration`
0 or >7 (`adts.rs:176-179`), so 6.1/7.1 layouts (config values 11/12/14) fail at the transport
even though the config accepts the channel count. The test driver narrows further to mono/stereo
(`stream_driver.rs:500-503`). *R:* verified. *Impact:* out‑of‑contract only; a C user of
960‑granule LC, or of 6.1/7.1 ADTS, cannot use the Rust port. **C4.**

**D‑003 — Input depth/format.** *Observed:* C requires 16‑bit PCM WAV, rejects others
(`aac-enc.c:79-86`); Rust fixtures are 16‑bit `i16` PCM generated in code
(`testdata.rs::fixture_samples`, twin of `tools/enc-oracle/make_fixtures.py`). Input sample
counts agree exactly with the harness (`in_samples`) for all 18 fixtures [derived‑here: WAV
headers parsed]. *R:* verified. **C3.**

**D‑004 — Input provenance defect found and fixed (test‑only).** *Observed:* the 44.1 kHz sine
fixture once shipped as an all‑silent WAV because `make_fixtures.py` passed a bare function where
the `(t, c, i)` protocol was required (channel index became frequency); the committed LONG
vectors described that silent file. Fixed with a one‑line lambda wrap; the WAV was regenerated
(sha1 `d9ee6d… → ee796e3b…`) and 7 vectors refreshed
(`docs/aac-enc-migration-spec.md` §11.3). The current `make_fixtures.py:170-173` shows the
corrected form. *R:* recorded (fix verified in source). *Impact:* none now; it is a *class* risk —
test‑only artifacts can masquerade as encoder divergence, which is why
`transform.rs:451 synthesis_matches_oracle_input` now pins the wav↔resynthesis contract.
**C3 (+C4 as a process risk).**

### 3.2 Psychoacoustic analysis and masking thresholds

**D‑010 — Q31 stored‑domain mapping is the single documented approximation point.** *Observed:*
all C decisions that consume the transformed spectrum consume Q31 ints; the Rust port maps its
`f64` stored spectrum once (`energy.rs:152-159 map_to_q31`, round‑to‑nearest) and the driver
documentation calls this "the documented single approximation point"
(`stream_driver.rs:24-28`). Everything downstream of that mapping in the integer lane is then
bit‑exact by construction; everything upstream (MDCT, TNS, Ld) is float. *C:*
`CalcSfbMaxScaleSpec`/`CheckBandEnergyOptim` (`band_nrg.cpp:111-184`). *R:* verified (source +
the pinned‑vector replays of §3.5). *Likely explanation:* by design (spec §5: preserve
mathematics/decisions, not bit patterns). *Impact:* this is the **mechanism class** behind every
frontend boundary flip. *Confidence:* high. **C3** (as a design property) / **C2** (as the root of
unverified downstream effects).

**D‑011 — maxscale ±1 clz‑boundary flips (21 pinned).** *Observed:* 21 `(fixture, frame, ch)`
entries in `KNOWN_MAXSCALE` (`psy.rs:1872-…`) where Rust and C disagree by ±1 (four +2 cases) in
individual band headroom values; e.g. click‑mono f11 has 9 flipped bands. *C:*
`CalcSfbMaxScaleSpec` (`band_nrg.cpp:111-132`, `min(30, clz(peak)-1)`). *Rust:* `energy.rs:126-144`.
*Evidence:* spec §11.2 mechanism + neutrality proof (flips straddle exact powers of two, both
directions, 7 up/2 down in the representative frame; stored energies algebraically invariant; 7/9
flipped bands bit‑equal, 2 differ by 1 LSB, all inside `32+E/500`; global‑min band and
`finalShift` equal; TNS never consumes maxscale). *R:* verified (pin count; mechanism and
neutrality quoted from the recorded study, not re‑derived). *Impact:* representational, **no
coding‑decision impact demonstrated**, conditional on `finalShift` equality (which held in all
five studied cases). *Confidence:* high for the studied frames; medium as a general claim (the
neutrality argument is case‑by‑case). **C3 (+C2 residual).**

**D‑012 — TNS decision divergences (40 pinned window cases).** *Observed:* 40
`(fixture, frame, ch, window)` entries in `KNOWN_TNS` (`psy.rs:2227-…`) covering multitone f1
(gain‑only, same filter), onset f24 w4/w6/w7, onset f25/f26 w0, onset‑441 f22 w6/w7, sine f5/f6
w0, sine‑441 f5 w0, stepdown f1 w1..w7, stepdown f2 w0, stepdown f5 w0. Causes recorded in the
pins: *"input‑driven gain shift, same filter"*, *"input‑driven parcor border flip, filter active
both sides"*, *"input‑driven knife‑edge, filter inactive both sides"*. *C:*
`FDKaacEnc_TnsDetect` (`aacenc_tns.cpp:766`), `FDKaacEnc_TnsSync` (`:961`),
`FDKaacEnc_TnsEncode` (`:1051`). *Rust:* `tns.rs` (detect `:722-…`,
`merged_autocorr_fixed`, float Schur). *Evidence:* spec §11/§11.1 (three‑way traces showing the
first divergence is the upstream Q31 input, and that a C‑reference replica is bit‑identical to
Rust at every intermediate). *R:* verified (pin count) + recorded (traces). *Likely explanation:*
f32‑MDCT noise crossing Q15 coefficient borders. *Impact:* where the filter is **active on both
sides with different coefficients, the encoded spectrum changes** — spec §11.5 states this
explicitly for f24/f25/f2 and adds "**quality impact unverified**"; where both sides are inactive,
the spectrum is unchanged. *Confidence:* high that the divergence is real and upstream; **the
quality impact is unknown**. **C2.**

**D‑013 — TNS prediction‑gain residue (float Schur vs fixed‑point).** *Observed:* with C‑exact
merged rxx input, Rust's float Schur gives gains differing by ≤11 in 22 154 (5×10⁻⁴ relative);
input‑driven decision flips are Δ29…9037. The agreement policy is now
`|Δ| ≤ 8 + |want|/1000` (`tns.rs:706-720`, `GAIN_TOL_PER_MILLE = 1`; `gain_tolerance(22154) == 30`
per `tns.rs:1102`), with `active`/order/coefficient checks kept **exact** so no decision flip can
hide behind the value gate. *Evidence:* spec §11.2 (measurement), §11.4 (policy implemented).
*R:* verified (formula in source). *Impact:* value‑level only; the filtered spectrum is scaled by
this gain, so a 5×10⁻⁴ gain difference can move LSBs of a Q31 spectrum even with identical
coefficients. *Confidence:* high. **C3.**

**D‑014 — PNS and intensity stereo are never activated on the corpus.** *Observed:* PNS is off by
configuration (`NO_NOISE_PNS`, zero books/scales) and IS is asserted inactive per frame — a live
`is_book != 0` would be a loud finding, never a silent fallback (`stream_driver.rs:34-36, :51-57`).
The driver feeds `noise = NO_NOISE_PNS` and zero books/scales (`stream_driver.rs:1122`, `:952`,
`:974`, `:1146-1147`); the S1 proof is recorded in `bitcount.rs:149-152` ("PNS/IS branches are
dead on fixtures but live in code"); the oracle dumps corroborate (`ISB/ISS` zeros, `NOISE` all
`NO_NOISE_PNS`; `tools/enc-oracle/README.md:190-191, :347-349`). *R:* verified (documentation +
oracle record). *Impact:* **the code paths for live IS/PNS are unvalidated C‑vs‑Rust**; equally,
the corpus cannot detect a regression in them. In‑contract risk is low *only* because the C
configuration also produces zero books — which is itself only proven for this corpus/rate.
**C6 (evidence) + C4 (coverage).**

### 3.3 Window sequence selection and block switching

**D‑020 — Window‑sequence decisions are verified identical (not a divergence).** *Observed:* the
end‑to‑end psy test asserts `seq` equality against the committed C `seq_*` vectors for **every
frame of each tested prefix** — per `CASES` entry, frames `0..=max(listed frames)`
(`psy.rs:2932-2947` case list, e.g. `("impulse-mono", &[4, 5])`,
`("onset-stereo", &[22,23,24,25,26])`, `("stepdown-stereo", &[0,1,2,5])`; loop bound
`let max_f = frames.iter().max()` at `psy.rs:2954`, assert at `psy.rs:2959-2964`). Parity is
therefore proven for those prefixes (e.g. impulse f0–f5 of 11 frames, click f0–f11 of 25,
sine f0–f6 of 49), **not for every frame of the 14 fixtures**. C:
`FDKaacEnc_BlockSwitching`/`SyncBlockSwitching`
(`block_switch.cpp:229, :426`). *R:* verified (source). *Impact:* none — record as a **verified
parity**. **C3.**

**D‑021 — Oracle short‑window "blips" (test‑harness limitation).** *Observed:* ten content frames
in the *oracle* replay diverge from the real `aac-enc` frame lengths — **all SHORT_WINDOW**: f1 of
sine, stepdown, click, multitone, sweep, tonal‑noise, sine‑441k, stress and onset f24 /
onset‑441 f22; every LONG/START/STOP frame matches exactly, and 5/15 files match on **every**
frame (silence 49/49, impulse 11/11, edge 14/14, noise 25/25, quiet‑sine 49/49). Root cause is
recorded as upstream of S6 in the short‑block replay inputs; trailing f47/f48 index diffs are
oracle flush‑tail artifacts (the oracle appends three zero frames).
*Evidence:* `docs/aac-enc-m4-quant-ratecontrol-plan.md` §17.4; `tools/enc-oracle/README.md:421-425`.
*R:* recorded. *Impact:* the divergence is between **oracle replay** and the encoder, i.e. a
measurement‑instrument limitation, not a demonstrated C/Rust production divergence. Note however
that the same short‑block region is where the real C‑vs‑Rust payload differences begin (§4.F).
*Confidence:* high for the classification. **C3 (+C6 for the exact upstream cause).**

**D‑022 — Short‑window LFE and mixed‑window CPE panic (sites now verified).** *Observed:* driving a
short‑window LFE channel panics in the energy path; a CPE whose left and right channel window
sequences differ (`short_l ≠ short_r`, in either direction) panics in the mid/side block; both were
observed to fail identically in old/new Rust trees during M5.20 (no regression) [recorded].
*Mechanism (source‑verified in this audit):*

* **Short‑window LFE:** for a SHORT config, `lowpass_line_lfe()` is forced to 0 and
  `sfb_active_lfe()` collapses to 0 (`sfb.rs:267-270, :279-285`), so `psy.rs:525-529` selects an
  empty LFE band count; the per‑window vectors (`ws.maxsc_l[w]`, `ws.e_l[w]`) are then empty
  (`psy.rs:587-595`), yet `psy.rs:650` calls
  `energy::max_nrg_for_shift(&ws.e_l[w], &ws.maxsc_l[w], …)`, whose loop leaves `nr = 0` and
  indexes `max_scale[nr]` → **`energy.rs:221`** (inside `max_nrg_for_shift`, `energy.rs:205-224`).
  A second, independent out‑of‑bounds would occur later in `group_short_channel::regroup` at
  **`psy.rs:1300/1302`** (it uses the non‑LFE active count, `psy.rs:1252`).
* **Mixed‑window CPE:** right SHORT / left not → `n_win_l = 1` but the MS loop iterates
  `n_win_r = 8` and indexes the **left** vector at **`psy.rs:957`** (`&ws.maxsc_l[w]`, loop at
  `psy.rs:953-962`); left SHORT / right not → one mid/side window is built (`psy.rs:951-973`) and
  the short left channel's `group_short_channel` later walks 8 windows over it →
  **`psy.rs:1300/1302`**. The code documents the assumption it relies on
  (`psy.rs:486-488`: C uses `isShortWindow[0]` element‑globally, "CPE pairs always agree
  post‑sync") but does not enforce it.

*Crucially, both implementations make short‑window LFE impossible in normal use:*
`block_switch.cpp:229-244` forces long blocks for `isLFE`, mirrored by `block_switch.rs:329-332`
("`is_lfe` forces long blocks with SINE shape and a single group"), and the driver hardcodes
`is_lfe: false` (`stream_driver.rs:648, :655`). *Note:* the M5.20 evidence cites
`energy.rs:194` ("stale line"); the same code lives at `energy.rs:205-224` in both the current tree
and the M5.23 backup, and the decisive index is `energy.rs:221` [derived‑here]. *R:* verified
(mechanism and the LFE forcing) / recorded (the panic observations). *Impact:* an API‑level
contract violation is unlikely to be reachable through normal configuration, but the Rust code
panics where C's behaviour is untested. **C4 (+C6).**

### 3.4 TNS analysis and filtering
Covered by D‑012, D‑013, plus: **LO‑active / `merged=1` TNS paths have zero corpus coverage**
(spec §11.5) — recorded, not re‑verified here. **C6.**

### 3.5 Spectral quantization / scalefactor selection / bit counting / rate control

**D‑030 — The integer quantizer lane is bit‑exact given identical inputs (verified parity).**
*Observed:* the project policy is that `scf`, `globalGain`, section `codeBook/sfbStart/sfbCnt`,
`maxSfbPerGroup`, `firstScf`, fill/align bits and all bit counts are **exact**; only Ld/Q31 values
use tolerances (`docs/aac-enc-m4-quant-ratecontrol-plan.md` §7.3). The Rust tests replay the
committed C vectors bit‑exactly at every stage, including full payload hex:
`qc4/qc5` (S4 scf/gain, S5 sections/bits), `frame.rs:1395 s8_corpus_elements_match_c_bit_exact`
(S8ELEM **and** S8RAW hex, `:1590-1597`), `bitenc`/`scf`/`bitcount` probe transcripts.
*R:* verified (test names and asserts in source; vector inventory counted). *Impact:* **there is no
demonstrated implementation divergence in these stages**; any end‑to‑end difference must come from
their inputs. **C3.**

**D‑031 — Unvisited‑band QMAX contract.** *Observed:* when a gain‑loop pass visits fewer bands than
the buffer holds, Rust zero‑initialises the untouched QMAX entries while C leaves stale values; the
S5 fixtures pin the Rust contract and the S6 hand‑off cross‑checks only visited bands
(`docs/aac-enc-m4-quant-ratecontrol-plan.md` §17.5). *R:* recorded. *Impact:* none demonstrated —
downstream loops are bounded by `maxSfbPerGroup`. **C3.**

**D‑040 — Retry / crash‑recovery coverage.** *Observed:* all 20 committed QC frames are
single‑pass (`npass = 1`, `crash = -1`); retry and crash are covered only by synthetic Rust tests
with shrunken buffers (2000‑bit multi‑pass, 1200‑bit crash). The gain loop is bounded by
`MAX_OUTER_PASSES` (25) and **panics** if tripped (`rate.rs:189-192`, `rate.rs:1545` "outer loop
runaway (implementation bug)"), and in the *driver* any corpus retry/crash is a **loud skip**,
never a force‑fit (`stream_driver.rs:42-44`). *Evidence:* m4 plan §17.3, §10 R4; source.
*R:* verified/recorded. *Impact:* the VBR‑5 path makes retries an emergency path
(`totalAvail == maxBitsPerFrame`), so real‑music coverage of retry/crash is still absent; a
retry on real content would currently *drop a frame* rather than encode it. **C6 (+C4).**

**D‑041 — `checkMinFrameBitsDemand` stub.** *Observed:* the Rust port returns 1, deliberately
mirroring the C stub (`rate.rs:1131`, doc `:125, :160`, C `qc_main.cpp:571-575`). *R:* verified
(source + doc). *Impact:* parity with the C stub, so no divergence; but it is a functional stub on
both sides. **C3.**

### 3.6 Section data, codebooks, spectral bitstream coding
Covered by D‑030 (bit‑exact replay of section/scf/spectral payload hex). No separate finding.

### 3.7 Frame assembly, ADTS headers, frame length

**D‑050 — ADTS/framing parity (verified identical).** *Observed* [derived‑here]: for all 18
fixtures and **all** frames, `profile`, `sampling_frequency_index`, `channel_configuration`,
`protection_absent`, `number_of_raw_data_blocks` and `adts_buffer_fullness` are identical between
C and Rust; every frame‑length field equals the parse offset step, both streams consume exactly to
EOF, and the first‑frame vectors are `ff f1 4c 80 01 ff fc` (48 k stereo),
`ff f1 4c 40 01 bf fc` (48 k mono) — exactly the vectors recorded in
`docs/aac-enc-migration-spec.md` §1.4; 44.1 kHz yields `ff f1 50 …` (index 4); fullness is 2047
(VBR) on both sides. Transport header cost is 56 bits, C‑probed
(`tools/enc-oracle/README.md:394-397`, `tp_probe`). *R:* verified. *Impact:* none; **header parity
is not bitstream parity and neither is perceptual quality** (see §5). **C3.**

**D‑051 — Payload content and length differences (the campaign's real divergence).**
*Observed* [derived‑here, full table in §4.F]: 16/18 fixtures differ in payload; per‑file size
deltas range C−Rust from +93 B (`m3-multitone`) to −353 B (`m7-music`); first diverging frame
index varies from 0 to 24; 435/471 frames differ for music and 452/471 for speech‑clean, while
`m2-impulse` differs in exactly **one** frame. *R:* verified. *Impact:* bitstream‑only so far;
quality impact unverified. **C2.**

**D‑052 — Decodability.** *Observed:* FFmpeg decodes all 18 Rust outputs cleanly (18/18) as
reported in the M5.20 and M5.22 review packets; the internal ADTS re‑parse exists in code
(`stream_driver.rs:4092+`). *R:* **recorded only — no raw FFmpeg log was preserved**; also, the
comparison decodes *Rust* output, not C‑vs‑Rust quality. **C6 (raw evidence) / C3 (claim).**

**D‑053 — Fill / END / alignment.** *Observed:* the impulse attack frame's C vector records
`static=76, useddyn=1282, totfill=7, align=0, total=1368` and the Rust S8 replay matches that hex
bit‑exactly (`frame.rs:1395`); the fill sizing is gated by the C `cnt >= 4` payload rule (a port
bug found and fixed, m4 §17.5 item 2). *R:* verified (vector + source). **C3.**

### 3.8 Flush, end‑of‑stream, termination

**D‑060 — Termination law parity (verified identical).** *Observed:* C pads the tail with zeros
until `nZerosAppended >= nDelay` and then returns `AACENC_ENCODE_EOF`
(`aacenc_lib.cpp:1091-1114`; `nDelay = DELAY_AAC(framelength)`, `:759` = 1600 for 1024 frames);
the Rust feeder does the same (`pcm.rs:99-122` documents "9600 mono samples produce 11",
`finish()` at `:261`, delay at `:213`) and the driver states the law
`ceil((S + 1600)/1024)` (`stream_driver.rs:47-49`, "verified against the C encoder in Phase 0").
[derived‑here] the law reproduces the observed frame count for **every** sample count in the
corpus: 48 000→49, 24 000→25, 12 000→14, 9 600→11, 44 100→45, 480 000→471 — and the C outputs
have exactly those counts too. *R:* verified. *Impact:* none for frame count; **the final frame's
payload still differs in 11/18 fixtures**, so termination equivalence is a count/zero‑pad
statement, not a content statement. **C3.**

**D‑061 — Oracle flush tail.** The oracle appends three extra all‑zero frames, producing the
recorded f47/f48 trailing index artifacts (README:167-168, m4 §17.4). Test‑only. **C3.**

### 3.9 Numerical behaviour, floating point, rounding

**D‑070 — Float‑native port vs Q31 reference.** *Observed:* Rust is `f32`/`f64` throughout (no
`FIXP` types), C is Q31. Spec §5 fixes the policy: preserve mathematics and decisions, **not bit
patterns**; spec §8 (m4 plan) enumerates the seven numeric‑representation areas (constant
mapping, log domains, the quantizer's normalize/`mTab_3_4`/dead‑zone chain, the `>>1` distortion
asymmetry with the `MAX_QUANT` early‑out, VBR threshold/chaos fixed‑point, PE/counting integers,
and the M3 lesson). *Evidence:* spec §5, m4 §8; mechanism of observed flips recorded in spec §11
("f32‑kernel noise through round‑to‑nearest Q mapping", worst |7536 LSB in a Q31 detect input).
*R:* verified (policy + the mapping site) / recorded (magnitudes). *Impact:* the root cause class
of D‑011/D‑012/D‑051. **C3 (as design) + C2 (as consequence).**

**D‑071 — Float Schur vs fixed‑point Schur.** Covered by D‑013. **C3.**

**D‑072 — Ld/log2 rounding residues.** *Observed:* the Rust Ld path uses `f64 log2/64` with
explicit sentinels; the code notes a ~1e-9 difference covered by tolerance
(`psy.rs:1461`), and the tolerance functions are `32 + |want|/500` for Q31 lanes
(`psy.rs:1496-1510`) and `32.0 + e_want / 500.0` for Ld lanes, where
`e_want = ld_to_e(...)` (`psy.rs:1528-1552`). *R:* verified (source).
*Impact:* bounded by the gates; these gates are wide enough (≈0.2 % + 32 LSB) to admit downstream
decision changes, which is what D‑051 shows. **C3 (+C2).**

### 3.10 Robustness, error handling, invalid configurations

**D‑080 — Unsupported configurations: errors vs panics.** Rust rejects unsupported sample rates,
channel counts and frame lengths with typed `ConfigError` values (`config.rs:184-198`); the corpus
harness turns C‑incomparable conditions into loud failures (`stream_driver.rs:42-44`) and asserts
its own narrowed surface (`stream_driver.rs:500-510` "single‑element mono/stereo only"). Within the
encoder modules, the only unconditional `panic!`s on non‑data‑dependent paths are the ELD
`bits2pe` table path (`rate.rs:490`, unreachable at LC) and the gain‑loop runaway guard
(`rate.rs:1545`, D‑040); the rest of the production error surface is `Result`-based or
`debug_assert!`. *R:* verified (source). *Impact:* production hardening is not yet a goal (no
production API exists); **C4.**

**D‑081 — 960 granules and multi‑element frames.** 960 is spec‑allowed (`docs/aac-enc-migration-spec.md`
§8.4) but unimplemented; multi‑element (CCE/LFE), CBR and ELD paths are absent and expected to
panic (`config` doc `:99`; m4 §17.5 "multi‑element/CBR/ELD paths panic explicitly"). *R:*
verified/recorded. **C4.**

**D‑082 — Asserted‑absent vs exercised paths.** Live IS books, PNS, gain‑loop retries, crash
recovery and large fills are asserted absent per frame rather than implemented‑and‑validated
(`stream_driver.rs:34-36, :42-44, :51-57`; README:190-191, :501-506). *Impact:* a robustness
posture (fail loud, never silently substitute) — good for trust, but it means the *encoder* (as
opposed to the port) cannot yet handle content where those paths fire. **C4 (+C6).**

**D‑083 — Pre‑existing decoder test failure.** `tp_dec::pce::tests::read` fails on default and
encoder features alike; reproduced independently by the M5.20 reviewer on a baseline copy.
Unrelated to the encoder and deliberately untouched. **C3.**

**D‑084 — Rust replaces C's silent out‑of‑bounds/silent‑no‑op cases with loud failures.**
*Observed:* several C code paths read out of bounds or silently no‑op on extreme input; the port
replicates the *guard* but aborts instead of reproducing the undefined read. Verified sites:
`quant.rs:204-210/:336-341` (the `q == -32768` case that slips every `|q| > MAX_QUANT` guard) with
the loud replacement at `quant.rs:353` (`assert!((spec_exp as u32) < 14)`);
`bitcount.rs:164-166` (scf‑delta table read → loud panic instead of C's OOB read) and
`bitcount.rs:530-537` (stride‑4 reads); `bitenc.rs:151-164, :202-234` (C mask/OOB/width/`|Δ|>60`/
default‑arm/buffer cases returned as `BitEncError`, with the IS‑book case explicitly skipped
"with the citation"); `frame.rs:256-258` ("Every C silent/UB case … is an explicit error here").
One residual asymmetry: `pe.rs:163-164` guards `|Δscf| ≤ 60` with a **`debug_assert!` only**, so in
a release build the table index would still panic on a *malformed* input, whereas C's `FDK_ASSERT`
compiles in only on non‑`NDEBUG` builds (the oracle build keeps asserts armed,
`tools/enc-oracle/README.md:358-362`). *R:* verified (source). *Impact:* behaviour differs from C
only on inputs C cannot handle defined‑ly; relevant to a future production error contract, not to
corpus encoding. **C4 (+C6 on the exact reachability of the `-32768` case).**

**D‑085 — Deliberate mirrors of C's stale scratch (not divergences).** *Observed:* the port
reproduces C's uninitialised/stale scratch semantics in several places rather than "cleaning" them:
short‑block MS Ld left stale as C does (`psy.rs:356-358`, `:965-967`) with the fresh grouped
refresh documented separately (`psy.rs:1408-1409`); `calc_max_value_in_sfb` never writes unvisited
bands, like C (`rate.rs:178-180`, cross‑checked only on visited bands); `sfbWeightedEnergyLdData`
is write‑only scratch (`pe.rs:583-587`) and the `isBook` branch is "a dead store in C"
(`pe.rs:880-881`); TNS keeps stale tail entries beyond `maxOrder` exactly as the reference structs
do (`tns.rs:738-739`); stored energies keep C's shift‑down unconditionally (`energy.rs:426-427`).
*R:* verified (source). *Impact:* none demonstrated — these are *faithful* reproductions, and two
of them were the subject of explicit oracle discussion (D‑031). **C3.**

### 3.11 Areas with insufficient evidence

* **Perceptual quality of any divergence** (all fixtures) — **C6**: no metric exists (§6.1).
* **First diverging processing stage for D‑051** — **C6**: no production‑stage dumps preserved.
* **C behaviour on the D‑022 out‑of‑contract inputs** — **C6**: no C run recorded.
* **PNS/IS/retry/crash paths C‑vs‑Rust** — **C6**: never activated.
* **`m7-*` (real audio) oracle ground truth** — **C6**: the C oracle dumps cover only the 15
  synthetic fixtures; for music/speech there is only the whole‑file `.aac`.

---

## 4. Detailed analysis of the known findings A–F

### A. Impulse output‑size discrepancy (306 vs 308 bytes) — **fully localized; not explained away**

**Facts established by this audit** [derived‑here unless noted]:

| Quantity | C (`phase0-20261015/aac/m2-impulse-mono-48k.aac`) | Rust (`m58-corpus/m2-impulse-mono-48k.m58.aac`) |
|---|---|---|
| Total bytes | 308 | 306 |
| Frames | 11 | 11 |
| Frame lengths | 13,13,13,13,13,**178**,13,13,13,13,13 | 13,13,13,13,13,**176**,13,13,13,13,13 |
| Payloads | 6,6,6,6,6,**171**,6,6,6,6,6 | 6,6,6,6,6,**169**,6,6,6,6,6 |
| ADTS fields (all frames) | identical (see D‑050) | identical |
| First differing payload byte | — | frame 5, payload byte 10 (`49` vs `39`) |

* **Which frames differ:** exactly one — frame index **5**, the short‑block attack frame (the
  impulse sample sits at index 4800 → frame 4.69; the block‑switch sequence makes frame 5 the
  SHORT frame with grouping `{3,2,1,2}`, spec §11.3/§10). Frames 0–4 and 6–10 are byte‑identical,
  including frame 4 (the frame containing the impulse sample).
* **Not flush/termination, not framing:** both streams have the same frame count under the same
  termination law (D‑060), the same ADTS header field values, and identical frame boundaries up to
  the differing frame. The 2 bytes are *inside one raw_data_block payload*.
* **Not an assembly/Huffman defect:** the committed C vector `qc8_impulse-mono_f5.txt`
  (`S8RAW bits=1368`, 171 bytes, `S8FINAL static=76 useddyn=1282 totfill=7 align=0 total=1368`) is
  **byte‑identical to the real C payload** [derived‑here: compared byte‑for‑byte], and the Rust
  test `frame.rs:1395` replays S8ELEM/S8RAW hex bit‑exactly; the C QC vector
  (`qc6_impulse-mono_f5.txt`: single pass, `npass=1`, `gain0=-32`, `dyn0=1282`) shows C reached
  this frame in one gain‑loop pass from its S4/S5 state.
* **Conclusion (hypothesis with strong support):** given the inputs C had at frame 5, the Rust
  bitstream machinery *would* have produced C's exact 171 bytes; the real Rust encode produced 169
  because the **inputs it derived for that frame differ** — i.e. somewhere in
  MDCT → psy → Q31 mapping → thresholds → S4 scalefactors/gain → S5 counting (or the cross‑frame
  adaptive state entering the frame). The frame is a short block, i.e. exactly the region named by
  D‑012 (active‑filter TNS) and D‑021 (oracle short‑block sensitivity).
* **Not established:** that the difference is a defect, that it changes audio quality, or which
  stage diverges first. The fixture is a synthetic single‑sample impulse, so it exercises a
  pathological transient rather than typical content.

*R:* verified. *Impact:* bitstream difference of unknown quality significance. *Confidence:* high
for the localization, **none** for any quality claim. **C2.**

### B. Active‑filter spectrum differences — **real, quality impact explicitly unverified**

* **Exact specification section:** `docs/aac-enc-migration-spec.md` §11 ("M3 TNS diagnosis"), §11.1
  (per‑case traces), §11.5 (checkpoint): *"Active‑filter spectrum differences (f24/f25/f2) change
  the encoded spectrum; quality impact unverified."*
* **What is known:** the TNS *detect* input is Q31 (float MDCT mapped by round‑to‑nearest); on
  several transient/short frames the Rust and C detect inputs differ by hundreds of LSBs
  (worst |7536|), which flips Q15 parcor borders → different filter order/coefficients → a
  **different active FIR applied to the spectrum**. Where both sides end inactive (or where only
  the gain differs with identical coefficients) the spectrum is unchanged; the pinned causes say
  exactly which of the 40 cases is which ("filter active both sides" vs "knife‑edge, filter
  inactive both sides" vs "gain shift, same filter").
* **Which stage introduces it:** the **frontend MDCT/Q31 mapping** (D‑010/D‑070) — the traces show
  the very first divergence is the detect input itself, not the merge/Schur port (the C‑reference
  replica is bit‑identical to Rust at every intermediate).
* **What remains unknown:** the *magnitude and audibility* of the resulting spectrum change, and
  whether a different active filter ever changes a coding decision (codebook/scalefactor/gain)
  rather than only the spectrum values. No quality score exists, and none may be invented.
* **Coverage gaps:** LO‑active and `merged=1` TNS paths have **zero corpus coverage** (§11.5).

*R:* recorded (spec) + verified (pins 40, causes, code sites). **C2** (impact) / **C6** (coverage
for LO/merged).

### C. Window‑sequence and short‑window findings — **four distinct classes, do not conflate**

| Class | Instance | Status |
|---|---|---|
| In‑contract, verified identical | Window sequence decisions on every frame of each fixture's tested prefix (per `CASES` entry, frames `0..=max`) (D‑020) | **parity** |
| Test‑harness limitation | 10 oracle short‑block "blips" vs real `aac-enc` (D‑021) | oracle‑side, not a C/Rust production divergence |
| Out‑of‑contract in both encoders | short‑window LFE (block switching forces long in C and Rust) (D‑022) | **C4**, panic at `energy.rs:221` (secondary `psy.rs:1300/1302`); C behaviour untested |
| Out‑of‑contract/invariant | mixed‑window CPE `short_l ≠ short_r` (D‑022) | **C4**, panic at `psy.rs:957` (right‑short) or `psy.rs:1300/1302` (left‑short); C behaviour untested |

Additionally, the real corpus divergence *begins* on short/transient frames for most synthetic
fixtures (D‑051 first‑divergence indices: sine f1, stepdown f1, sweep f1, tonal‑noise f1,
sine‑441 f1, onset f24, onset‑441 f22, click f2, multitone f2) — consistent with the documented
cause class (frontend noise on transients), but this is a correlation, not proof.

*R:* verified + recorded. *Confidence:* high for the classification; the plain statement "Rust
`SHORT_WINDOW` handling is broken" is **not** supported by any evidence.

### D. Flush and termination differences — **no frame‑count or framing difference found**

* Encoded **output length**: differs only as a consequence of payload content (D‑051); no
  length difference is attributable to termination.
* **Valid AAC frame content**: both encoders produce the same number of ADTS frames for all 18
  fixtures (D‑060), with identical header fields.
* **ADTS framing**: byte‑identical headers; frame lengths consistent with the parsed boundaries.
* **Termination behaviour**: the zero‑pad‑to‑nDelay law is implemented on both sides and
  reproduces every observed frame count. What differs is the **content** of trailing frames
  (final‑frame payload differs in 11/18 fixtures) — i.e. termination semantics agree, downstream
  coding decisions do not.
* The only recorded "flush" mismatch is oracle‑internal (three appended zero frames, D‑061).

*R:* verified. **C3** (no divergence) / **C6** (the C output's last frame vs Rust's last frame has
no per‑stage ground truth).

### E. Floating‑point and numerical differences — **understood as a class, unquantified in effect**

* Design: float‑native Rust vs Q31 C; policy = identical decisions + bounded numerical agreement
  (spec §5). The single documented approximation point in the integer lane is `map_to_q31`
  (`energy.rs:152-159`).
* Downstream relationship documented: M3 traces show small intermediate differences flip discrete
  TNS decisions at Q15 borders (spec §11/§11.1); m4 §8 predicts the same at scalefactor/codebook
  boundaries; the corpus payload differences (D‑051) are consistent with that prediction.
* Bounded but non‑trivial gates: `32 + |want|/500` for Q31 lanes, `32.0 + e_want / 500.0`
  (`e_want = ld_to_e(...)`) for Ld lanes, `8 + |want|/1000` for TNS gain — all allow changes large
  enough to move decisions; the
  *decision‑bearing* integer fields (scf, gain, sections, bit counts) are compared exactly.
* **Not established:** that any of these differences degrades audio quality — and equally, they
  must not be waved away as harmless. No metric quantifies them (§6.1).

*R:* verified + recorded. **C3** (mechanism) / **C2** (effect).

### F. Existing ADTS and corpus validation — **accurate, with boundaries and exceptions**

* **ADTS header parity:** verified identical on all frames of all 18 fixtures, and the recorded
  first‑frame vectors (§1.4) reproduce exactly [derived‑here]. This is a **strong** statement about
  framing/transport, and a **weak** statement about content.
* **Corpus:** 18/18 fixtures encode, both modes; output hashes stable across every milestone from
  pre‑M5.16 through post‑M5.22 (`docs/aac-enc-performance-campaign-m5.12-m5.22.md` §6); FFmpeg
  decode 18/18 (reviewer‑attested).
* **C‑vs‑Rust content (new):** table below. `frames` C/Rust, `bytes` C→Rust, `first` = first frame
  whose payload differs, `#diff` = number of differing frames, `last` = whether the final frame
  differs.

| fixture | frames | C bytes | Rust bytes | Δ | first | #diff | last |
|---|---|---|---|---|---|---|---|
| m2‑silence‑stereo‑48k | 49/49 | 735 | 735 | 0 | — | 0 | – |
| m2‑sine‑stereo‑48k | 49/49 | 3104 | 3147 | +43 | 1 | 44 | DIFF |
| m2‑onset‑stereo‑48k | 49/49 | 2318 | 2359 | +41 | 24 | 23 | DIFF |
| m2‑stepdown‑stereo‑48k | 49/49 | 2209 | 2236 | +27 | 1 | 22 | – |
| m2‑click‑mono‑48k | 25/25 | 2739 | 2680 | −59 | 2 | 23 | DIFF |
| **m2‑impulse‑mono‑48k** | 11/11 | **308** | **306** | **−2** | **5** | **1** | – |
| m2‑edge‑mono‑48k | 14/14 | 415 | 415 | 0 | — | 0 | – |
| m3‑multitone‑stereo‑48k | 49/49 | 4726 | 4819 | +93 | 2 | 41 | DIFF |
| m3‑sweep‑stereo‑48k | 49/49 | 4088 | 3988 | −100 | 1 | 43 | DIFF |
| m3‑noise‑mono‑48k | 25/25 | 10601 | 10603 | +2 | 15 | 2 | DIFF |
| m3‑tonal‑noise‑stereo‑48k | 25/25 | 10628 | 10630 | +2 | 1 | 7 | – |
| m3‑quiet‑sine‑stereo‑48k | 49/49 | 4189 | 4200 | +11 | 0 | 20 | DIFF |
| m3‑sine‑stereo‑441k | 45/45 | 3014 | 2998 | −16 | 1 | 28 | DIFF |
| m3‑onset‑stereo‑441k | 45/45 | 2247 | 2250 | +3 | 22 | 15 | DIFF |
| m6‑stress‑stereo‑48k | 25/25 | 10728 | 10728 | 0 | 10 | 3 | – |
| m7‑music‑stereo‑48k | 471/471 | 251279 | 250926 | −353 | 1 | 435 | DIFF |
| m7‑speech‑pauses‑stereo‑48k | 471/471 | 125996 | 125818 | −178 | 1 | 430 | DIFF |
| m7‑speech‑clean‑stereo‑48k | 471/471 | 127971 | 127900 | −71 | 0 | 452 | DIFF |

* Boundaries of these claims: header parity ≠ bitstream parity ≠ perceptual equivalence. Two files
  match byte‑for‑byte (silence, edge) and one file differs in a single frame (impulse); the rest
  differ broadly, with both size directions present, so neither encoder is systematically larger.
* Known exceptions recorded elsewhere: the oracle's 10 SHORT_WINDOW replay blips (D‑021); the
  documented input‑driven divergences (D‑011/D‑012); `m6‑stress` has *no* differing frame length on
  3 content frames (differing payload, equal length).

*R:* verified. **C3** (framing) / **C2** (content).

---

## 5. What these findings do **not** establish

1. **Nothing about audio quality.** No SNR/SDR, NMR, PE, spectral‑distance or listening measurement
   exists for any fixture; byte counts and frame lengths are not quality metrics.
2. **Nothing about C/Rust perceptual equivalence** — byte‑identity claims in earlier milestones
   compare Rust against *previous Rust*, never against C.
3. **Nothing about standards violations.** No bitstream was rejected by a decoder (18/18 FFmpeg,
   reviewer‑attested), but no independent parser/conformance check of section data, codebooks or
   TNS syntax was performed on the C or Rust streams.
4. **Nothing about the audibility of D‑012's active‑filter spectrum changes** — a changed active
   FIR can be anywhere from inaudible to significant; both claims would need evidence.
5. **Nothing about robustness in the field** — the panic paths (D‑022) and the asserted‑absent
   paths (D‑014, D‑040, D‑082) are unexercised, and no fuzzing exists for the encoder.
6. **The two‑byte impulse difference is not shown to be harmless**, and equally is not shown to be
   a defect.

---

## 6. Evidence gaps and limitations

### 6.1 Gaps

| # | Gap | Consequence |
|---|---|---|
| G1 | **No quality metric at all** (spec §6 metric list unexecuted; m4 §10 exit criterion 5 deferred to "M7") | Every quality statement in this audit must be "unverified" |
| G2 | No production‑stage dumps from the *real* Rust encode (S1/S2/S4/S5/S6 per frame) | D‑051 (including the §4.A impulse attack frame) cannot be attributed to a first diverging stage without new instrumentation |
| G3 | C oracle vectors exist only for the 15 synthetic fixtures; **no C ground truth for `m7-*`** | Real‑audio divergences (music: 435/471 frames) can only be compared whole‑file |
| G4 | FFmpeg 18/18 decode is reviewer‑attested; no raw logs preserved | Decodability claim is second‑hand |
| G5 | `validate_s*.py` 3‑run stability logs not preserved (conclusions only, in docs) | Oracle determinism is documented, not re‑checkable |
| G6 | The M5.20 record cites `energy.rs:194` for the short‑window LFE panic, which is a stale line number | **Resolved by this audit**: the same code sits at `energy.rs:205-224` with the decisive index at `energy.rs:221`, and the two independent mixed‑window panic sites are `psy.rs:957` / `psy.rs:1300-1302` (both verified against the current tree). The *panic observations themselves* remain recorded‑only |
| G7 | The oracle's 10 SHORT_WINDOW blips: cause recorded as "upstream short‑block inputs", not isolated | Weakened trust in short‑block oracle frames (the very frames that matter for D‑012) |
| G8 | C‑side behaviour on out‑of‑contract inputs (short‑window LFE, mixed‑window CPE, live PNS/IS, retries) unrecorded | Cannot classify those as "identical" or "better/worse" |
| G9 | Legal: FDK licence + AAC patent notes ride along (spec §0/§8.6, m4 R5) | Not a technical finding; must surface in release notes |

### 6.2 Limitations of this audit's own methods

* The C‑vs‑Rust comparison uses one C binary (`850a8e6e…`) and one Rust binary state; a different
  C build (e.g. `fdk-aac-build/aac-enc`, md5 `e0900e…`) could differ. The reference used is the one
  recorded as Phase‑0 and matching the M5.21 note.
* The corpus comparison is whole‑file; frames are aligned only by ADTS parse (no PCR/timeline
  alignment), which is sound because frame counts and lengths are self‑describing.
* **A probe that did not work (reported for completeness):** I attempted to prove input identity by
  locating the committed `ring_*` vectors inside the C input WAVs; no exact slice match exists
  because `ring` holds **normalized floats in a rotating buffer** (`transform.rs:440-446, :468-477`:
  `ring[1600..2048]` = current frame's first 448 samples, `ring[1024..1600]` = previous frame's
  remaining samples). Input identity therefore rests on the committed
  `synthesis_matches_oracle_input` pin (`transform.rs:451`) plus the WAV/­sample‑count checks
  (D‑003), not on this probe.
* No C or Rust code was executed; no oracle was re‑run; all conclusions come from existing
  artifacts and source reading (plus the ADTS arithmetic above).

### 6.3 Conflicts between sources (recorded, not reconciled)

* `docs/aac-enc-migration-spec.md` §11.2 lists the relative TNS gain gate as a **proposal**; §11.4
  and the code (`tns.rs:706-720`) show it **implemented**. The code is strongest.
* The M5.24 report repeated "impulse 306 vs 308 B" as an open divergence; this audit confirms it and
  adds the per‑frame localization. No contradiction, only added detail.
* `tools/enc-oracle/README.md:421-425` and m4 §17.4 describe the SHORT_WINDOW blips slightly
  differently in emphasis (harness limitation vs "upstream of S6"); both agree it is not a proven
  production divergence.
* **Stale line number in the campaign record:** the short‑window LFE panic is recorded at
  `energy.rs:194` (M5.20 report, repeated in `docs/aac-enc-performance-campaign-m5.12-m5.22.md`),
  but the same code is at `energy.rs:205-224` in both the current tree and the M5.23 backup, with
  the decisive index at `energy.rs:221`; the source is strongest.

---

## 7. Classification and prioritized findings

### 7.1 Priority order

Ranked by (1) potential quality/correctness impact, (2) strength of evidence, (3) likelihood of
affecting normal in‑contract use, (4) isolatability with a small controlled investigation.
The scope column gives the bounded frame set for the next step (§8).

| Rank | Finding | Class | Why it ranks here | Scope for next step |
|---|---|---|---|---|
| 1 | **D‑012** active‑filter TNS spectrum changes | C2 | Real, affects transients/short blocks in real music; quality explicitly unverified; no metric exists | 40 pinned windows; 11 oracle frames |
| 2 | **D‑051** corpus payload divergence | C2 | Real, 16/18 files, both directions; cause class expected but unquantified | 11 positive + 9 control oracle frames |
| 3 | **§4.A** impulse attack frame (instance of D‑051) | C2 | Fully localized; decisive test available; 2 bytes on one frame | 1 frame (impulse f5) |
| 4 | **D‑010/D‑070** Q31 mapping & float MDCT noise | C3+C2 | Root mechanism behind 1–3; understood, unquantified | all oracle frames |
| 5 | **D‑014/D‑040/D‑082** unexercised paths (IS/PNS, retry, crash, large fill) | C6+C4 | Coverage gap that can hide defects; retries currently skip frames | synthetic + new inputs |
| 6 | **D‑022/D‑080/D‑081/D‑084** out‑of‑contract robustness (panics; 960/multi‑element; C's silent UB replaced by loud errors) | C4 | Needed before any production API; C behaviour unknown | source + targeted probes |
| 7 | **D‑011/D‑013/D‑031/D‑041/D‑053/D‑060/D‑050/D‑020/D‑030/D‑085** | C3 | Understood or verified‑identical; documented neutrality; deliberate C‑scratch mirrors | none |
| 8 | **D‑002/D‑083**, G9 | C4/C3 | Scope/legal notes | none |

### 7.2 Not to be prioritised (explicitly)

* The 306/308 impulse byte count itself — the number is a symptom; the cause is item 3 above.
* Anything performance‑related (C5): no finding in this audit has a demonstrated performance
  component; the campaign already showed allocation work does not move wall clock.
* The oracle's SHORT_WINDOW blips (D‑021) as an *encoder* defect — no evidence supports it.

---

## 8. Proposed objective validation strategy

For each measurement: question → input/reference → expected evidence → what a positive/negative
result establishes → what it does **not** establish. No thresholds are invented; no ABX; no fixes.

**V1 — First‑divergence stage attribution on frozen frames (highest value; drives M5.26).**
*Question:* for the frames where C and Rust payloads differ *and* a C ground truth exists, which
processing stage first leaves the documented agreement gate? *Input/reference:* the 11 intersection
frames (click f11, impulse f5, onset f24/f25/f26, onset‑441 f22, sine f1/f5, sine‑441 f5,
stepdown f2, sweep f10) plus the 9 control frames (edge f4, multitone f1, noise f5, onset f22/f23,
quiet‑sine f5, silence f5, stress f5, tonal‑noise f5); committed `qc1..qc8` vectors + oracle S1/S2
state (`ATS pre`, `S6GSTATE`); a temp instrumented copy of the Rust production path. *Expected
evidence:* per frame and stage, the Rust production vector vs the committed C vector, plus the real
C frame length (`S6 total + 56`). *Positive result (a stage leaves the gate):* names the cause
stage and classifies it (documented numeric class vs structural). *Negative result (all stages
within gate):* shows the difference is produced by the *interaction* (e.g. exact integer decisions
taken from slightly different values), which then requires comparing the quantized spectra and
scalefactors, not the inputs. *Does not establish:* quality impact, or that a difference is a
defect.

**V2 — Per‑frame side‑information comparison of the real streams (cheap, complements V1).**
*Question:* which coding decisions differ (window sequence/grouping, maxSfbPerGroup, sections,
codebooks, scalefactors)? *Input:* the existing 18 + 18 streams. *Method:* parse both raw_data_blocks
(ICS info → section data → scalefactors) and tabulate differences per frame. *Positive:* locates the
divergence in *decision space* (e.g. "same window sequence, different scalefactor in band 12"),
which discriminates the S4/S5 story from the S1/psy story without instrumenting the encoder.
*Does not establish:* why the decision differed, or quality impact.

**V3 — Quality baseline (spec §6 metric list; deferred from M4 by design).**
*Question:* what is the measured quality/bitrate difference between C and Rust output on the
corpus? *Input:* the 18 Rust outputs and 18 C outputs + source PCM; delay‑compensated (nDelay 1600)
alignment per spec §6. *Expected evidence:* per‑fixture SNR/SDR (decoded vs input), average bitrate,
frame‑size histogram, per‑fixture deltas C vs Rust. *Positive (non‑trivial deltas):* identifies where
D‑012/D‑051 matter and bounds the risk. *Negative (deltas at the level of the C encoder's own
run‑to‑run/numeric residue):* supports "no measurable quality impact on this corpus" *for this
corpus only*. *Does not establish:* audibility, or behaviour on other rates/content; and it is not a
substitute for conformance checking.

**V4 — Targeted TNS impact bound.** *Question:* do the D‑012 active‑filter differences change coding
decisions? *Method:* for the pinned active‑filter cases (onset f25, stepdown f2, onset f24 w4/w6),
compare the *post‑TNS* Q31 spectrum (C oracle `SPEC`/S3 vectors vs the Rust production value) and
then the resulting S4 scf/gain/sections. *Positive (bitstream decisions change):* elevates D‑012
from "spectrum change" to "decision change", making V3 mandatory. *Negative:* confines D‑012 to
spectrum‑value noise. *Does not establish:* perceptual effect.

**V5 — Out‑of‑contract behaviour matrix (for D‑022/D‑080/D‑082).** *Question:* what does C do for
short‑window LFE, mixed‑window CPE, live PNS/IS books, and a forced retry/crash, and what should the
Rust contract be? *Method:* driver‑level probes against the C oracle/library on the same inputs
(test‑only harness; production C untouched) + source reading. *Positive:* yields a documented
contract (error vs panic vs encode) for both sides. *Does not establish:* that any such input occurs
in practice.

**V6 — Negative‑control calibration.** *Question:* how large can a stage‑level difference be while
the payload stays identical? *Method:* apply V1/V2 to the 9 control frames (which have ground truth
and no payload difference). *Positive:* gives an empirical "harmless difference" envelope. *Does not
establish:* that differences within the envelope are harmless in general.

**V7 — Conformance/robustness spot check.** *Question:* are the differing streams structurally valid
beyond decodability? *Method:* independent ADTS/raw_data_block validation (the repo already has an
internal ADTS walk, `stream_driver.rs:4092+`) and FFmpeg decode of *both* sides with recorded logs
(closing G4). *Does not establish:* audio quality.

---

## 9. Recommended next milestone

**M5.26 — First‑divergence stage attribution on the frozen oracle frames.**

* **Question (one):** For the 11 frames that have both a committed C ground‑truth vector and a real
  C‑vs‑Rust payload difference, at which processing stage does the Rust production value first
  leave the documented agreement gate — and is that the documented numeric class (frontend float
  residue) or something structural?
* **Bounded scope:** the 11 intersection frames + the 9 ground‑truth control frames (§7.1 scope
  column), one fixture‑frame at a time, no new fixtures, no new corpus, no music/speech frames
  (G3: no C ground truth for them). Instrumentation lives in a **temporary copy** of the worktree
  (the M5.19–M5.22 practice); no worktree file may change.
* **Method:** for each frame, dump the production values for the stages that have committed vectors
  — S1 (`SPEC`, thresholds, PE), S2 (VBR thresholds), S4 (`scf`/`gain`/`quantSpec`), S5
  (sections/bits), S6 (grants/passes/fill) — and compare against
  `rust/src/aac_enc/testdata/qc{1,2,4,5,6}_*` with the project's own comparison policy
  (exact for integer decisions; `32+|want|/500` for Q31 lanes; `8+|want|/1000` for TNS gain);
  include the cross‑frame state entering each frame (`ATS pre`, `S6GSTATE`) because the vectors are
  single‑frame and cannot by themselves distinguish per‑frame noise from adaptive‑state drift.
* **Deliverable:** a per‑frame attribution table (first diverging stage, magnitude, gate status),
  a per‑frame classification against the existing divergence classes, and an explicit statement for
  the impulse attack frame of whether its 2‑byte difference is the documented numeric class or a
  new class.
* **Completion criterion (measurable):** for **all 20 frames**, either (a) the earliest stage that
  leaves the gate is named with a recorded magnitude, or (b) every stage is proven to be inside the
  gate (recorded) — in which case the report states that the payload difference arises from the
  decision machinery acting on in‑gate values, and names the next measurement (V2 side‑info
  comparison).
* **Validation of the result:** the 9 control frames must reproduce their *identical* payloads
  during the same instrumented runs (they are the built‑in negative control); any control frame
  showing a gate violation that changes its payload falsifies the method and must be reported.
* **Explicitly out of scope:** fixing any divergence; changing gates/tolerances; quality metrics
  (V3, next milestone); performance work; psychoacoustic tuning; running the C oracle or rebuilding
  it; touching the worktree or the M5.23 backup.
* **If inconclusive:** if the instrumented dump cannot be matched to the committed vectors (e.g.
  state injection differs), stop and report the mismatch rather than adjusting the vectors; the
  follow‑up becomes V2 (bitstream side‑info comparison), which needs no instrumentation.

*Why this and not the quality baseline:* the task's priority is quality and correctness, but the
*evidence* needed to make a quality measurement meaningful is knowing whether the differing frames
are produced by in‑gate numeric residue (expected, likely benign) or by a structural difference
(a correctness question). M5.26 answers that for a frozen, fully instrumented frame set; V3 then
measures the significance of whatever is found. If M5.26 shows all differences are in‑gate, V3
(or the checksum/bitrate‑distribution part of V3) becomes the immediate priority.

---

## 10. Source evidence index

**Repository**

* `docs/aac-enc-migration-spec.md` — §1.4 ADTS first‑frame vectors; §5 numerical‑compatibility
  policy; §6 corpus + metric plan (SNR/SDR, bitrate, frame‑size histogram; nDelay 1600); §8 risks
  (incl. 960 granules, PNS‑off `sfbTonality`, legal); §10 M2 results; §11–§11.5 TNS/maxscale
  divergence inventory, traces, gain‑gate and maxscale‑neutrality studies, checkpoint.
* `docs/aac-enc-m4-quant-ratecontrol-plan.md` — §7.3 comparison policy; §8 numeric‑representation
  differences; §9 slice definitions; §10 exit criteria + risks R1–R5; §12–§18 S1–S7 records
  (incl. §17.3 oracle coverage, §17.4 `aac-enc` ADTS frame‑size cross‑check + SHORT_WINDOW blips,
  §17.5 four port bugs + QMAX contract + `checkMinFrameBitsDemand` stub).
* `docs/aac-enc-benchmark-plan.md` — M5.7 (unexecuted) benchmark plan; §4 known correctness notes.
* `docs/aac-enc-performance-campaign-m5.12-m5.22.md` — M5.24 campaign review (no quality metric;
  byte‑identity chain; allocation findings).
* `tools/enc-oracle/README.md` — oracle stages S1–S8, output formats, valid‑envelope gates,
  flush‑frame note, SHORT_WINDOW blip note, `globHdrBits = 56`.
* `tools/enc-oracle/{enc_oracle.cpp,make_fixtures.py,extract_s*.py,validate_s*.py,quant_probe.cpp,
  scf_probe.cpp,scf_unit_probe.cpp,bitcount_probe.cpp,bitenc_probe.cpp,s6lit_probe.cpp,tp_probe.cpp}`
* C reference: `cpp/libAACenc/src/{qc_main.cpp:955,bitenc.cpp:1115,aacenc_lib.cpp:759,1091-1114,
  block_switch.cpp:229-244,psy_main.cpp:271-273,486-487,band_nrg.cpp:111-184,aacenc_tns.cpp:766,961,1051,
  sf_estim.cpp,quantize.cpp,bit_cnt.h}`; `aac-enc.c:33-48,75-97,110,119-130,163-172,188-193`.
* Rust implementation (line references verified against the worktree): `config.rs:99,106-109,
  118-127,131-137,184-198`; `channel_map.rs:151-212,227-232`; `adts.rs:176-179`;
  `energy.rs:120-144,152-159,165-183,205-224,426-427,551`; `psy.rs:112-114,348,356-358,478,486-488,
  488,509-534,587-595,650,695,951-973,957,965-967,1004-1023,1252,1300-1302,1397-1409,1461,
  1496-1510,1528-1552,1702-1703,1872-…,2227-…,2932-2964`; `tns.rs:112,203,706-720,722,736-739,936,
  1101-1102`; `block_switch.rs:329-332`; `transform.rs:119-120,440-446,451,468-477`;
  `frame.rs:171-175,256-258,266-267,485-487,508-510,546,572-576,1395,1590-1597`;
  `quant.rs:204-210,307-355`; `scf.rs:190-191,1123,1143-1144`; `pe.rs:163-164,583-587,880-881`;
  `bitcount.rs:149-156,164-166,530-537,733-735`; `bitenc.rs:151-164,202-234`;
  `rate.rs:112,125,128,136,160,178-192,209-210,480-490,1131-1134,1430,1545`;
  `sfb.rs:107-115,267-285,312-313,470`; `pcm.rs:99-122,189,213,241,261`;
  `stream_driver.rs:1-57,500-510,648,655,952,974,1122,1146-1147,3678-3724,4011-4030,4038-4091,
  4092+`.
* Committed C ground truth: `rust/src/aac_enc/testdata/` (16 `mdct_*`, 13 `ring_*`, 47 `psy_*`,
  47 `rxx_*`, 20 `ms_*`, 14 `seq_*`, `qc1`–`qc8` unit vectors (20 files each except `qc3` = 19),
  `qprobe_quant.txt`, `s7probe.txt`,
  `scfunit.txt`, `cfg_*`, `configs-all.txt`), `m55-PROVENANCE.txt`, 3 × `m7-*.wav`.

**Preserved runtime evidence**

* C outputs: `…/T/opencode/phase0-20261015/aac/*.aac`; C oracle dumps
  `…/T/opencode/phase0-20261015/oracle/*.s1.txt`; C binary
  `…/T/opencode/phase0-20261015/bin/aac-enc` (md5 `850a8e6ed56641f453dc12b3c83946f5`).
* m7 C outputs: `…/T/opencode/m55/aac/*.aac`; m7 inputs `…/T/opencode/m55/wavs/*.wav`
  (SHA‑256 identical to the repo testdata copies).
* Rust outputs: `/var/folders/…/T/m52-rust-aac/m58-corpus/*.m58.aac` (+ `m58-smoke`, earlier
  `*.rust.aac`); hash manifests `…/T/opencode/m522/{base,after}_hashes.txt`.
* Corpus/suite results: `…/T/opencode/m522/{corpus_run1,corpus_final,m58_lines_run1,m58_lines_run2,
  ref_m58_lines_preM522}.txt`, `…/T/opencode/m516..m520_*_m58*.txt`; suites and FFmpeg decode
  reported in `…/T/opencode/m520_review_glm53.md`, `…/T/opencode/m522/review_packet.md`.
* Preservation backup: `/Users/jeroen/backups/fdk-aac-m5.23-worktree-20261016T193606Z`
  (`evidence/` holds copies of the above; `verification/` holds the M5.23 hash manifests).

**This audit's scripts (read‑only; results in §2.3):**
`…/T/opencode/m525/{adts_cmp.py,adts_localize.py,header_parity.py,pins_and_impulse.py,inputs.py,
ring_check.py,ring_full.py,intersect.py}`.

---

## 11. Final status of this milestone

* Deliverable: this file only; no encoder, test, fixture, harness, reference‑C or existing‑report
  file was modified; nothing was staged or committed; the M5.23 backup was not written to.
* Known findings A–F all addressed, including those with insufficient evidence (marked C6).
* The plain‑language summary for a reader without this context: **the two encoders agree on
  framing, termination and every integer decision they can both make from the same inputs; they
  disagree in payload content on 16 of 18 fixtures because the Rust frontend produces slightly
  different Q31 inputs (float MDCT → round‑to‑nearest mapping) and those differences flip a
  handful of discrete decisions (TNS filters, band headroom) on transients; the resulting quality
  difference has never been measured.**

---

## 12. V4 localized divergence investigations (2026-10-25, HEAD `3878c45`)

Follow-up to §§4.A–C/E and §8 on two fixtures where the V3 full-corpus baseline
(`aac-enc-m5b-objective-quality-baseline.md` §9) left bounded questions: sweep-stereo
(distributed −100 B / −2.45% payload gap) and onset-stereo-441k (codec-SNR gap 1.27 dB
favoring Rust, +3 B total). No encoder, test, fixture, or gate change was made in V4;
all analysis ran in scratch (`$TMPDIR/v4-*`, temporary) against retained V3 artifacts
and fresh pin-clone (`f14ad40`) oracle runs. Tier-B, D-021, and P2b conclusions stand.

### 12.1 Sweep: distributed payload gap, block switching excluded

- Per-frame ADTS census (independent parser, cross-validated against V3 metrics exactly):
  rust 3988 / C 4088 B over 49 frames each; headers equal (343/343), so the full −100 B
  is payload. Distributed: negatives sum −131 B over ~25 LONG frames (largest f6 −23,
  f8 −17, f3/f19 −10), positives +31 B; f1/f2 ±2 B is residue noise; f0 equal; frames
  20–24 exactly equal; first notable diff f3 (−10 B, first LONG frame).
- Window sequences from a scratch first-element ICS-header parse (not a full AAC decoder;
  later elements explicitly unparsed): single CPE tag 0, `common_window` shared, on every
  frame of both bitstreams. Rust and C agree 49/49 and both match committed
  `seq_sweep-stereo.txt` (f0 LONG_START, f1 EIGHT_SHORT, f2 LONG_STOP, **f3–f48 all
  ONLY_LONG**). **Block switching is excluded as the cause on both sides.**
- Fresh-oracle S1 at f6/f8/f19 is smooth vs neighbors (no MS flips, TNS inactive,
  ordinary thresholds; f10 S1 byte-identical to committed vectors). Live Rust census
  (existing `m5g_branch_census`, scratch outputs only): npass=1, crash=−1 everywhere;
  live seq == oracle on all 49 frames.
- Verdict: **inconclusive on stage, narrowed** — C-side S1 shows no decision anomaly;
  Rust-side S1 (thresholds/energies/TNS/spectra/MS) was not compared (no existing
  capture; committed vectors exist only at f10). Consistent with VBR rate-control
  operating-point shift (decoded-error signs mixed per frame), not a local failure.
  ±2.2% remains a descriptive band, not a gate.

### 12.2 onset441: localized transient difference, causal frames f21–f22

- Error localization (retained WAVs): silent half bit-exact 0.00 RMS both (alignment
  independently confirmed); the entire SNR gap sits in orig block 22000–24000
  (rust 15.93 vs C 22.47 RMS); tone region near-identical. Rust peaks at the exact
  onset sample 22050; C peaks ~700 samples later at 22749/22750.
- **Causal-frame mapping (correction recorded):** an earlier V4 scratch note placed the
  22749 lobe in the f23 span (decoded-sample coordinates). The encoding-cause mapping
  (orig sample *i* encoded in output frame ⌊i/1024⌋, from feeder order + nominal lag)
  puts onset 22050 in **f21 (LONG_START)** and C's peak 22749 in **f22 (EIGHT_SHORT)** —
  the oracle START/SHORT pair. Window sequences from the scratch parse: rust and C
  identical on 45/45 (f21 START, f22 SHORT, f23 STOP, rest LONG), matching committed
  `seq_onset-stereo-44100.txt`. f0–f21 encode bit-identically (15 B); f22 is 310/303
  (+7 rust, Tier-B-pinned via `phase1h_d021_live_frame_reconciliation`).
- **Oracle evidence taxonomy (do not conflate):** committed vectors (Tier-B ground truth)
  vs fresh-oracle S1 (determinism check: f22 58/60 lines identical; the 2 differing lines
  are SPEC-only top-spectrum wobble, 10/1024 coeffs @1002–1020, zero decision movement —
  documented float-residue class) vs **real-C decisions** (passive V4.25 captures).
  At f22 the fresh/committed oracle MSD has 18 bands vs the real-C mask's 26 bands
  (bands 47–54, coinciding with the wobble region): **oracle SHORT-frame MS is not valid
  C ground truth** (Tier-B precedent, reproduced live). MS exclusion therefore rests on
  `phase1f` (live == real-C passive, passing), never on oracle MSD.
- Live Rust census at f22/f23: seq STOP/LONG confirmed; QC scalars reproduce phase1h pins
  (total 2424/static 279 at f22); npass=1, crash=−1. **TNS was not compared**: no capture
  mechanism records live TNS decisions (nor do committed vectors), though C STNSW is
  active on windows 4–7 at f22 — the TNS-first hypothesis is explicitly blocked, not
  resolved. f23 C-side S1 was obtained usable (LONG STOP, MSD digest 1, 26 bands); no
  Rust-side f23 vectors exist (committed or live).
- Verdict: **inconclusive on mechanism, narrowed** — block structure and MS mask excluded;
  open lanes in order: live TNS (blocked), Rust S1 thresholds/energies (ungated), post-S1
  QC/bitenc residue (D-021 class). The gap favors Rust and violates no Tier-B gate: not a
  Rust defect finding, and not evidence of Rust superiority (single fixture, blunt metric).

### 12.3 Decision rationale (recorded, not a gate change)

- **No encoder fix is justified yet**: neither fixture yields a localized defect — sweep
  shows no C-side anomaly and no Rust-side comparison; onset441 shows a same-direction
  residue operating-point difference under identical block structure.
- **No performance/SIMD campaign is warranted**: worst-case throughput is 159× realtime
  (Rust, music) and the median encoder-core gap is 1.34× — hand-vectorization risk is not
  justified by these numbers.
- **Bit-identical Rust/C output is not the acceptance criterion** (consistent with §6.4
  of `aac-enc-vbr5-profile.md`): structural equality plus bounded payload agreement is;
  silence/edge bit-identity and stress byte-identity are recorded as instances, not goals.

### 12.4 Outstanding questions and smallest follow-ups (all unapproved)

1. Live Rust S1/QC capture (test-only, scratch outputs) at sweep f6 and onset441 f22 —
   thresholds → spectra → MS → **TNS first** → S2→S8 — diffed line-by-line vs the fresh
   V4.3 oracle S1. Pass = first differing stage named; fail-safe (full S1 equality
   modulo documented SPEC wobble) = proven post-S1 QC/bitenc residue. The needed harness
   extension (S1Cap/TNS/MS capture) is **proposed, not implemented or approved**.
2. Systematic 44.1 kHz check (one more fixture) — requires a tracked addition; gated on
   (1) showing f22/f23 sequence-level agreement (established) and therefore currently
   the lowest priority.
3. The V3 scratch corpora (`$TMPDIR/v3-*`, `$TMPDIR/v4-*`) are temporary; durable
   conclusions are those recorded here and in `aac-enc-m5b-objective-quality-baseline.md`
   §9. Re-measurement needs only committed fixtures, existing harnesses, and a fresh
   pin-clone build. No report evidence conflicts with another: the f23→f21/f22 mapping
   correction above supersedes the earlier V4 scratch note with the derivation stated;
   raw-argmax candidate variance (pilot exact vs coarse scan) never changed a nominal
   decision.
