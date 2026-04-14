# M0 Walking Skeleton — Code Review

**Date:** 2026-04-15
**Scope:** Everything introduced or modified in the three M0 commits
(`1e26e08`, `4cec1a5`, `005b9ae`).

---

## Critical — policy violation

### 1. `src/backend/libavm.rs` — all `unsafe` blocks missing `SAFETY:` comments

Every non-kernel module is required by project policy to carry a `SAFETY:`
comment on every `unsafe` block. `libavm.rs` has none. Affected locations:

- Lines 53–54: raw pointer cast/dereference in `fb_get_shim`
- Lines 76–77: raw pointer cast/dereference in `fb_release_shim`
- Lines 93–118: `MaybeUninit::assume_init()` after C init in `new()`
- Lines 122–132, 136–145, 149–170: FFI calls in `decode()`, `flush()`, `get_stream_info()`
- Lines 186–198, 210–212: `set_frame_buffer_functions()`, `set_frame_buffer_manager()`
- Lines 217, 223, 230: `Box::from_raw` reclaims in normal and error paths
- Lines 249–266, 272–277: `refresh_stream_info()` and `Drop::drop()`

The file also has no `#![deny(unsafe_op_in_unsafe_fn)]` at the file level,
meaning the compiler will not catch unsafe operations added inside
`unsafe extern "C"` functions without a nested `unsafe {}` block.

---

## Important — will cause real problems in later milestones

### 2. `src/backend/rust.rs:289` — `zeroed_image()` unsafe block has no `SAFETY:` comment

```rust
fn zeroed_image() -> avm_image_t {
    unsafe { MaybeUninit::<avm_image_t>::zeroed().assume_init() }
}
```

The safety condition (all-zero is a valid initialized state for this C struct)
is non-trivial and must be stated. The equivalent call in
`src/decoder/mod.rs:876` does have a comment; this one is inconsistent.

### 3. `src/backend/rust.rs:127` — `frame_payload[1..]` is a silent assumption

```rust
let tile_group = parse_tile_group(&sequence_header, &uncompressed, &frame_payload[1..]);
```

The `[1..]` slice assumes the uncompressed frame header is exactly one byte
long. This is only true for the reduced-still-picture single-tile path.
`parse_uncompressed_frame_header` does not return its consumed byte count, so
when the non-reduced path is enabled in M1+ this will silently point at garbage
rather than panicking. Needs either a return value from the parser or a
prominent comment tagging it for M1 cleanup.

### 4. `src/bitstream.rs:957–959` — `parse_obus_auto` silently discards the first error

```rust
parse_obus(data).or_else(|_| parse_annexb_obus(data))
```

If `parse_obus` fails for a reason other than "this is Annex-B" (e.g.
`InvalidHeader` on a corrupt packet), the original error is discarded and the
Annex-B error is returned instead. Callers get the wrong error code. Either
distinguish formats before trying, or explicitly document the swallowing
behaviour.

### 5. `src/bitstream.rs:316–321` — `ObuType::RedundantFrameHeader` and `TileList` are dead code

`from_raw` maps raw type 7 → `TileGroup` (should map to `RedundantFrameHeader`)
and raw type 8 → `Metadata` (should map to `TileList`). Both enum variants are
referenced in `classify_frame_packet` but can never be returned by `from_raw`,
making those branches unreachable. A real `RedundantFrameHeader` OBU will be
silently treated as a tile group. The mapping needs to be corrected against the
AV2 OBU type table.

### 6. `src/bitstream.rs:1097–1098` — `force_screen_content_tools` and `force_integer_mv` hardcoded

In the non-reduced sequence header path these are actual bitstream syntax
elements, not defaults. The code hardcodes them to 0 and 2 respectively without
reading the bitstream. Any stream that sets `force_screen_content_tools = 1`
will be silently misdecoded.

### 7. `src/decoder/core.rs:216–221` — `gather_left_4` panic safety relies on an undocumented outer precondition

```rust
fn gather_left_4(plane: &PlaneBuffer<u8>, bx: usize, by: usize) -> [u8; 4] {
    [
        plane.row(by)[bx - 1],
        plane.row(by + 1)[bx - 1],
        plane.row(by + 2)[bx - 1],
        plane.row(by + 3)[bx - 1],
    ]
}
```

`plane.row(y)` panics if `y >= height`. The function is safe only because
`frame_height % 4 == 0` is checked at the frame level. That dependency is
completely implicit. Add a comment tying this function's safety to that outer
precondition.

