# `gm_stage2` semantics and the incremental-candidate argument

## Existing algorithm (`dyn_bits.cpp:294-340`)

After `gmStage1`, `sections` is a flat array indexed by scalefactor-band
start. Each live section occupies `[s, e]`, where `sections[s].sfb_cnt` is
the run length, `sections[e].sfb_start == s`, and non-start elements keep
`sfb_cnt == 1` and `sfb_start == i`. `max_sfb` is the band count for the
group.

`gmStage2` repeats:

1. **Candidate scan** (`findMaxMerge`): walk `i = 0, i += cnt_i` while
   `i + cnt_i < max_sfb`; keep the first entry with `merge_gain[i] > best`,
   starting from `best = 0`. A section is a candidate iff it has a
   successor. Ties keep the earliest start (strict `>`); a non-positive best
   stops the loop.
2. **Merge** at `max_ndx` with `max_next = max_ndx + cnt`: add the counts,
   `section_bits += next_bits - max_gain`, merge the 12-entry lookup row
   (saturating at `INVALID_BITCOUNT`), and re-point the new last element's
   `sfb_start`.
3. **Gain refresh**: recompute the gain of the previous section (the one
   that now merges into the enlarged section) and of the merged section
   itself when it still has a successor.

`calc_merge_gain` = `split_bits - (side_info[cnt1+cnt2] + min_j(lookup1[j]
+ lookup2[j]))`, forced to `-1` when either book is PNS/IS. Gains can be
negative, so "non-positive best" is a real terminator.

## Dependency structure

For a start `i` with successor `j = i + cnt_i`, `merge_gain[i]` depends on:

- `sections[i]` (`sfb_cnt`, `section_bits`, `code_book`),
- `sections[j]` (same fields),
- `bit_lookup[i]` and `bit_lookup[j]`,
- the constant `side_info_tab`.

A merge at `A` absorbs `B = A + cnt_A`:

| Entry | Before | After |
|---|---|---|
| `A` | depends on `B` | depends on `C = A + cnt_A + cnt_B` (if any) |
| `prev(A)` | depends on `A` | depends on the enlarged `A` |
| `B` | candidate iff `B` has successor `C` | no longer a start |
| everything else | — | unchanged |

Only `A`, `prev(A)` and `B` are affected. `B` is a candidate exactly when
`C` exists; if `C` does not exist, `B` was the last section and was never a
candidate, and the enlarged `A` becomes last and leaves the candidate set.

Therefore the candidate set can be maintained as an ordered list with at
most two value updates and one removal per merge. The list order is exactly
the scan order (`0, cnt, ...`), so the strict-`>` first-maximum rule is
preserved by scanning the list from front to back.

## Implementation

- `merge_cand: Vec<(i32, u32)>` holds `(gain, start)` in ascending start
  order; `n` is the live prefix length.
- Initial pass: same walk as `findMaxMerge`, computing gains once.
- After a merge at position `p`:
  - `cand[p-1].0 = calc_merge_gain(prev, A)` when `p > 0`;
  - if `C` exists: `cand[p].0 = calc_merge_gain(A, C)`, then
    `copy_within(p+2..n, p+1)`, `n -= 1`;
  - else: `n = p` (the enlarged `A` is now last).
- The scan is `for (pos, &(gain, _)) in cand[..n]` with `if gain > max_gain`.

Boundary cases:

- `n == 0` (single section, or `max_sfb == 1`): scan returns `(0, 0)` and
  the loop stops, like the original empty walk.
- all gains non-positive: `(0, 0)` -> stop, like the original `max_gain = 0`.
- tie at a positive gain: the earlier list position wins.
- `p == 0`: no previous section, matching `max_ndx == 0`.
- last-section removal: `n = p` also drops any stale entries behind it.

## Why the original and the candidate pick the same sequence

Induction over merges: assume the list is exactly the ordered candidate set
with current gains. The scan returns the same `(gain, start)` as
`findMaxMerge` because the order and strict-`>` rule are identical. The
merge mutates only the three entries above, each of which is recomputed or
removed; all other entries and their dependencies are untouched. Hence the
list remains the exact candidate set with current gains, and the next choice
is again identical. Termination uses the same `max_gain <= 0` test.

The `sfb_start` breadcrumb used by the original for `prev(A)` is replaced by
`cand[p-1].1`, which is the same section start by construction.

## Verification performed

- 20,000 randomized stage-1-shaped states (1-24 bands, run lengths 1-5,
  mixed normal/PNS/IS book pools), compared against a verbatim copy of the
  original `gmStage2` plus `findMaxMerge`: final `sections` and `bit_lookup`
  were bit-identical in every case; 13,930 cases performed at least one
  merge.
- Mutation checks: changing the tie rule to `>=` was caught at case 59;
  changing the last-section removal to `n = p + 1` was caught immediately.
- All 19 fixture hashes and the 40 s long-workload hash `889c3be4ccd53dd7`
  are unchanged.
