# PrismTrace

Chinese name: `棱镜观测`

[中文说明](./README.zh-CN.md)

PrismTrace is an AI application observability tool focused on capturing real model-facing payloads from running AI applications without forcing a restart.

The first version targets macOS and focuses on Node or Electron based AI CLI and desktop applications. The immediate goal is simple:

- attach to a running target
- capture post-attach LLM requests and responses
- inspect payloads, tools, and metadata in a local observability console

## Status

This repository is in the design and bootstrap stage.

The current design spec lives in [docs/总体设计与V1方案.md](./docs/总体设计与V1方案.md).
The current product roadmap lives in [docs/产品迭代路线图.md](./docs/产品迭代路线图.md).

## V1 Direction

PrismTrace V1 is scoped to:

- macOS only
- already-running Node and Electron AI apps
- no restart requirement
- payload visibility first
- local-first storage and privacy

## Alpha Install

PrismTrace alpha releases are distributed as unsigned macOS tarballs. The first release kit targets Apple Silicon macOS (`aarch64-apple-darwin`). Homebrew, `.pkg`, `.dmg`, codesigning, and notarization are not available yet.

Download the latest `prismtrace-*-aarch64-apple-darwin.tar.gz` archive from GitHub Releases, then install:

```bash
tar -xzf prismtrace-*-aarch64-apple-darwin.tar.gz
cd prismtrace-*-aarch64-apple-darwin
./install.sh --prefix "$HOME/.local"
```

Make sure the install prefix is on your `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Smoke test the installed command:

```bash
prismtrace --discover
```

Start the local console:

```bash
prismtrace --console
```

Observer entrypoints are also exposed through the installed command:

```bash
prismtrace --codex-observe
prismtrace --claude-observe
prismtrace --opencode-observe
```

## Longer-Term Vision

PrismTrace is not only a payload capture utility. The broader direction is AI observability:

- information collection
- session reconstruction
- analysis and explanation

The first step is to build a trustworthy local fact layer for later analysis.

## Workspace Layout

- `crates/prismtrace-core`: shared runtime and process domain types
- `crates/prismtrace-storage`: local state directory layout and storage bootstrap
- `crates/prismtrace-host`: runnable host binary and startup bootstrap
- `docs/`: design and implementation planning documents

## Local Development

```bash
cargo test
cargo run -p prismtrace-host
cargo run -p prismtrace-host -- --discover
cargo run -p prismtrace-host --bin prismtrace -- --discover
cargo run -p prismtrace-host -- --readiness
cargo run -p prismtrace-host -- --attach <pid>
```

`--attach <pid>` currently performs a foreground attach session that can capture request and response artifacts for supported running Node CLI targets.

## Local Console

Start the local observability console with:

```bash
cargo run -p prismtrace-host -- --console
cargo run -p prismtrace-host --bin prismtrace -- --console
```

Limit the console to specific target identities with repeatable `--target` flags:

```bash
cargo run -p prismtrace-host -- --console --target opencode
cargo run -p prismtrace-host -- --console --target opencode --target codex
```

When `--target` is present, the homepage and `/api/*` payloads stay in the same filtered view. If nothing matches, the console still opens and shows an explicit filtered empty state instead of falling back to the global process list.

By default, PrismTrace serves the console at `http://127.0.0.1:7799`.

The current bootstrap console provides:

- target summaries
- recent activity timeline
- request summary list
- basic request detail and observability health panels
