# Security Audit: rustavm — Rust AV2 Decoder Wrapper

**Date:** 2026-04-07
**Scope:** `rustavm/src/decoder.rs`, `rustavm/src/lib.rs`, `rustavm/src/ivf.rs`, `rustavm/src/bin/avmdec.rs`, `rustavm/build.rs`
**C headers reviewed:** `avm/avm_decoder.h`, `avm/avm_codec.h`, `avm/avm_frame_buffer.h`, `avm/avm_image.h`

---

## Summary

The `rustavm` crate provides a safe Rust wrapper around the C-based AV2 video decoder library (`libavm`). The wrapper uses `bindgen` to generate FFI bindings and exposes a higher-level `Decoder` API. This audit identified **2 Critical**, **4 High**, **4 Medium**, and **3 Low** severity findings.

---

## Critical

### C-1: Unsound `transmute` of function pointers in `set_frame_buffer_functions`

**File:** `src/decoder.rs:146-147`

```rust
Some(std::mem::transmute(get_fb)),
Some(std::mem::transmute(release_fb)),
```

The code `transmute`s user-provided function pointers to match the bindgen-generated callback types. A code comment on line 139-142 explicitly acknowledges the Rust signature "was slightly wrong." If the transmuted function pointer has a different calling convention, parameter count, parameter types, or return type than the C callback typedef (`avm_get_frame_buffer_cb_fn_t` / `avm_release_frame_buffer_cb_fn_t`), calling the resulting pointer is **undefined behavior**.

The C signatures (from `avm_frame_buffer.h:68-69,81-82`) are:
```c
typedef int (*avm_get_frame_buffer_cb_fn_t)(void *priv, size_t min_size, avm_codec_frame_buffer_t *fb);
typedef int (*avm_release_frame_buffer_cb_fn_t)(void *priv, avm_codec_frame_buffer_t *fb);
```

The Rust parameter declarations on lines 127-135 match this signature, so the `transmute` is *currently* a no-op on most platforms — but it silently suppresses the compiler's type-checking. Any future drift between the declared parameter types and the bindgen output will compile without error and produce UB at runtime.

**Recommendation:** Remove the `transmute` calls. Use the bindgen-generated callback type aliases directly in the function signature, or write a trivial shim that converts between the types with an explicit, auditable cast. At minimum, add a `static_assert`-style check (e.g., `const _: () = assert!(size_of::<get_fb_type>() == size_of::<bindgen_type>());`).

---

### C-2: `slice::from_raw_parts` with negative stride produces unsound slice

**File:** `src/decoder.rs:244-245`

```rust
let stride = (*self.img).stride[index] as usize;
let height = self.height_for_plane(index);
Some(slice::from_raw_parts(plane_ptr, stride * height))
```

The C struct `avm_image_t` declares `stride[3]` as `int` (signed). A negative stride is a valid representation in the C image API — it indicates a vertically-flipped image (see `avm_img_flip()` in `avm_image.h:507`). Casting a negative `int` (e.g., `-1920`) to `usize` produces an astronomically large value (e.g., `18446744073709549696` on 64-bit). Multiplying by `height` will either:
1. Wrap around (in release builds), producing a garbage length, or
2. Produce a value vastly exceeding the actual allocation.

In either case, `slice::from_raw_parts` constructs a slice whose length exceeds the backing allocation. Any read through this slice is **undefined behavior** — and the slice is returned as a safe `&[u8]`, so callers have no indication that it is unsound.

**Recommendation:** Check `stride >= 0` before the cast. If stride is negative, either return `None` (rejecting flipped images) or compute the correct base pointer and positive stride to construct a valid slice over the flipped buffer. Document the decision.

---

## High

### H-1: `stride * height` multiplication can overflow without check

**File:** `src/decoder.rs:245`

