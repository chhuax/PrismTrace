# PrismTrace Workspace Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first Rust workspace skeleton for PrismTrace so the repo has a compilable host binary, shared domain types, and local storage bootstrap code.

**Architecture:** Start with a small Rust workspace split into focused crates: a shared core crate for domain types, a storage crate for local state layout, and a host crate for bootstrap logic and the first runnable binary. Keep dependencies to the standard library for now so the scaffold can compile and test offline.

**Tech Stack:** Rust 1.94 workspace, standard library, cargo test

---

### Task 1: Create the workspace root

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`

- [ ] Define a workspace with three members: `crates/prismtrace-core`, `crates/prismtrace-storage`, and `crates/prismtrace-host`.
- [ ] Add a `.gitignore` that ignores `target/` and local state directories created by the host binary.

### Task 2: Add the shared core crate

**Files:**
- Create: `crates/prismtrace-core/Cargo.toml`
- Create: `crates/prismtrace-core/src/lib.rs`

- [ ] Write a failing unit test for the first core behavior, such as stable runtime labels or human-readable process target names.
- [ ] Add minimal shared domain types for runtime kind, process target, and probe health.
- [ ] Run `cargo test -p prismtrace-core` and verify the test passes.

### Task 3: Add the storage crate

**Files:**
- Create: `crates/prismtrace-storage/Cargo.toml`
- Create: `crates/prismtrace-storage/src/lib.rs`

- [ ] Write a failing unit test for storage layout generation and state directory creation.
- [ ] Add a small storage layout type that computes `state/observability.db`, `state/artifacts`, `state/tmp`, and `state/logs`.
- [ ] Add an initialization function that creates the directory tree.
- [ ] Run `cargo test -p prismtrace-storage` and verify the tests pass.

### Task 4: Add the host crate and runnable binary

**Files:**
- Create: `crates/prismtrace-host/Cargo.toml`
- Create: `crates/prismtrace-host/src/lib.rs`
- Create: `crates/prismtrace-host/src/main.rs`

- [ ] Write a failing unit test for host bootstrap behavior and startup summary formatting.
- [ ] Add a minimal app config, bootstrap function, and startup summary.
- [ ] Add a runnable binary that initializes local state and prints a startup summary.
- [ ] Run `cargo test -p prismtrace-host` and verify the tests pass.

### Task 5: Run the full workspace verification

**Files:**
- Modify: `README.md`
- Modify: `README.zh-CN.md`

- [ ] Update the READMEs with the new workspace skeleton and crate layout.
- [ ] Run `cargo test` from the workspace root.
- [ ] Confirm the entire skeleton compiles and tests cleanly.
