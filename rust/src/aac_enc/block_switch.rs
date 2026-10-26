/* Ferrite: this file is part of a Third-Party Modified Version of the
Fraunhofer FDK AAC Codec Library for Android. Modifications and Rust
implementation copyright © 2025–2026 Jeroen Steenvoorde. Modification
notice date: 2026-10-26. See rust/NOTICE for the full license text. */
/* -----------------------------------------------------------------------------
Software License for The Fraunhofer FDK AAC Codec Library for Android

© Copyright 2025 Fraunhofer-Gesellschaft zur Förderung der angewandten Forschung
e.V. All rights reserved.

 1.    INTRODUCTION
The Fraunhofer FDK AAC Codec Library for Android ("FDK AAC Codec") is software
that implements the MPEG Advanced Audio Coding ("AAC") encoding and decoding
scheme for digital audio. This FDK AAC Codec software is intended to be used on
a wide variety of Android devices.

AAC's HE-AAC and HE-AAC v2 versions are regarded as today's most efficient
general perceptual audio codecs. AAC-ELD is considered the best-performing
full-bandwidth communications codec by independent studies and is widely
deployed. AAC has been standardized by ISO and IEC as part of the MPEG
specifications.

Patent licenses for necessary patent claims for the FDK AAC Codec (including
those of Fraunhofer) may be obtained through Via Licensing
(www.vialicensing.com) or through the respective patent owners individually for
the purpose of encoding or decoding bit streams in products that are compliant
with the ISO/IEC MPEG audio standards. Please note that most manufacturers of
Android devices already license these patent claims through Via Licensing or
directly from the patent owners, and therefore FDK AAC Codec software may
already be covered under those patent licenses when it is used for those
licensed purposes only.

Commercially-licensed AAC software libraries, including floating-point versions
with enhanced sound quality, are also available from Fraunhofer. Users are
encouraged to check the Fraunhofer website for additional applications
information and documentation.

2.    COPYRIGHT LICENSE

Redistribution and use in source and binary forms, with or without modification,
are permitted without payment of copyright license fees provided that you
satisfy the following conditions:

You must retain the complete text of this software license in redistributions of
the FDK AAC Codec or your modifications thereto in source code form.

You must retain the complete text of this software license in the documentation
and/or other materials provided with redistributions of the FDK AAC Codec or
your modifications thereto in binary form. You must make available free of
charge copies of the complete source code of the FDK AAC Codec and your
modifications thereto to recipients of copies in binary form.

The name of Fraunhofer may not be used to endorse or promote products derived
from this library without prior written permission.

You may not charge copyright license fees for anyone to use, copy or distribute
the FDK AAC Codec software or your modifications thereto.

Your modified versions of the FDK AAC Codec must carry prominent notices stating
that you changed the software and the date of any change. For modified versions
of the FDK AAC Codec, the term "Fraunhofer FDK AAC Codec Library for Android"
must be replaced by the term "Third-Party Modified Version of the Fraunhofer FDK
AAC Codec Library for Android."

3.    NO PATENT LICENSE

NO EXPRESS OR IMPLIED LICENSES TO ANY PATENT CLAIMS, including without
limitation the patents of Fraunhofer, ARE GRANTED BY THIS SOFTWARE LICENSE.
Fraunhofer provides no warranty of patent non-infringement with respect to this
software.

You may use this FDK AAC Codec software or modifications thereto only for
purposes that are authorized by appropriate patent licenses.

4.    DISCLAIMER

This FDK AAC Codec software is provided by Fraunhofer on behalf of the copyright
holders and contributors "AS IS" and WITHOUT ANY EXPRESS OR IMPLIED WARRANTIES,
including but not limited to the implied warranties of merchantability and
fitness for a particular purpose. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE for any direct, indirect, incidental, special, exemplary,
or consequential damages, including but not limited to procurement of substitute
goods or services; loss of use, data, or profits, or business interruption,
however caused and on any theory of liability, whether in contract, strict
liability, or tort (including negligence), arising in any way out of the use of
this software, even if advised of the possibility of such damage.

5.    CONTACT INFORMATION

Fraunhofer Institute for Integrated Circuits IIS
Attention: Audio and Multimedia Departments - FDK AAC LL
Am Wolfsmantel 33
91058 Erlangen, Germany

www.iis.fraunhofer.de/amm
amm-info@iis.fraunhofer.de
----------------------------------------------------------------------------- */
//! AAC-LC block switching: window-sequence decisions with look-ahead.
//!
//! Float-native port of `cpp/libAACenc/src/block_switch.cpp` (LC path:
//! 8 analysis windows, short frames and look-ahead enabled). Per frame, the
//! fresh 1024 input samples are split into eight 128-sample windows; a
//! second-order high-pass filter feeds per-window energies; an attack is
//! declared when a filtered window energy exceeds ~10x the running average
//! (with a minimum-energy gate); the next window sequence follows from the
//! previous sequence plus previous/current attack flags, i.e. decisions use
//! previous-frame state, never the current frame alone.
//!
//! Fixed-point mapping (the C code is Q31 `FIXP_DBL`; this port computes the
//! same real-number mathematics in `f32`):
//! - Input scaling: the C code shifts 16-bit samples left by 15
//!   (`block_switch.cpp:389-390`), i.e. real value `s = x / 65536`. This
//!   port divides by 65536.0 once, identically.
//! - `fMultDiv2(a, b)` is the halved product (`common_fix.h`), `fMultAdd(x,
//!   a, b) = 2x + ab`, `fPow2Div2(a) = a^2 / 2`. The accumulator update
//!   `acc = fMultAdd(fMultDiv2(0.7, acc), 0.3, e)` is therefore exactly
//!   `0.7 * acc + 0.3 * e` in real arithmetic, and the attack test
//!   `fMult(e, 0.1) > acc` is `0.1 * e > acc`. The IIR
//!   (`t1 = hp1 * (x - s0) / 2`, `t2 = hp0 * s1 / 2`, `s1 = 2 * (t1 - t2)`)
//!   collapses to the direct-form filter
//!   `s1 = hp1 * (x - s0) - hp0 * s1`.
//! - Per-sample energy `(s^2 / 2) / 16` summed over 128 samples equals the C
//!   sum of `fPow2Div2(s) >> (7 - 1 - 2)`; the C sum saturates at
//!   `MAXVAL_DBL`, mapped here to `(2^31 - 1) / 2^31`.
//! - Minimum attack energy `(1e6 * NORM_PCM_ENERGY) >> 7`
//!   (`block_switch.cpp:143-145`, `psy_const.h:158-160`) with
//!   `NORM_PCM_ENERGY = (1/32768)^2` is exactly representable: the Q31
//!   integer is 15625, i.e. `15625 / 2^31 = 1e6 / 2^37` in real units.
//!
//! Parity target: identical window sequences, shapes, and groupings on the
//! corpus (exact integer match), not identical energy integers.

