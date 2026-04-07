# Final Security Audit: rustavm — Rust AV2 Decoder Wrapper

**Date:** 2026-04-07
**Auditor:** QA/Security Agent (Phase 4 final review)
**Prior audit:** `SECURITY_AUDIT.md` (Phase 1)
**Scope:** All Rust source in `rustavm/src/` and `rustavm/tests/`

---

## Executive Summary

The Phase 2 safety hardening addressed the two **Critical** findings and most of the
**High** findings from the original audit.  The `transmute`-based callback registration
(C-1) has been replaced with correctly-typed function pointer parameters.  The unsound
`slice::from_raw_parts` path (C-2) now validates negative stride, uses `checked_mul`,
and checks the slice length against `img.sz`.  The `.leak()` memory leak (H-3) has been
replaced with `Cow<'static, str>`.

Three original findings remain **UNRESOLVED** (all Low severity), three are **PARTIAL**
(High and Medium), and one High finding (H-2, IVF frame-size DoS) is **UNRESOLVED**.
Two new findings were identified during this re-audit.

**Overall assessment: The crate is substantially safer than Phase 1.  No Critical or
High-severity _unsoundness_ remains.  The remaining issues are hardening gaps (DoS
resistance, documentation, defense-in-depth) rather than memory-safety violations.**

---

## Original Finding Status

### C-1: Unsound `transmute` of function pointers — RESOLVED

**File:** `src/decoder.rs:155-182`

The `transmute` calls have been completely removed.  `set_frame_buffer_functions` now
declares its callback parameters with exact C-compatible types:

```rust
get_fb: unsafe extern "C" fn(
    priv_: *mut c_void,
    min_size: usize,
    fb: *mut avm_codec_frame_buffer_t,
) -> c_int,
```

The function itself is `unsafe`, correctly signaling that the caller must uphold
invariants.  The test `test_callback_type_safety_no_transmute` in
`tests/frame_buffer_test.rs` serves as a compile-time regression test.

---

### C-2: `slice::from_raw_parts` with negative stride — RESOLVED

**File:** `src/decoder.rs:297-336`

The `plane()` method now performs comprehensive validation before constructing the
slice:

| Check | Lines | Purpose |
|-------|-------|---------|
| `index >= 3` | 298-300 | Reject invalid plane index |
| `plane_ptr.is_null()` | 303-306 | Reject null plane (e.g. monochrome chroma) |
| `raw_stride < 0` | 309-312 | Reject vertically-flipped images |
| `stride < plane_w` | 319-321 | Reject corrupted stride narrower than pixel data |
| `stride.checked_mul(height)?` | 325 | Prevent usize overflow |
| `len > img.sz` when `sz > 0` | 330-333 | Reject slice exceeding total image allocation |

All six checks return `None` rather than panicking.  The original negative-stride UB
path is fully closed.

---

### H-1: `stride * height` overflow — RESOLVED

**File:** `src/decoder.rs:325`

Now uses `stride.checked_mul(height)?` with `?` propagating `None` on overflow.
Subsumed by the C-2 fix.

---

### H-2: IVF frame size denial-of-service — UNRESOLVED (High)

**File:** `src/ivf.rs:53,59`

```rust
let size = u32::from_le_bytes(size_buf) as usize;
let mut data = vec![0u8; size];
```

No maximum frame size cap has been added.  A crafted IVF file can still trigger a
single allocation of up to ~4 GiB.  This remains a denial-of-service vector for any
application that opens untrusted IVF files through this crate.

**Recommendation (unchanged):** Add a configurable `max_frame_size` parameter
(e.g. default 256 MiB) and return `Err` when exceeded.

---

### H-3: Memory leak via `.leak()` — RESOLVED

**File:** `src/bin/avmdec.rs:56-86`

The `colorspace` variable is now `Cow<'static, str>`.  Borrowed variants use
`Cow::Borrowed(...)` and the dynamically-built high-bitdepth string uses
`Cow::Owned(template.replace(...))`.  No `.leak()` call exists anywhere in the
codebase.