Even when stride is non-negative, the multiplication `stride * height` is performed as `usize` arithmetic. On a 32-bit target (where `usize` is 32 bits), a stride of `65536` and height of `65536` overflows to `0`. On 64-bit this is less likely but still possible with adversarial input. The resulting slice length is wrong, violating the `slice::from_raw_parts` safety contract.

**Recommendation:** Use `stride.checked_mul(height)` and return `None` on overflow.

---

### H-2: IVF frame size denial-of-service vector

**File:** `src/ivf.rs:53,59`

```rust
let size = u32::from_le_bytes(size_buf) as usize;
let mut data = vec![0u8; size];
```

A crafted IVF file can declare a frame size up to `u32::MAX` (≈ 4 GiB). The code unconditionally allocates that much memory. This is a trivial denial-of-service for any application that opens untrusted IVF files.

**Recommendation:** Add a configurable maximum frame size (e.g., 256 MiB) and reject frames exceeding it. At minimum, document the risk.

---

### H-3: Memory leak via `.leak()` in `avmdec.rs`

**File:** `src/bin/avmdec.rs:83`

```rust
.replace("{}", &img.bit_depth().to_string()).leak()
```

`String::leak()` deliberately leaks the `String`'s heap allocation to obtain a `&'static str`. This is called once per execution (inside the `frame_count == 0` branch), so it leaks a small amount per run. While the practical impact is minor for a CLI tool, this pattern is unsound as a library pattern and signals a type-system workaround rather than a correct solution.

**Recommendation:** Store the `String` in a local variable and use `&string[..]` for the remainder of the scope. The `colorspace` variable should be a `String` (or `Cow<'static, str>`) rather than `&str`.

---

### H-4: Raw `*mut c_void` in `set_frame_buffer_functions` has no lifetime safety

**File:** `src/decoder.rs:136,148`

```rust
priv_: *mut c_void,
```

The `priv_` pointer is passed directly to the C library, which stores it and later passes it back to the `get_fb` / `release_fb` callbacks. There is no Rust-side lifetime tracking to ensure the pointed-to data remains valid for the lifetime of the decoder. If the caller frees the data behind `priv_` while the decoder still holds it, any subsequent callback invocation dereferences a dangling pointer.

**Recommendation:** Wrap `priv_` in a mechanism that ties its lifetime to the `Decoder`. For example, accept a `Box<dyn FrameBufferManager>` (the struct already has an `ext_fb_manager: Option<Box<dyn FrameBufferManager>>` field that is never used), store it, and pass a pointer to the boxed value as `priv_`. This ensures the data lives as long as the decoder.

---

## Medium

### M-1: Panic unwinding through FFI boundaries is undefined behavior

**File:** `src/decoder.rs:126-157` (frame buffer callback path)

If a user-provided frame buffer callback (registered via `set_frame_buffer_functions`) panics, the panic will attempt to unwind through C stack frames. This is **undefined behavior** in Rust — see [the Rustonomicon on FFI and panics](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-panics).

While the current crate doesn't provide a safe callback wrapper, any future use of the `ext_fb_manager` field with Rust closures would be vulnerable.

**Recommendation:** Wrap all callback bodies in `std::panic::catch_unwind()` and return an error code to C on panic. Alternatively, ensure callbacks are `extern "C-unwind"` (requires nightly or Rust ≥ 1.71 with stabilized `c_unwind`).

---

### M-2: `plane_width()` and `chroma_plane_height()` have no bounds check on `index`

**File:** `src/decoder.rs:257-275`

```rust
pub fn plane_width(&self, index: usize) -> usize {
    let w = unsafe { (*self.img).d_w as usize };
    if index == 0 { w } else { /* chroma calc */ }
}
```

Unlike `plane()` (which checks `index >= 3`), `plane_width()` and `chroma_plane_height()` accept any `index`. For `index >= 3`, they return a chroma-shifted value that has no semantic meaning and could mislead callers into computing incorrect buffer sizes — which then feed into the `plane()` slice construction in `avmdec.rs`.

**Recommendation:** Add `if index >= 3 { return 0; }` guards consistent with `plane()` and `stride()`.