use crate::common::enums::WindowShape;

/// AAC window sequence. Discriminants match `blockType` (`psy_const.h:120-127`)
/// and the bitstream `window_sequence` (`ONLY_LONG_SEQUENCE = 0`,
/// `LONG_START_SEQUENCE = 1`, `EIGHT_SHORT_SEQUENCE = 2`,
/// `LONG_STOP_SEQUENCE = 3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WindowSequence {
    /// Long block of 1024 lines.
    Long = 0,
    /// Long-to-short transition block.
    Start = 1,
    /// Eight short blocks of 128 lines.
    Short = 2,
    /// Short-to-long transition block.
    Stop = 3,
}

/// Number of block-switching analysis windows per frame (LC).
pub const SWITCH_WINDOWS: usize = 8;
/// Analysis window length in samples (1024 / 8).
pub const SWITCH_WINDOW_LEN: usize = 128;
/// Maximum short-block groups per frame (`MAX_NO_OF_GROUPS`).
pub const MAX_GROUPS: usize = 4;

/// Second-order high-pass coefficients (`block_switch.cpp:130-131`).
const HP_COEFF_0: f32 = -0.5095;
/// Second coefficient of the attack-detection high-pass filter.
const HP_COEFF_1: f32 = 0.7548;
/// Running-average weight for the current window (`accWindowNrgFac`).
const ACC_WINDOW_NRG_FAC: f32 = 0.3;
/// Running-average weight for history (`oneMinusAccWindowNrgFac`).
const ONE_MINUS_ACC_WINDOW_NRG_FAC: f32 = 0.7;
/// Inverted attack ratio: energy must exceed ~10x the average
/// (`invAttackRatio`, `block_switch.cpp:137-138`).
const INV_ATTACK_RATIO: f32 = 0.1;
/// Minimum mean-square energy for attacks: `1e6 * (1/32768)^2 / 128`
/// (see module docs; `block_switch.cpp:143-145`). The Q31 integer is exactly
/// 15625, so this is `15625 / 2^31`.
const MIN_ATTACK_NRG: f32 = 15_625.0 / 2_147_483_648.0;
/// Energy saturation matching `fMin(sum, MAXVAL_DBL)`: the Q31 cap is
/// `(2^31 - 1) / 2^31`, which rounds to exactly 1.0 in `f32`. Real window
/// energies cannot exceed ~1.0 anyway (full-scale square wave), so the cap
/// only mirrors the C saturation for pathological inputs.
const MAX_WINDOW_NRG: f32 = 1.0;