---

### H-4: Raw `*mut c_void` with no lifetime safety — PARTIAL

**File:** `src/decoder.rs:148-154,166`

**What was done:**
- `set_frame_buffer_functions` is now `unsafe fn` (was safe in Phase 1)
- The doc comment explicitly documents the lifetime invariant the caller must uphold
- The `ext_fb_manager: Option<Box<dyn FrameBufferManager>>` field exists on `Decoder`
  (line 40) with a comment referencing this finding

**What remains:**
- The `ext_fb_manager` field is still `#[allow(dead_code)]` and not wired to anything
- No safe API alternative exists — all users must use the unsafe raw-pointer API
- The `FrameBufferManager` trait (line 43-47) is defined but unused

The `unsafe` marking with documented safety contracts is a valid engineering choice for
an FFI wrapper.  However, the original recommendation to provide a safe path via
`ext_fb_manager` is unimplemented.  This is acceptable for the current low-level use
case but should be revisited if the crate gains external users.

---

### M-1: Panic unwinding through FFI boundaries — PARTIAL

**File:** `src/decoder.rs:153-154,209-213`

**What was done:**
- The doc comment on `set_frame_buffer_functions` warns about panic-through-FFI UB
  and recommends `catch_unwind`
- The `Drop` impl has a safety note about the same issue

**What remains:**
- No `catch_unwind` wrapper is provided or enforced
- No `extern "C-unwind"` is used (stable since Rust 1.71)
- A user who writes a panicking callback will trigger UB with no compiler or runtime
  guard

**Recommendation:** At minimum, provide a helper function or macro that wraps a closure
in `catch_unwind` and returns an error code to C.  Alternatively, since the project
targets Rust 2021 edition, consider switching callback declarations to
`extern "C-unwind"` where semantically appropriate.

---

### M-2: `plane_width()` and `chroma_plane_height()` missing bounds check — RESOLVED

**File:** `src/decoder.rs:360-362,380-382`

Both functions now check `if index >= 3 { return 0; }` at the top.  Additionally, the
chroma shift values are clamped via `.min(31)` (lines 373, 395) to prevent
panics from oversized shift amounts.

---

### M-3: Thread safety not documented — UNRESOLVED (Medium)

**File:** `src/decoder.rs:35-41`

The `Decoder` struct still has no doc comment explaining its `!Send` / `!Sync` status
or the C library's threading requirements.  The `TEST_STRATEGY.md` document does note
(section 7, "Thread Safety") that `Decoder` is `!Send`/`!Sync`, but this is not visible
to API consumers.

**Recommendation (unchanged):** Add a doc comment on `struct Decoder` explaining thread
safety constraints and how to use multi-threaded decoding (create one `Decoder` per
thread; the C library manages internal threading via the `threads` config parameter).

---

### M-4: `avmdec.rs` plane data access can panic — PARTIAL

**File:** `src/bin/avmdec.rs:129-145`

**What was done:**
- The `plane()` method now returns `None` for many invalid states, so the `if let
  Some(plane) = img.plane(i)` guard in `avmdec.rs:120` skips corrupted planes
  gracefully rather than constructing an invalid slice.

**What remains:**
- Inside the `if let Some(plane)` block, indexing is still unchecked:
  ```rust
  row_buf[x] = plane[start + x * 2];  // line 131
  let row_data = &plane[start..end];   // line 145
  ```
- If `plane()` returns `Some` but stride/width/height are inconsistent (possible with
  corrupted metadata that passes all current checks), these accesses will panic.

The risk is substantially reduced because `plane()` now validates `stride >= plane_w`
and `stride * h <= img.sz`.  However, the `stride >= plane_w` check uses **pixel**
width, not **byte** width.  See **N-1** below for details.

---

### L-1: `std::mem::zeroed()` fragility — UNRESOLVED (Low)

**File:** `src/decoder.rs:56-57,125`

Still uses `std::mem::zeroed()` with no documenting comment about the
zero-initialization contract.  Risk is unchanged — low, but increases maintenance
burden as the C struct evolves.