---

### M-3: Thread safety is not documented; `Decoder` is not `Send`

**File:** `src/decoder.rs:35-38`

```rust
pub struct Decoder {
    ctx: avm_codec_ctx_t,
    ext_fb_manager: Option<Box<dyn FrameBufferManager>>,
}
```

`avm_codec_ctx_t` contains raw pointers (`*mut avm_codec_priv_t`, `*const char`, etc.), so Rust's auto-trait rules correctly prevent `Decoder` from implementing `Send` or `Sync`. This is safe-by-default but **undocumented**. Users who need to move a `Decoder` across threads will be blocked by the compiler but given no guidance.

Additionally, the C library header (`avm_decoder.h:113`) explicitly states: *"If the library was configured with cmake -DCONFIG_MULTITHREAD=0, this call is not thread safe and should be guarded with a lock."* The Rust wrapper does not enforce or document this.

**Recommendation:** Add explicit documentation on the `Decoder` type explaining thread safety constraints. If the decoder is safe to send between threads (i.e., the C library was built with multithread support), consider an `unsafe impl Send for Decoder {}` with a documented safety contract.

---

### M-4: `avmdec.rs` plane data access can panic on corrupted frames

**File:** `src/bin/avmdec.rs:128-129,143`

```rust
row_buf[x] = plane[start + x * 2];  // line 129
let row_data = &plane[start..end];   // line 143
```

These accesses index into the slice returned by `plane()`. If `stride`, `plane_width`, or `height_for_plane` report values inconsistent with the actual plane allocation (which can happen with corrupted bitstreams or the negative-stride issue from C-2), these will panic with an out-of-bounds index.

While panicking is safe (no UB), in a production decoder it can be a denial-of-service vector — a single corrupted frame crashes the entire process.

**Recommendation:** Use `.get()` or `.get(..)` with graceful error handling instead of panicking indexing, or validate that `stride * h <= plane.len()` before entering the loop.

---

## Low

### L-1: `std::mem::zeroed()` for C structs is fragile

**File:** `src/decoder.rs:53-54`

```rust
let mut ctx: avm_codec_ctx_t = std::mem::zeroed();
let mut cfg: avm_codec_dec_cfg_t = std::mem::zeroed();
```

`std::mem::zeroed()` is valid here because both C structs are zero-initializable (pointers become null, integers become 0). However, if the C struct ever adds a field with a non-zero required initial value, or if bindgen maps an enum variant to a non-zero default, this becomes instant UB with no compiler warning.

**Recommendation:** Consider using `MaybeUninit` and initializing fields explicitly, or add a comment documenting the zero-initialization contract.

---

### L-2: `avm_codec_get_frame` return constness mismatch

**File:** `src/decoder.rs:170,174`

The C API declares:
```c
avm_image_t *avm_codec_get_frame(avm_codec_ctx_t *ctx, avm_codec_iter_t *iter);
```

Bindgen generates the return type as `*mut avm_image_t`, but the Rust wrapper stores it as `*const avm_image_t` (line 201). This is sound because the wrapper only reads through the pointer, but the mismatch requires an implicit cast that could mask future mutations that the C library expects.

**Recommendation:** Store as `*mut avm_image_t` internally and expose read-only access through the public API.

---

### L-3: `build.rs` hardcodes GCC include path

**File:** `build.rs:21`

```rust
.clang_arg("-I/usr/lib/gcc/x86_64-linux-gnu/13/include")
```

This hardcoded path ties the build to a specific GCC version and architecture. While not a security vulnerability per se, it means builds on other systems will silently miss system headers, potentially causing bindgen to generate incorrect bindings that mismatch the compiled C library.

**Recommendation:** Use `cc::Build` or `pkg-config` to discover the correct include paths, or remove the hardcoded path if clang can find the headers through its own search paths.

---

## Inventory of `unsafe` Blocks