/// Window shape per sequence for LC (`blockType2windowShape[1]`,
/// `block_switch.cpp:122-124`): long blocks use KBD, transitions use SINE.
const LC_WINDOW_SHAPE: [WindowShape; 4] = [
    WindowShape::KBD,
    WindowShape::Sine,
    WindowShape::Sine,
    WindowShape::KBD,
];

/// Suggested short-block grouping per attack position
/// (`suggestedGroupingTable`, `block_switch.cpp:191-199`).
const SUGGESTED_GROUPING: [[u8; MAX_GROUPS]; SWITCH_WINDOWS] = [
    [1, 3, 3, 1],
    [1, 1, 3, 3],
    [2, 1, 3, 2],
    [3, 1, 3, 1],
    [3, 1, 1, 3],
    [3, 2, 1, 2],
    [3, 3, 1, 1],
    [3, 3, 1, 1],
];

/// Next sequence from `[last_attack][attack][last_sequence]`, LC look-ahead
/// table (`chgWndSqLkAhd`, `block_switch.cpp:215-227`; LOWOV/WRONG columns
/// are LD-only and absent here).
const NEXT_SEQUENCE: [[[WindowSequence; 4]; 2]; 2] = [
    [
        [
            WindowSequence::Long,
            WindowSequence::Short,
            WindowSequence::Stop,
            WindowSequence::Long,
        ],
        [
            WindowSequence::Start,
            WindowSequence::Short,
            WindowSequence::Short,
            WindowSequence::Start,
        ],
    ],
    [
        [
            WindowSequence::Long,
            WindowSequence::Short,
            WindowSequence::Short,
            WindowSequence::Long,
        ],
        [
            WindowSequence::Start,
            WindowSequence::Short,
            WindowSequence::Short,
            WindowSequence::Start,
        ],
    ],
];

/// Stereo merge of two window sequences (`synchronizedBlockTypeTable`,
/// `block_switch.cpp:411-424`; LOWOV row/column are LD-only).
const SYNCED_SEQUENCE: [[WindowSequence; 4]; 4] = [
    [
        WindowSequence::Long,
        WindowSequence::Start,
        WindowSequence::Short,
        WindowSequence::Stop,
    ],
    [
        WindowSequence::Start,
        WindowSequence::Start,
        WindowSequence::Short,
        WindowSequence::Short,
    ],
    [
        WindowSequence::Short,
        WindowSequence::Short,
        WindowSequence::Short,
        WindowSequence::Short,
    ],
    [
        WindowSequence::Stop,
        WindowSequence::Short,
        WindowSequence::Short,
        WindowSequence::Stop,
    ],
];

