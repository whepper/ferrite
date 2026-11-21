# 12 — Licensing and Provenance Assessment

Date: 2026-11-16 (rev 2: licensing-decision remediation). This is an
engineering/provenance assessment, **not a legal opinion**. It preserves the
existing Ferrite licensing arrangement; it grants no clearance. Open legal
questions (replacement-term application per artifact, patent authorization for
distribution) remain for qualified review, per the repository `AGENTS.md`.

## 0. Authoritative licensing decision (owner, binding)

- **FPE remains part of Ferrite and retains the existing FDK-AAC licence.**
- **No separate licence** is introduced for FPE.
- **No change** to the project's SPDX identifier (`FDK-AAC`), licence files
  (`rust/LICENSE-FDK-AAC`, `rust/NOTICE`, root `NOTICE`), or attribution
  approach.
- The existing FDK-AAC licensing and provenance requirements for translated or
  derived code **apply to FPE** (per-file TPMV notices, package-level license
  text, provenance recording).
- New FPE source files, documentation, and contribution guidance are consistent
  with this decision (`00`, `11`, ADRs).
- This package makes **no categorical legal conclusion** beyond the project's
  documented compliance position; the recommendation for qualified legal review
  is retained.

## 1. Licenses in play (verified)

| Material | License | Evidence |
|---|---|---|
| Upstream FDK-AAC 3.0.0 | Software License for The Fraunhofer FDK AAC Codec Library for Android (FDK-AAC); **no patent license** | `cpp/…` file headers; `METADATA` (`BY_EXCEPTION_ONLY`) |
| Ferrite codec (`rust/src`) | FDK-AAC, as a **Third-Party Modified Version** (TPMV) | `rust/Cargo.toml:7-8` (`license="FDK-AAC"`, `license-file`); `rust/NOTICE`; `rust/LICENSE-FDK-AAC`; root `NOTICE` |
| `rust/convert` (aac-convert) | MIT OR Apache-2.0, independently developed | `rust/tools/check-licensing.sh` boundary; `AGENTS.md` |
| FPE (new) | **FDK-AAC TPMV** — same licence as the encoder; treated as translated/derived codec code (§0, §3) | owner decision; this assessment |

## 2. FDK-AAC obligations relevant to FPE (from the license text)

- Retain the **complete license text** in source redistributions; in binary
  redistributions retain it in documentation/materials **and** make the complete
  corresponding source available free of charge.
- Modified versions must carry **prominent notices** stating that the software
  was changed and the **date** of change.
- For modified versions, the designation "Fraunhofer FDK AAC Codec Library for
  Android" must be replaced by "**Third-Party Modified Version of the Fraunhofer
  FDK AAC Codec Library for Android**".
- No copyright license fees; no Fraunhofer endorsement; **no patent license**
  (patent authorization is separate).

Ferrite already satisfies the package-level obligations (`rust/NOTICE`,
`rust/LICENSE-FDK-AAC`, root `NOTICE`, per-file headers, `check-licensing.sh`).
FPE inherits them; it does not create a new licensing regime.

## 3. Translation does not remove obligations (project compliance position)

FPE is a **translation of FDK's algorithms, constants, tables, and structure**
into Rust floating-point form. The project's documented compliance position
(`AGENTS.md`, `docs/aac-enc-provenance-inventory.md`) treats translated/derived
codec code as part of the Ferrite Third-Party Modified Version and requires the
FDK-AAC notices and provenance for it. **FPE follows that same treatment**: it
is codec code under FDK-AAC TPMV, not a separately licensed or "independent"
work. This is the project's compliance posture, not a legal adjudication of
derivative-work status; the underlying legal characterization remains for
qualified review.

### Distinguishing the material (assignment §5)

The categories below inform provenance handling. The "character" column is a
working distinction for attribution, **not** a legal conclusion.

| Category | Examples | Working character | FPE handling |
|---|---|---|---|
| Mathematical algorithms / concepts | masking-envelope idea, pre-echo clamp concept, energy = Σx² | ideas/methods as such | FPE implements the same methods; the specific FDK expression is what the project treats as derived |
| Numerical constants / tuning parameters | C_RATIO 0x02940a10, clipEnergy 0x773593ff, slopes 1.5/3.0/2.0, 0x0148 | values/arrangement as FDK tuning | FPE reuses the **same values** (derived `q31/2^31`); provenance recorded |
| Data tables | SFB widths, SPREAD_* factors, minSnr snapshots, grouping/sequence tables | ROM extracts (licensed data) | already in repo under license (`sfb_tables.rs`, `cfg_*.txt`); FPE consumes, does not re-extract or re-license |
| Original source-code expression | C `psy_main.cpp`/`band_nrg.cpp` code | upstream expression | FPE does **not copy C code**; it re-expresses the algorithms in Rust. The Q31 mirror (already a licensed translation) is the structural reference |
| Independently written Rust implementation | hypothetically clean-room code | could be separate | FPE is **not** clean-room; it is a faithful translation, treated as derived. Do not label FPE "independent" |