---

### L-2: `*const avm_image_t` constness mismatch — UNRESOLVED (Low)

**File:** `src/decoder.rs:231`

`Frame` still stores `img: *const avm_image_t` while `avm_codec_get_frame` returns
`*mut avm_image_t`.  The implicit coercion is sound since `Frame` only reads.
Unchanged.

---

### L-3: `build.rs` hardcodes GCC include path — UNRESOLVED (Low)

**File:** `build.rs:21`

`.clang_arg("-I/usr/lib/gcc/x86_64-linux-gnu/13/include")` is still hardcoded.
Unchanged.

---

## New Findings

### N-1 (Medium): `plane()` stride validation incomplete for high-bitdepth formats

**File:** `src/decoder.rs:318-321`

```rust
let plane_w = self.plane_width(index);
if stride < plane_w {
    return None;
}
```

`plane_width()` returns the **pixel** width (e.g. 1920 pixels).  `stride` is in
**bytes**.  For 8-bit formats, 1 byte/pixel, so `stride >= pixel_width` is correct.
For high-bitdepth formats (2 bytes/pixel), the C library sets `stride >= 2 * pixel_width`,
but this check only enforces `stride >= pixel_width`.

If a corrupted frame somehow has `stride == pixel_width` for a 16-bit format, `plane()`
will return `Some(...)`, but the consumer code in `avmdec.rs:131`:

```rust
row_buf[x] = plane[start + x * 2];
```

would access `plane[start + 2*w - 2]` which exceeds the validated `stride * height`
when `stride < 2*w`.  This results in a **panic** (bounds check), not UB.

The comment on line 316-317 acknowledges this is "a conservative but sufficient guard"
relying on the C library setting stride correctly.

**Severity:** Medium (panic/DoS only, not UB)
**Recommendation:** Change the check to account for bytes-per-sample:
```rust
let bps = if ((*self.img).fmt & 0x800) != 0 { 2 } else { 1 };
if stride < plane_w * bps { return None; }
```

---

### N-2 (Low): 17 undocumented `unsafe` blocks

**File:** `src/decoder.rs` (17 instances)

Clippy with `-W clippy::undocumented_unsafe_blocks` flags all 17 `unsafe` blocks in
`decoder.rs` as missing `// SAFETY:` comments.  While the code is functionally correct
and many blocks have nearby doc comments explaining the safety rationale, the formal
`// SAFETY:` convention is not followed.

No `#[allow(unused_unsafe)]`, `#[allow(clippy::undocumented_unsafe_blocks)]`, or other
suppression attributes were found — this is good.  The only `#[allow(...)]` in the
source is `#[allow(dead_code)]` on `ext_fb_manager`.

**Severity:** Low (documentation/maintainability, no correctness impact)
**Recommendation:** Add `// SAFETY:` comments to each unsafe block.  This is
especially important for `plane()` (line 301) which has complex preconditions.

---

## Test Suite Assessment

### Coverage Summary

| Test binary | Total | Pass | Ignored | Fail |
|---|---|---|---|---|
| `rustavm` (unit + bindgen) | 24 | 24 | 0 | 0 |
| `frame_buffer_test` | 23 | 13 | 10 | 0 |
| `md5_verification_test` | 12 | 2 | 10 | 0 |
| **Total** | **59** | **39** | **20** | **0** |

### Test Quality Review

**Positive observations:**
- `test_callback_type_safety_no_transmute` — effective compile-time regression test for C-1
- `test_decoder_dropped_before_manager` — correctly tests the critical drop-ordering invariant
- `test_manager_double_release` — tests error detection for double-free
- `test_manager_exhaustion` — exercises pool boundary conditions
- `frame_md5()` faithfully reproduces the C MD5 logic including the HIGHBITDEPTH downshift path
- Multi-threaded consistency tests (1/2/4 threads) will catch threading regressions once test data is available
- All tests that claim to test something actually test it (no false-confidence tests found)

**Gaps:**
- No tests for negative stride rejection (C-2 fix) — `plane()` returns `None` for
  negative stride, but no test exercises this path