| File | Lines | Purpose | Soundness |
|------|-------|---------|-----------|
| `decoder.rs` | 52-76 | `Decoder::with_config` — zeroes ctx/cfg, calls `avm_codec_dec_init_ver` | OK (see L-1) |
| `decoder.rs` | 80-93 | `Decoder::decode` — calls `avm_codec_decode` with slice ptr/len | OK |
| `decoder.rs` | 104-119 | `Decoder::get_stream_info` — calls `avm_codec_get_stream_info` | OK |
| `decoder.rs` | 138-155 | `Decoder::set_frame_buffer_functions` — transmute + FFI call | **Unsound (C-1)** |
| `decoder.rs` | 169-180 | `FrameIterator::next` — calls `avm_codec_get_frame`, null-check | OK |
| `decoder.rs` | 185-188 | `Decoder::drop` — calls `avm_codec_destroy` | OK |
| `decoder.rs` | 207-219 | `Frame` accessors (`width`, `height`, `bit_depth`, `format`, `monochrome`, `csp`, `range`) — dereference `self.img` | OK (pointer guaranteed non-null by construction) |
| `decoder.rs` | 238-246 | `Frame::plane` — `slice::from_raw_parts` from C image buffer | **Unsound (C-2, H-1)** |
| `decoder.rs` | 252-254 | `Frame::stride` — reads `(*self.img).stride[index]` | OK (bounds-checked) |
| `decoder.rs` | 258-264 | `Frame::plane_width` — reads `d_w`, `x_chroma_shift` | Missing bounds (M-2) |
| `decoder.rs` | 268-274 | `Frame::chroma_plane_height` — reads `d_h`, `y_chroma_shift` | Missing bounds (M-2) |
| `lib.rs` | 17 | `CStr::from_ptr(avm_codec_version_str())` | OK (C function guarantees null-terminated string) |
| `lib.rs` | 24-36 | Test: init/destroy codec context | OK |

---

## Lifetime Soundness Analysis

**Can a `Frame` outlive its `Decoder`?**

No — the lifetime design is **sound**. `FrameIterator<'a>` holds `&'a mut Decoder`, and `Frame<'a>` carries the same lifetime `'a` via `PhantomData<&'a Decoder>`. As long as any `Frame<'a>` exists, the mutable borrow on the `Decoder` is active, preventing both destruction and further calls to `decode()` (which would invalidate the frame pointers per the C API contract on `avm_decoder.h:204-206`).

---

## Resource Leak Analysis

**Does `Drop` always run?**

- `Decoder::drop` calls `avm_codec_destroy`. This runs in normal control flow and during stack unwinding (panic).
- If `Decoder::with_config` fails, no `Decoder` is constructed, so `Drop` is not needed — the C library was either not initialized or returned an error before allocating resources.
- **Risk:** If a panic occurs inside a C callback (M-1), unwinding is UB, so `Drop` may never execute. This is a theoretical resource leak subordinate to the UB itself.

---

## Recommendations Summary (by priority)

1. **C-1:** Remove `transmute` in `set_frame_buffer_functions`; use bindgen types directly.
2. **C-2:** Validate `stride >= 0` before casting; use `checked_mul` for `stride * height`.
3. **H-1:** Guard `slice::from_raw_parts` length with `checked_mul`.
4. **H-2:** Cap IVF frame allocation size.
5. **H-3:** Replace `.leak()` with a properly-scoped `String`.
6. **H-4:** Tie `priv_` lifetime to the `Decoder` via `ext_fb_manager`.
7. **M-1:** Wrap FFI callbacks with `catch_unwind`.
8. **M-2:** Add bounds checks to `plane_width()` and `chroma_plane_height()`.
9. **M-3:** Document `Send`/`Sync` status of `Decoder`.
10. **M-4:** Use checked indexing in `avmdec.rs` plane loops.
11. **L-1:** Document zero-initialization contract.
12. **L-2:** Store `*mut avm_image_t` internally.
13. **L-3:** Remove hardcoded GCC include path.