/// Persistent per-channel block-switching state.
///
/// Mirrors `BLOCK_SWITCHING_CONTROL` (`block_switch.h:123-148`) for the LC
/// configuration. One instance per channel; stereo pairs are additionally
/// merged with [`synchronize_pair`](BlockSwitch::synchronize_pair).
#[derive(Debug, Clone)]
pub struct BlockSwitch {
    sequence: WindowSequence,
    shape: WindowShape,
    attack: bool,
    last_attack: bool,
    attack_index: usize,
    last_attack_index: usize,
    groups: u8,
    group_len: [u8; MAX_GROUPS],
    max_window_nrg: f32,
    window_nrg: [[f32; SWITCH_WINDOWS]; 2],
    window_nrg_f: [[f32; SWITCH_WINDOWS]; 2],
    acc_window_nrg: f32,
    iir: [f32; 2],
}

impl BlockSwitch {
    /// Initial LC state (`FDKaacEnc_InitBlockSwitching` with `isLowDelay = 0`:
    /// 8 windows, short frames and look-ahead on, `LONG_WINDOW` with KBD
    /// shape, `noOfGroups = MAX_NO_OF_GROUPS`).
    pub fn new() -> Self {
        Self {
            sequence: WindowSequence::Long,
            shape: WindowShape::KBD,
            attack: false,
            last_attack: false,
            attack_index: 0,
            last_attack_index: 0,
            groups: MAX_GROUPS as u8,
            group_len: [0; MAX_GROUPS],
            max_window_nrg: 0.0,
            window_nrg: [[0.0; SWITCH_WINDOWS]; 2],
            window_nrg_f: [[0.0; SWITCH_WINDOWS]; 2],
            acc_window_nrg: 0.0,
            iir: [0.0; 2],
        }
    }

    /// Window sequence decided for the current frame.
    pub fn sequence(&self) -> WindowSequence {
        self.sequence
    }

    /// Window shape for the current frame.
    pub fn shape(&self) -> WindowShape {
        self.shape
    }

    /// Short-block grouping: group count plus four group lengths
    /// (sums to 8 for short blocks, `{1, 0, 0, 0}` otherwise after sync).
    pub fn grouping(&self) -> (u8, [u8; MAX_GROUPS]) {
        (self.groups, self.group_len)
    }

    /// Whether the current frame holds an attack (pre-sync).
    #[cfg(test)]
    pub fn has_attack(&self) -> bool {
        self.attack
    }

    /// Analyze one frame of fresh time-domain samples and decide its window
    /// sequence (`FDKaacEnc_BlockSwitching`, `block_switch.cpp:229-346`).
    ///
    /// `is_lfe` forces long blocks with SINE shape and a single group, and
    /// freezes the analysis history, exactly like the C early return.
    pub fn process(&mut self, fresh: &[i16; 1024], is_lfe: bool) {
        if is_lfe {
            self.sequence = WindowSequence::Long;
            self.shape = WindowShape::Sine;
            self.groups = 1;
            self.group_len = [1, 0, 0, 0];
            return;
        }

        // Save previous-frame attack state and energy histories.
        self.last_attack = self.attack;
        self.last_attack_index = self.attack_index;
        self.window_nrg[0] = self.window_nrg[1];
        self.window_nrg_f[0] = self.window_nrg_f[1];

        // Publish grouping for the previous frame from its attack position.
        self.groups = MAX_GROUPS as u8;
        self.group_len = SUGGESTED_GROUPING[self.last_attack_index];
        self.max_window_nrg = if self.attack {
            self.window_nrg[0][self.last_attack_index]
        } else {
            0.0
        };

        self.calc_window_energy(fresh);

        // Attack search over the eight current windows.
        self.attack = false;
        let mut en_max = 0.0f32;
        let mut en_m1 = self.window_nrg_f[0][SWITCH_WINDOWS - 1];
        for i in 0..SWITCH_WINDOWS {
            self.acc_window_nrg =
                ONE_MINUS_ACC_WINDOW_NRG_FAC * self.acc_window_nrg + ACC_WINDOW_NRG_FAC * en_m1;
            if self.window_nrg_f[1][i] * INV_ATTACK_RATIO > self.acc_window_nrg {
                self.attack = true;
                self.attack_index = i;
            }
            en_m1 = self.window_nrg_f[1][i];
            en_max = en_max.max(en_m1);
        }
        if en_max < MIN_ATTACK_NRG {
            self.attack = false;
        }

        // Attack spreading over the frame border: an attack in the last
        // window of the previous frame repeats as window 0 here.
        if !self.attack
            && self.last_attack
            && self.last_attack_index == SWITCH_WINDOWS - 1
            && self.window_nrg_f[0][SWITCH_WINDOWS - 1] / 16.0 > 0.625 * self.window_nrg_f[1][1]
        {
            self.attack = true;
            self.attack_index = 0;
        }

        self.sequence =
            NEXT_SEQUENCE[self.last_attack as usize][self.attack as usize][self.sequence as usize];
        self.shape = LC_WINDOW_SHAPE[self.sequence as usize];
    }