- No tests for `checked_mul` overflow path (H-1 fix) — would require constructing a
  mock `avm_image_t` with adversarial stride/height values
- No tests for `stride < plane_w` rejection
- No tests for `len > img.sz` rejection
- No decode API validation tests (Category A from TEST_STRATEGY.md) — e.g. decode with
  empty data, garbage data, truncated input
- No invalid-file regression tests (Category E from TEST_STRATEGY.md)

These gaps are understandable given that the test infrastructure cannot easily construct
mock `avm_image_t` structs (the C struct layout is complex), and invalid-file tests
require the oss-fuzz test data.

### `#[ignore]` Gating Assessment

The `#[ignore]` pattern with `LIBAVM_TEST_DATA_PATH` is appropriate for the current
development stage:
- Always-run tests (39) cover all code that can be tested without real bitstreams
- Ignored tests (20) are structurally complete and print clear skip messages
- The pattern is consistent across both test files

A cargo feature flag (`--features test-data`) would be marginally cleaner for CI but
offers no practical advantage over `#[ignore]` + `--include-ignored` at this stage.

---

## Callback Soundness Review (Phase 3 additions)

### `get_frame_buffer` / `release_frame_buffer` (frame_buffer_test.rs:222-246)

```rust
unsafe extern "C" fn get_frame_buffer(
    priv_: *mut c_void,
    min_size: usize,
    fb: *mut avm_codec_frame_buffer_t,
) -> c_int {
    if priv_.is_null() || fb.is_null() { return -1; }
    let list = unsafe { &mut *(priv_ as *mut ExternalFrameBufferList) };
    let fb_ref = unsafe { &mut *fb };
    list.get_free_buffer(min_size, fb_ref)
}
```

**Assessment: Sound in practice, with caveats.**

1. **Null checks:** Both `priv_` and `fb` are null-checked before dereferencing.  Good.
2. **Mutable aliasing:** The `&mut` borrow from the raw pointer coexists with the
   `Box<ExternalFrameBufferList>` in the caller.  This is technically mutable aliasing,
   but the `Box` is not accessed during the callback (the C library calls back
   synchronously from within `avm_codec_decode`).  Under both Stacked Borrows and Tree
   Borrows, this is valid because the raw pointer was derived from an `&mut *manager`
   borrow and no other access occurs through the `Box` while the callback is in flight.
3. **`fb.priv_` stores an index, not a pointer** (line 190: `fb.priv_ = idx as *mut c_void`).
   This is a common C pattern — the value is used as an opaque token and never
   dereferenced.  The `release_buffer` function casts it back to `usize` (line 204).
   This is safe as long as the index remains in bounds, which is guaranteed by the pool
   size.
4. **`do_not_release_frame_buffer`** (line 249): Intentional no-op callback for testing
   buffer exhaustion.  Sound — the C library doesn't require the release callback to
   actually free anything.
5. **No `catch_unwind`:** These callbacks cannot panic (no `.unwrap()` on fallible ops,
   no bounds-unchecked indexing), so the M-1 concern does not apply here.

---

## Tooling Results

### `cargo test` — PASS

39 tests pass, 20 ignored (test-data-dependent), 0 failures.

### `cargo clippy -- -W clippy::undocumented_unsafe_blocks` — 17 WARNINGS

All 17 warnings are for missing `// SAFETY:` comments on `unsafe` blocks in
`decoder.rs`.  No other clippy warnings.  No `#[allow(...)]` suppression of safety
lints found.

### AddressSanitizer (`-Zsanitizer=address`) — NOT RUN

The installed nightly toolchain (1.79.0, 2024-03-20) is too old to build current
dependency versions (`home` crate requires `edition2024`).

**Recommendation:** Update to a recent nightly (`rustup toolchain install nightly`) and
run:
```sh
RUSTFLAGS='-Zsanitizer=address' cargo +nightly test --target x86_64-unknown-linux-gnu
```
This would catch memory errors in the C library triggered by the Rust test suite.
Consider adding this to CI as a periodic check.

