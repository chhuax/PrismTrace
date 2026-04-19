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
cargo run -p prismtrace-host -- --readiness
cargo run -p prismtrace-host -- --attach <pid>
```

`--attach <pid>` currently performs a foreground attach session that can capture request and response artifacts for supported running Node CLI targets.
