# M7 FFI Removal and Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retire libavm as a decode backend. By the end of M7, `BackendKind::Rust` is the default and only backend, the crate builds without the libavm C library, the `avmdec` CLI defaults to Rust, and the differential harness is preserved behind a feature flag for future regression triage.

**Architecture:** M7 is a subtraction milestone — no new decode features. The `sys` FFI module and the `avm_image_t` / `avm_codec_frame_buffer_t` types stay, because they define the public output ABI the Rust decoder writes into. The `Libavm` backend enum variant, the `backend/libavm.rs` module, and the `build.rs` logic that compiles libavm all go away. `src/diff.rs` survives behind `#[cfg(feature = "libavm-diff")]` so a future conformance regression can be triaged against libavm from a git checkout.

**Tech Stack:** Rust 2021. Removing the C build removes `bindgen` / `cc` from the default build path.

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M7.

**Prerequisites:** M6 complete (perf within 10% of libavm, conformance 100%, TSan clean).

---

## Phase A — Pre-flight validation

### Task A.1: Final conformance sweep including previously-deferred vectors

- [ ] **Step 1: Re-fetch the entire conformance corpus including any tags skipped in M1–M4** (e.g. palette/IBC if it slipped to M1.5, 4:2:2 / 4:4:4 / 12-bit if they were conditional)

```bash
bash tests/conformance/fetch.sh --all
```

- [ ] **Step 2: Run**

```bash
cargo test -p rustavm --test conformance_test --features threaded-tiles -- --nocapture
```

- [ ] **Step 3: Any remaining failures must be root-caused and fixed before Phase B** — M7 does not begin subtraction while conformance is red
- [ ] **Step 4: Commit** — `rustavm: final conformance sweep before M7 FFI removal`

---

## Phase B — Flip the default backend

### Task B.1: Rust is the default

**Files:**
- Modify: `src/backend.rs`

- [ ] **Step 1: Change `BackendKind` default**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "bin", derive(clap::ValueEnum))]
pub enum BackendKind {
    /// Pure-Rust AV2 decoder.
    #[default]
    Rust,
    /// Legacy backend backed by upstream libavm. Retained only for
    /// differential debugging under the `libavm-diff` feature.
    #[cfg(feature = "libavm-diff")]
    Libavm,
}
```

- [ ] **Step 2: Update `avmdec` CLI default**

The `--backend` flag now defaults to `Rust`. The `Libavm` choice only appears when the crate is built with `--features libavm-diff`.

- [ ] **Step 3: Update `avmdec --help` text**
- [ ] **Step 4: Commit** — `rustavm: default to Rust backend in library and CLI`

### Task B.2: Update the public docs

**Files:**
- Modify: `src/lib.rs` — crate-level docstring
- Modify: `src/backend.rs` — module docstring
- Modify: `src/decoder/mod.rs` — docstring

- [ ] **Step 1: Every crate-level and module-level docstring that describes libavm as "the production backend" must be updated**
- [ ] **Step 2: README.md if present**
- [ ] **Step 3: Commit** — `rustavm: update docs to reflect Rust as the default backend`

---

## Phase C — Delete the libavm backend

### Task C.1: Remove the libavm backend module

**Files:**
- Delete: `src/backend/libavm.rs`
- Modify: `src/backend.rs`

- [ ] **Step 1: Delete `src/backend/libavm.rs`**
- [ ] **Step 2: Remove the `pub(crate) mod libavm;` declaration from `src/backend.rs`**
- [ ] **Step 3: Remove every remaining reference to `LibavmDecoder` in the codebase** — grep first, delete after
- [ ] **Step 4: Build**

```bash
cargo build -p rustavm
cargo build -p rustavm --features libavm-diff
```

The default build compiles clean. The `libavm-diff` feature build *does* still need the libavm backend for the differential harness — see Task D.1.

- [ ] **Step 5: Commit** — `rustavm: remove libavm decoder backend`

### Task C.2: Remove libavm from default build dependencies

**Files:**
- Modify: `build.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Move libavm compilation into a `build.rs` branch gated on `cfg(feature = "libavm-diff")`**
- [ ] **Step 2: Move `bindgen` / `cc` to be optional dependencies enabled only by `libavm-diff`**
- [ ] **Step 3: Default build should now succeed on a machine with no C toolchain**
- [ ] **Step 4: Verify** — on a fresh checkout without `libavm-diff`, `cargo build -p rustavm` must not invoke cc/bindgen
- [ ] **Step 5: Commit** — `rustavm: make libavm C build optional under libavm-diff feature`

### Task C.3: Preserve FFI output ABI types

**Files:**
- Modify: `src/lib.rs`

The `sys` module and the `ffi` re-exports (`avm_image_t`, `avm_codec_frame_buffer_t`, `avm_codec_iter_t`, the decoder ABI version constants) stay — they define the public output ABI. But their contents now come from a hand-written module rather than being bindgen-generated, because there is no libavm C header to bind against in the default build.

- [ ] **Step 1: Convert `sys` from a bindgen-generated include to a hand-written module containing the types we actually use at the ABI boundary**