---

## Findings Summary Table

| ID | Severity | Category | Status | Notes |
|----|----------|----------|--------|-------|
| C-1 | Critical | Soundness | **RESOLVED** | transmute removed, typed fn pointers |
| C-2 | Critical | Soundness | **RESOLVED** | negative stride + 6 validation checks |
| H-1 | High | Soundness | **RESOLVED** | checked_mul |
| H-2 | High | DoS | **UNRESOLVED** | IVF frame size still uncapped |
| H-3 | High | Leak | **RESOLVED** | Cow replaces .leak() |
| H-4 | High | Lifetime | **PARTIAL** | unsafe + docs, no safe API path |
| M-1 | Medium | UB | **PARTIAL** | documented, not enforced |
| M-2 | Medium | Bounds | **RESOLVED** | index >= 3 guards added |
| M-3 | Medium | Docs | **UNRESOLVED** | no Send/Sync documentation |
| M-4 | Medium | DoS | **PARTIAL** | plane() validates more; indexing still unchecked |
| L-1 | Low | Fragility | **UNRESOLVED** | zeroed() undocumented |
| L-2 | Low | Style | **UNRESOLVED** | constness mismatch |
| L-3 | Low | Portability | **UNRESOLVED** | hardcoded GCC path |
| N-1 | Medium | Bounds | **NEW** | stride check ignores bytes-per-sample |
| N-2 | Low | Docs | **NEW** | 17 unsafe blocks lack SAFETY comments |

**Resolved:** 5 (C-1, C-2, H-1, H-3, M-2)
**Partial:** 3 (H-4, M-1, M-4)
**Unresolved:** 5 (H-2, M-3, L-1, L-2, L-3)
**New:** 2 (N-1, N-2)

---

## Overall Safety Assessment

### Memory Safety

All `unsafe` blocks in the production code (`src/`) are sound under the documented
preconditions.  The two Critical unsoundness findings (C-1 transmute, C-2 negative
stride) are fully resolved.  No path exists that can produce undefined behavior through
the safe public API.

The `set_frame_buffer_functions` API is correctly marked `unsafe` and documents its
safety requirements.  The raw pointer lifetime issue (H-4) is a design limitation, not
a soundness bug — misuse requires calling an `unsafe fn`.

### Denial of Service

Two DoS vectors remain:
1. **H-2:** Unbounded IVF frame allocation (~4 GiB from crafted input)
2. **M-4 / N-1:** Potential panic on corrupted frame metadata that passes `plane()`
   validation but triggers out-of-bounds indexing in `avmdec.rs`

These are relevant for applications processing untrusted input.

### Test Coverage

The always-run test suite (39 tests) provides good coverage of:
- FFI binding layout correctness (20 bindgen tests)
- Decoder initialization and basic lifecycle (4 unit tests)
- External frame buffer management (13 tests covering registration, lifecycle,
  exhaustion, failure modes, and drop ordering)
- Test infrastructure (2 md5 infrastructure tests)

The 20 ignored tests will provide comprehensive correctness coverage once test data is
available.

---

## Recommendations for Future Work (Priority Order)

1. **Cap IVF frame size** (H-2) — Add `max_frame_size` to `IvfReader` or `next_frame()`
2. **Add SAFETY comments** (N-2) — Annotate all 17 unsafe blocks
3. **Fix stride check for HBD** (N-1) — Account for bytes-per-sample in `plane()`
4. **Document thread safety** (M-3) — Add doc comment on `Decoder` struct
5. **Add `catch_unwind` helper** (M-1) — Provide a safe callback wrapper
6. **Wire up `ext_fb_manager`** (H-4) — Provide a safe alternative to raw `priv_`
7. **Update nightly toolchain** — Enable ASAN testing in CI
8. **Add negative-stride unit test** — Construct mock `avm_image_t` to test C-2 fix
9. **Add decode API validation tests** — Exercise error paths with empty/garbage input
10. **Remove hardcoded GCC path** (L-3) — Use `pkg-config` or clang's default search