**Compliance conclusion**: FPE files are codec code under FDK-AAC TPMV (the
existing licence, §0). They are not standalone utilities/application code (the
`AGENTS.md` exception for `rust/convert`, `tools`, build scripts does not
apply). No licensing file, SPDX identifier, or NOTICE is changed.

## 4. Source-provenance and attribution strategy for FPE

1. **Per-file notice**: new `fpe/*.rs` files carry the Ferrite TPMV notice in
   the **concise module-doc form** used by recently authored codec files
   (e.g. `encoder.rs:1-4`), adapted to `//!`:
   ```rust
   //! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK
   //! AAC Codec Library for Android. Modifications and Rust implementation
   //! copyright © 2025–2026 Jeroen Steenvoorde. Modification notice date:
   //! <FPE implementation date>. Package: ferrite-aac (package license
   //! FDK-AAC); see rust/NOTICE for the full license text.
   ```
   The candidate `SPDX-License-Identifier: FDK-AAC` form in `AGENTS.md` records
   the packaging identifier; it is **not** independent per-file licensing proof.
   Match the surrounding file style; do not mix forms.
2. **Modification date**: FPE files carry their own implementation date (the
   date the FPE modification is made). **Preserve** the existing baseline
   notice date `2026-10-26` wherever it already appears; do not rewrite existing
   headers.
3. **Package-level**: no change needed — `rust/NOTICE`, `rust/LICENSE-FDK-AAC`,
   root `NOTICE` already ship the complete text and TPMV designation. FPE adds
   no new package-level obligation beyond what Ferrite already meets. Verify
   `cargo package --locked -p ferrite-aac --list` still includes them.
4. **Algorithmic provenance record**: `03` is the function-level mapping
   (upstream file:function:lines ↔ Q31 mirror ↔ FPE). Each FPE module doc cites
   its upstream counterpart(s). This documents derivation without copying C
   expression.
5. **No ad-hoc upstream copying**: do not paste C code into FPE. Constants and
   tables are consumed from the existing licensed Rust data files. If a new
   table were ever needed, it must go through the established oracle-extraction
   tooling with provenance (not hand-copied), and only with approval.
6. **No bulk header rewrites**; do not touch the history of the licensing
   commit `e3aeb7f` (`AGENTS.md`).
7. **Hygiene gate**: `rust/tools/check-licensing.sh` (accepts full-block or
   concise notice for `rust/src/**/*.rs`) + `git diff --check` before review.

## 5. Distribution notes (unchanged from Ferrite)

- Binary distributions must include the notices/license text in
  documentation/materials and make complete corresponding source available
  free of charge; no copyright fees; no patent license granted. Repository
  presence alone does not guarantee compliance for any future distribution
  method (root `README.md` guidance). FPE does not change this posture.
- The FDK-AAC license grants **no patent license**; patent authorization for
  any distribution is separate and remains an open question for qualified
  review.

## 6. Provenance evidence base and gaps

- Ferrite provenance inventory: `docs/aac-enc-provenance-inventory.md`
  (2026-10-27): 53 LIKELY_TRANSLATED_UPSTREAM, 4 GENERATED (tables), 13
  MIXED_ORIGIN, 58 POSSIBLY_INDEPENDENT. FPE adds new LIKELY_TRANSLATED_UPSTREAM
  files (faithful float translations); record them in a future inventory pass.
- Upstream tree availability: `3878c45a:cpp/` (git history); pinned reference
  `f14ad40a` re-provisioning (A-3, risk R-12).
- **Gap**: no line-by-line C→Rust diff was performed in the inventory
  (classification rests on anchors/history). FPE's `03` mapping is
  function-level, verified by reading; a future audit may add line-level
  evidence. This is a hygiene signal, not legal proof.

## 7. Recommendation

Per the authoritative decision (§0), treat FPE as **FDK-AAC TPMV codec code**
under the **existing** licence: concise per-file TPMV notices with FPE
modification dates, **package-level license / SPDX identifier / NOTICE
unchanged**, function-level provenance in `03`/module docs, no ad-hoc upstream
copying, no regeneration of committed tables, hygiene gate enforced. This
follows the project's documented compliance position for translated/derived
code; it is not a categorical legal conclusion. Escalate any legal question
(replacement-term per artifact, patent authorization) to qualified review — do
not resolve it in code.