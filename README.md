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

PrismTrace alpha releases support Apple Silicon and Intel macOS. Homebrew is the recommended install path:

```bash
brew install chhuax/tap/prismtrace
```

Unsigned macOS tarballs are also available from GitHub Releases for Apple Silicon (`aarch64-apple-darwin`) and Intel (`x86_64-apple-darwin`). `.pkg`, `.dmg`, codesigning, and notarization are not available yet.

To install from a tarball, download the latest archive matching your Mac, then run:

```bash
case "$(uname -m)" in
  arm64) target="aarch64-apple-darwin" ;;
  x86_64) target="x86_64-apple-darwin" ;;
  *) echo "unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
esac

tar -xzf prismtrace-*-"$target".tar.gz
cd prismtrace-*-"$target"
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

## Quick Start

PrismTrace is local-first. By default, all observers and the console share one user-level local-machine store, so commands do not need to be started from the same directory.

Start with discovery:

```bash
prismtrace --discover
```

Open the local console:

```bash
prismtrace --console
```

Then visit `http://127.0.0.1:7799`. The console currently shows discovered targets, observer/source health, sessions, timeline events, request details, capabilities, and diagnostics.

For a focused console view:

```bash
prismtrace --console --target codex
prismtrace --console --target opencode
prismtrace --console --target claude
```

## Observing AI Tools

Run one observer command in a terminal while the target AI tool is running. Each observer writes local artifacts into the user-level PrismTrace store, then the console reads those artifacts and projects sessions/events.

Codex Desktop / Codex app-server observer:

```bash
prismtrace --codex-observe
```

If auto-discovery cannot find the Codex socket, pass it explicitly:

```bash
prismtrace --codex-observe --codex-socket /path/to/codex.sock
```

Claude Code transcript observer:

```bash
prismtrace --claude-observe
```

Use a custom transcript root when needed:

```bash
prismtrace --claude-observe --claude-transcript-root "$HOME/.claude/projects"
```

opencode server observer:

```bash
prismtrace --opencode-observe
```

By default PrismTrace reads opencode at `http://127.0.0.1:4096`. If opencode is serving elsewhere:

```bash
prismtrace --opencode-observe --opencode-url http://127.0.0.1:4096
```

Keep the console open in another terminal:

```bash
prismtrace --console
```

## Stored Data

PrismTrace stores local state under:

```text
~/Library/Application Support/PrismTrace/state/
```

Important paths:

- `~/Library/Application Support/PrismTrace/state/artifacts/`: raw observer artifacts and captured payload facts
- `~/Library/Application Support/PrismTrace/state/observability.db`: local state database
- `~/Library/Application Support/PrismTrace/state/index/`: projected session/event/capability read models

To use a custom state location:

```bash
prismtrace --console --state-root /path/to/prismtrace-state
PRISMTRACE_STATE_ROOT=/path/to/prismtrace-state prismtrace --opencode-observe
```

To reset local PrismTrace data:

```bash
rm -rf "$HOME/Library/Application Support/PrismTrace"
```

## Current Alpha Limits

- macOS only.
- Release binaries are unsigned and not notarized.
- Homebrew installs a prebuilt macOS binary selected by CPU architecture.
- Observers are best-effort integrations against fast-moving AI tools.
- `--attach <pid>` remains a bootstrap path for supported Node CLI targets; observer-first flows are preferred for Codex, Claude Code, and opencode.

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