    /// Merge a stereo pair to a common window (`FDKaacEnc_SyncBlockSwitching`
    /// with `commonWindow = 1`, `block_switch.cpp:426-560`).
    ///
    /// Sequences merge through [`SYNCED_SEQUENCE`](SYNCED_SEQUENCE); grouping
    /// resolves so both channels share one grouping: the louder channel wins
    /// when both are short (ties go right, matching the C `>` comparison),
    /// the short channel wins over a non-short one, and a start/stop mix
    /// yields two groups of four.
    pub fn synchronize_pair(left: &mut Self, right: &mut Self) {
        let merged = SYNCED_SEQUENCE[left.sequence as usize][right.sequence as usize];
        let left_old = left.sequence;
        let right_old = right.sequence;
        left.sequence = merged;
        right.sequence = merged;
        left.shape = LC_WINDOW_SHAPE[merged as usize];
        right.shape = LC_WINDOW_SHAPE[merged as usize];

        if merged != WindowSequence::Short {
            for side in [left, right] {
                side.groups = 1;
                side.group_len = [1, 0, 0, 0];
            }
        } else if left_old == WindowSequence::Short && right_old == WindowSequence::Short {
            let (winner, loser) = if left.max_window_nrg > right.max_window_nrg {
                (&*left, &mut *right)
            } else {
                (&*right, &mut *left)
            };
            loser.groups = winner.groups;
            loser.group_len = winner.group_len;
        } else if left_old == WindowSequence::Short {
            right.groups = left.groups;
            right.group_len = left.group_len;
        } else if right_old == WindowSequence::Short {
            left.groups = right.groups;
            left.group_len = right.group_len;
        } else {
            for side in [left, right] {
                side.groups = 2;
                side.group_len = [4, 4, 0, 0];
            }
        }
    }

    /// Single-channel grouping fixup (mono arm of `FDKaacEnc_SyncBlockSwitching`,
    /// `block_switch.cpp:550-560`): non-short frames use one group.
    pub fn synchronize_mono(&mut self) {
        if self.sequence != WindowSequence::Short {
            self.groups = 1;
            self.group_len = [1, 0, 0, 0];
        }
    }

    /// Per-window plain and high-passed energies of the fresh frame
    /// (`FDKaacEnc_CalcWindowEnergy`, `block_switch.cpp:359-409`).
    fn calc_window_energy(&mut self, fresh: &[i16; 1024]) {
        let mut s0 = self.iir[0];
        let mut s1 = self.iir[1];
        for w in 0..SWITCH_WINDOWS {
            let mut nrg = 0.0f32;
            let mut nrg_f = 0.0f32;
            for i in 0..SWITCH_WINDOW_LEN {
                // Matches `(FIXP_DBL)sample << (32 - 16 - 1)` in real units.
                let x = fresh[w * SWITCH_WINDOW_LEN + i] as f32 / 65_536.0;
                // Direct form of the C two-multiply IIR (see module docs).
                let y = HP_COEFF_1 * (x - s0) - HP_COEFF_0 * s1;
                s0 = x;
                s1 = y;
                nrg += s0 * s0 / 2.0 / 16.0;
                nrg_f += s1 * s1 / 2.0 / 16.0;
            }
            self.window_nrg[1][w] = nrg.min(MAX_WINDOW_NRG);
            self.window_nrg_f[1][w] = nrg_f.min(MAX_WINDOW_NRG);
        }
        self.iir[0] = s0;
        self.iir[1] = s1;
    }
}

