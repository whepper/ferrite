# Equivalence proof and what the assembly showed

## Source under test

`rust/src/aac_enc/quant.rs` (unchanged tree around §21):

```rust
fn accumulate_dist(acc: i32, inv: i32, spec: i32) -> i32 {
    let mut diff = (inv.wrapping_abs().wrapping_sub((spec >> 1).wrapping_abs())).wrapping_abs();
    let scale = count_leading_bits(diff);
    diff = shift_int(diff, scale);                    // first call, line 435
    diff = fpow2(diff);
    let scale2 = (2 * (scale - 1)).min(32 - 1);
    diff = shift_int(diff, -scale2);                  // second call, line 438
    acc.wrapping_add(diff)
}
```

`rust/src/aac_enc/q31.rs`:

```rust
pub(crate) fn shift_int(v: i32, s: i32) -> i32 {
    if s >= 0 {
        (v as u32).wrapping_shl(s as u32) as i32
    } else {
        v >> (-s as u32).min(31)
    }
}

pub(crate) fn count_leading_bits(v: i32) -> i32 {
    if v == 0 {
        0
    } else {
        let a = if v < 0 { !v } else { v };
        (a as u32).leading_zeros() as i32 - 1
    }
}
```

## Proof for the first call

1. `count_leading_bits` returns a value in `0..=31` for **every** `i32`
   input, not only reachable ones. For `v != 0`, `a` is non-zero except for
   `v == -1`, where `a == 0` and `leading_zeros() == 32` (so the result is
   `31`). Hence `leading_zeros() - 1` is in `0..=31`.
2. Therefore `scale >= 0` at the first call site. The `else` arm of
   `shift_int` is unreachable there.
3. In the `s >= 0` arm, `shift_int(v, s)` is exactly
   `(v as u32).wrapping_shl(s as u32) as i32`. Since `0 <= s <= 31`, the
   `u32` conversion is value-preserving and `wrapping_shl`'s implicit mask
   (`rhs & 31`) is the identity. The result is the same 32-bit pattern
   reinterpreted as `i32`; no arithmetic, rounding or overflow behaviour
   changes.
4. Boundary values: `diff == 0` -> `scale == 0` -> `0`; `diff == i32::MIN`
   -> `count_leading_bits == 0` -> `i32::MIN`; `diff == i32::MAX` ->
   `scale == 0` -> unchanged; `diff == 1` -> `scale == 30` ->
   `0x4000_0000`. Each matches the generic call.
5. The second call `shift_int(diff, -scale2)` can receive either sign
   (`scale2 == -2` for `scale == 0`, `scale2 >= 0` otherwise) and was left
   untouched.

The candidate therefore was exactly equivalent for all inputs. The task's
premise — a dead signed-shift branch at this call site — was correct at the
source level.

## What the compiler had already done

`calc_sfb_dist` contains the inlined `accumulate_dist`. Its first shift was
already emitted as a plain left shift, with no sign select, in both the
production and the dSYM profiling build:

```
clz   w22, w21            ; count_leading_bits
sub   w22, w22, #0x1
cmp   w20, w19            ; diff == 0 (and INT_MIN) special case
ccmp  w21, #0x0, #0x8, ne
csel  w19, wzr, w22, mi   ; scale = 0 for diff <= 0
lsl   w20, w21, w19       ; first shift: already the specialized form
```

The `csel` here implements `count_leading_bits`'s zero/`INT_MIN` guard, not
`shift_int`'s sign test. LLVM had already proved `scale >= 0` and removed
the dead arm.

## Where the profiled 4.80% actually came from

The source-line profile's hot PCs `calc_sfb_dist+0x108` (`asr w20, w20, w22`)
and `+0x10c` (`cmp w19, #0x2`) carry the inline chain

```
shift_int (q31.rs:219) <- accumulate_dist (quant.rs:438) <- calc_sfb_dist
```

`quant.rs:438` is the **second** shift. Its sign really does vary with
`scale`, and the compiler emits a branch-free select between a left and a
right shift. Those instructions are necessary work, not a dead branch. The
§21 recommendation had attributed the line to the first call; that was a
call-site misread, corrected here.

## Codegen result of the candidate

| Item | Before | After |
|---|---|---|
| `calc_sfb_dist` size | 704 B | 704 B |
| `calc_sfb_dist` instructions | 176 | 176 |
| Normalized instruction-stream differences | — | 0 |
| `__text` section size | 0x1bfefc | 0x1bfefc |
| Binary size | 10,930,480 B | 10,930,480 B |
| Text symbols | 2,868 | 2,868 |
| Symbols with size change | — | 20 (test/aux/drop-glue only) |
| `calc_sfb_dist` address | 0x1000f0f00 | 0x100112b00 |

The source rewrite produced no change in the function's instruction
selection or register allocation; the link layout shifted, which is the
likely source of the paired-run spread.