```rust
// src/sys_abi.rs
#[repr(C)]
pub struct avm_image_t {
    // ... exact fields from the previous bindgen output that external
    // consumers rely on
}
```

- [ ] **Step 2: Under `feature = "libavm-diff"`, the full bindgen output is still available for the diff harness**
- [ ] **Step 3: Verify external users of `rustavm::ffi::*` still compile**
- [ ] **Step 4: Commit** — `rustavm: hand-rolled ABI types for output interop without libavm headers`

---

## Phase D — Differential harness preservation

### Task D.1: Move diff.rs under the feature flag

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/diff.rs`
- Modify: `tests/differential_test.rs`

- [ ] **Step 1: Gate `src/diff.rs` and its tests on `#[cfg(feature = "libavm-diff")]`**

```rust
// src/lib.rs
#[cfg(feature = "libavm-diff")]
pub mod diff;

#[cfg(feature = "libavm-diff")]
pub use diff::{
    compare_ivf_file, compare_ivf_file_outcomes, /* ... */
};
```

- [ ] **Step 2: Same for `tests/differential_test.rs` — put `#![cfg(feature = "libavm-diff")]` at the top**
- [ ] **Step 3: Same for `tests/smoke_test.rs` (added in M1 Task 0.3)** — the Xiph smoke test calls `BackendKind::Libavm` which only exists under the feature flag post-M7. Gate the file with `#![cfg(feature = "libavm-diff")]`.
- [ ] **Step 4: Document in `src/diff.rs` docstring that this is a debugging-only tool**
- [ ] **Step 5: CI: the existing `cargo test -p rustavm --features libavm-diff` job must now also cover `smoke_test.rs` — verify the smoke test still runs under the feature flag build**
- [ ] **Step 6: Commit** — `rustavm: gate differential harness and xiph smoke behind libavm-diff feature`

### Task D.2: Ensure the libavm-diff feature still works end-to-end

- [ ] **Step 1: On a machine with the C toolchain installed, run `cargo test -p rustavm --features libavm-diff`**
- [ ] **Step 2: Both the Rust conformance tests and the differential tests must pass**
- [ ] **Step 3: Commit only if both green**

---

## Phase E — Cleanup

### Task E.1: Purge libavm mentions from comments, docs, and test names

- [ ] **Step 1: `grep -rn "libavm" src/ tests/ docs/` and update every reference**
- [ ] **Step 2: Comments that describe the old "libavm is the production backend" architecture become "Rust is the only backend; libavm is a debugging oracle"**
- [ ] **Step 3: Commit** — `rustavm: purge stale libavm references from comments`

### Task E.2: Remove dead code paths

- [ ] **Step 1: Anything that was `Unimplemented`-guarded on the Rust backend during M0 but has been implemented since**
- [ ] **Step 2: Any `DecoderError::BackendUnavailable` variants or branches that can no longer be reached**
- [ ] **Step 3: Commit** — `rustavm: remove dead unimplemented paths`

### Task E.3: Tighten the public API surface

**Files:**
- Modify: `src/lib.rs`

- [ ] **Step 1: Anything that was exposed as `pub` to support the dual-backend architecture but is no longer needed** — demote to `pub(crate)`
- [ ] **Step 2: Build with `cargo check --all-targets` — external consumers should still compile**
- [ ] **Step 3: Commit** — `rustavm: tighten public API surface after backend unification`

---

## Phase F — Final validation

### Task F.1: Full test sweep

- [ ] **Step 1: Default build**

```bash
cargo build -p rustavm
cargo test -p rustavm
cargo test -p rustavm --test conformance_test -- --nocapture
```

All green.

- [ ] **Step 2: Default build on a C-toolchain-free machine** — verify no `cc` / `bindgen` invocation

- [ ] **Step 3: `libavm-diff` feature build on a C-capable machine**

```bash
cargo build -p rustavm --features libavm-diff
cargo test -p rustavm --features libavm-diff
```

Diff harness passes.

- [ ] **Step 4: `threaded-tiles` feature build**

```bash
cargo test -p rustavm --features threaded-tiles
```

Threaded conformance still 100%.

- [ ] **Step 5: Miri + Clippy**

```bash
cargo +nightly miri test -p rustavm
cargo clippy -p rustavm -- -D warnings
```

### Task F.2: M7 exit checklist

- [ ] `BackendKind::Rust` is the default and only backend in the default build
- [ ] Default `cargo build -p rustavm` succeeds without a C toolchain
- [ ] 100% Main-profile conformance passes
- [ ] Perf still within 10% of libavm under `threaded-tiles`
- [ ] The `libavm-diff` feature still builds and its tests still pass (on a C-capable machine)
- [ ] `avmdec --backend rust <file.ivf>` is the default CLI path
- [ ] `src/backend/libavm.rs` no longer exists
- [ ] `src/diff.rs` is behind `#[cfg(feature = "libavm-diff")]`
- [ ] `src/decoder/` and `src/backend/` have no `TODO`, `FIXME`, `unimplemented!()`, or stale libavm references
- [ ] README / crate docs describe Rust as the production backend

When every box is checked, M7 is done. The rustavm crate is now a pure-Rust AV2 decoder.