### 8. `src/decoder/entropy.rs:64–65` — `assert!` in production hot-path

```rust
assert!(cdf.len() >= 2);
assert_eq!(cdf[cdf.len() - 1], 32767);
```

These fire as panics in release builds on bad input. A corrupt bitstream should
yield `Err(...)`, not abort the process. Currently safe because CDFs are
compile-time constants, but will become a crash vector as soon as
context-adaptive CDFs are introduced in M1+. Convert to `debug_assert!` or
return an error.

---

## Suggestions

### 9. `src/decoder/frame_buffer.rs` — missing `#![forbid(unsafe_code)]`

Every other pure-Rust decoder module carries this attribute. The file has no
unsafe code today, but without the lint attribute there is nothing to prevent
unsafe code from being added in a future PR without review notice.

### 10. `src/decoder/kernels/` — document why unsafe is permitted

The spec carves kernels out of the `forbid(unsafe_code)` policy to allow future
SIMD intrinsics. Neither `mod.rs` nor `scalar.rs` documents this intent. A
comment would prevent reviewers from flagging the missing attribute as an
oversight.

### 11. `src/decoder/kernels/scalar.rs:10` — document the DCT fixed-point convention

`COSPI_16_64 = 11585` is correct (`⌊√2 · 2¹³⌋`) but the file has no comment
explaining the 14-bit cosine / 12-bit shift convention or citing the spec
section. The M5 SIMD developer will need to replicate these constants exactly.

### 12. `src/decoder/symbols.rs` — document that CDFs are stub uniform distributions

`PARTITION_NONE_SPLIT_CDF`, `SKIP_CDF`, `INTRA_MODE_CDF`, and `ALL_ZERO_CDF`
are flat distributions, not AV2 default CDF values. Without a comment this
looks like an implementation gap rather than an intentional placeholder for M1.

### 13. `src/decoder/core.rs:152` — explain the `128` fill in `decode_none_block`

The `fill(128)` for a non-minimum `BlockSize::None` partition looks like a
silent error without a comment explaining that this is a stub placeholder not
exercised by the M0 corpus.

### 14. `src/decoder/mod.rs:370` — `get_stream_info` missing doc comment

All adjacent `Decoder` methods have doc comments; this public API method does
not.

### 15. `src/backend/rust.rs:90–95` — comment the hardcoded `number_*layers: 1`

These constants are valid only because of the `reduced_still_picture_header`
guard a few lines above. Nothing ties them together; a future developer relaxing
the guard will not know to update these values.

---

## Summary

| # | Location | Severity |
|---|----------|----------|
| 1 | `src/backend/libavm.rs` — all unsafe blocks missing SAFETY comments + no `deny(unsafe_op_in_unsafe_fn)` | Critical |
| 2 | `src/backend/rust.rs:289` — `zeroed_image` unsafe block undocumented | Important |
| 3 | `src/backend/rust.rs:127` — `frame_payload[1..]` silent byte-offset assumption | Important |
| 4 | `src/bitstream.rs:957` — first error swallowed in `parse_obus_auto` | Important |
| 5 | `src/bitstream.rs:316` — `RedundantFrameHeader` and `TileList` unreachable from `from_raw` | Important |
| 6 | `src/bitstream.rs:1097` — `force_screen_content_tools` / `force_integer_mv` hardcoded | Important |
| 7 | `src/decoder/core.rs:216` — `gather_left_4` implicit panic precondition | Important |
| 8 | `src/decoder/entropy.rs:64` — `assert!` in production hot-path | Important |
| 9 | `src/decoder/frame_buffer.rs` — missing `#![forbid(unsafe_code)]` | Suggestion |
| 10 | `src/decoder/kernels/` — missing explanation for unsafe permission | Suggestion |
| 11 | `src/decoder/kernels/scalar.rs:10` — DCT fixed-point convention undocumented | Suggestion |
| 12 | `src/decoder/symbols.rs` — stub CDFs undocumented | Suggestion |
| 13 | `src/decoder/core.rs:152` — `fill(128)` placeholder unexplained | Suggestion |
| 14 | `src/decoder/mod.rs:370` — `get_stream_info` missing doc comment | Suggestion |
| 15 | `src/backend/rust.rs:90–95` — hardcoded layer counts untied from guard | Suggestion |