impl Default for BlockSwitch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aac_enc::testdata;

    fn sine_frame(amp: f32, phase0: f32) -> [i16; 1024] {
        let mut out = [0i16; 1024];
        for (i, v) in out.iter_mut().enumerate() {
            let t = (phase0 + i as f32) / 48000.0;
            *v = (amp * (2.0 * core::f32::consts::PI * 1000.0 * t).sin()) as i16;
        }
        out
    }

    #[test]
    fn initial_state_matches_init_block_switching() {
        let bs = BlockSwitch::new();
        assert_eq!(bs.sequence(), WindowSequence::Long);
        assert_eq!(bs.shape(), WindowShape::KBD);
        assert_eq!(bs.grouping(), (4, [0, 0, 0, 0]));
        assert!(!bs.has_attack());
    }

    #[test]
    fn silence_never_triggers() {
        let mut bs = BlockSwitch::new();
        let silence = [0i16; 1024];
        for _ in 0..60 {
            bs.process(&silence, false);
            bs.synchronize_mono();
            assert_eq!(bs.sequence(), WindowSequence::Long);
            assert_eq!(bs.shape(), WindowShape::KBD);
            assert!(!bs.has_attack());
        }
    }

    #[test]
    fn sub_threshold_signal_never_triggers() {
        // Amplitude-2 sine: window mean-square energy (~7.5e-9) sits far
        // below MIN_ATTACK_NRG (~7.3e-6), locking the minimum-energy gate.
        let mut bs = BlockSwitch::new();
        let tiny = sine_frame(2.0, 0.0);
        for _ in 0..30 {
            bs.process(&tiny, false);
            bs.synchronize_mono();
            assert!(!bs.has_attack());
            assert_eq!(bs.sequence(), WindowSequence::Long);
        }
    }

    #[test]
    fn lfe_path_freezes_history() {
        let mut bs = BlockSwitch::new();
        let loud = sine_frame(20000.0, 0.0);
        bs.process(&loud, true);
        assert_eq!(bs.sequence(), WindowSequence::Long);
        assert_eq!(bs.shape(), WindowShape::Sine);
        assert_eq!(bs.grouping(), (1, [1, 0, 0, 0]));
        // A subsequent normal frame still sees pristine (zero) history.
        let mut probe = BlockSwitch::new();
        bs.process(&loud, false);
        probe.process(&loud, false);
        assert_eq!(bs.sequence(), probe.sequence());
        assert_eq!(bs.shape(), probe.shape());
    }

    #[test]
    fn sync_merge_table_spot_checks() {
        fn merged(a: WindowSequence, b: WindowSequence) -> WindowSequence {
            let mut l = BlockSwitch::new();
            let mut r = BlockSwitch::new();
            l.sequence = a;
            r.sequence = b;
            BlockSwitch::synchronize_pair(&mut l, &mut r);
            assert_eq!(l.sequence(), r.sequence());
            assert_eq!(l.shape(), r.shape());
            l.sequence()
        }
        assert_eq!(
            merged(WindowSequence::Long, WindowSequence::Long),
            WindowSequence::Long
        );
        // Start/stop mix resolves to short with two groups of four.
        assert_eq!(
            merged(WindowSequence::Start, WindowSequence::Stop),
            WindowSequence::Short
        );
        let mut l = BlockSwitch::new();
        let mut r = BlockSwitch::new();
        l.sequence = WindowSequence::Start;
        r.sequence = WindowSequence::Stop;
        BlockSwitch::synchronize_pair(&mut l, &mut r);
        assert_eq!(l.grouping(), (2, [4, 4, 0, 0]));
        // Non-short merge resets grouping to a single group.
        assert_eq!(
            merged(WindowSequence::Start, WindowSequence::Long),
            WindowSequence::Start
        );
        let mut l = BlockSwitch::new();
        let mut r = BlockSwitch::new();
        l.sequence = WindowSequence::Start;
        BlockSwitch::synchronize_pair(&mut l, &mut r);
        assert_eq!(l.grouping(), (1, [1, 0, 0, 0]));
    }

    #[test]
    fn sync_short_tie_goes_right() {
        // Equal max energies: the C `left > right` comparison fails, so the
        // right channel grouping wins.
        let mut l = BlockSwitch::new();
        let mut r = BlockSwitch::new();
        l.sequence = WindowSequence::Short;
        r.sequence = WindowSequence::Short;
        l.groups = 4;
        l.group_len = [1, 3, 3, 1];
        l.max_window_nrg = 5.0;
        r.groups = 4;
        r.group_len = [3, 3, 1, 1];
        r.max_window_nrg = 5.0;
        BlockSwitch::synchronize_pair(&mut l, &mut r);
        assert_eq!(l.grouping(), (4, [3, 3, 1, 1]));
    }

    #[test]
    fn oracle_window_decisions_match_exactly() {
        // Every frame/channel of every oracle fixture must reproduce the
        // reference sequence, shape, and grouping bit-for-bit. Fixture PCM
        // is resynthesized in-test (formulae identical to
        // tools/enc-oracle/make_fixtures.py); synthesis fidelity itself is
        // anchored by the ring-vector tests in transform.rs.
        for name in [
            "onset-stereo",
            "stepdown-stereo",
            "click-mono",
            "edge-mono",
            "sine-stereo",
            "impulse-mono",
            "silence-stereo",
        ] {
            let frames = testdata::fixture_samples(name);
            let rows = testdata::load_seq(name);
            let channels = frames.len();
            let mut switches: Vec<BlockSwitch> =
                (0..channels).map(|_| BlockSwitch::new()).collect();
            // Oracle appends 3 zero flush frames; the seq files hold them.
            let n_frames = rows.iter().map(|r| r.frame).max().unwrap() as usize + 1;
            let mut ri = 0;
            for f in 0..n_frames {
                let fresh: Vec<[i16; 1024]> = (0..channels)
                    .map(|ch| {
                        let mut block = [0i16; 1024];
                        for (i, v) in block.iter_mut().enumerate() {
                            let t = f * 1024 + i;
                            *v = if t < frames[ch].len() {
                                frames[ch][t]
                            } else {
                                0
                            };
                        }
                        block
                    })
                    .collect();
                for (ch, sw) in switches.iter_mut().enumerate() {
                    sw.process(&fresh[ch], false);
                }
                if channels == 2 {
                    let (l, r) = switches.split_at_mut(1);
                    BlockSwitch::synchronize_pair(&mut l[0], &mut r[0]);
                } else {
                    switches[0].synchronize_mono();
                }
                for (ch, sw) in switches.iter().enumerate() {
                    let row = &rows[ri];
                    ri += 1;
                    assert_eq!(row.frame as usize, f, "{name} frame order");
                    assert_eq!(row.channel as usize, ch, "{name} channel order");
                    assert_eq!(sw.sequence() as u8, row.seq, "{name} f{f} ch{ch} seq");
                    assert_eq!(sw.shape() as u8, row.shape, "{name} f{f} ch{ch} shape");
                    assert_eq!(sw.grouping().0, row.groups, "{name} f{f} ch{ch} groups");
                    assert_eq!(
                        sw.grouping().1,
                        row.group_len,
                        "{name} f{f} ch{ch} group_len"
                    );
                }
            }
            assert_eq!(ri, rows.len(), "{name} row count");
        }
    }
}
