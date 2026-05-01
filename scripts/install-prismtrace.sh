#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Install PrismTrace.

Usage:
  install.sh [--prefix <dir>]

Environment:
  PREFIX  Installation prefix. Defaults to /usr/local.

Examples:
  ./install.sh
  ./install.sh --prefix "$HOME/.local"
  PREFIX="$HOME/.local" ./install.sh
USAGE
}

prefix="${PREFIX:-/usr/local}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      if [[ $# -lt 2 || "$2" == --* ]]; then
        echo "error: missing value after --prefix" >&2
        exit 2
      fi
      prefix="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source_bin="$script_dir/bin/prismtrace"
target_dir="$prefix/bin"
target_bin="$target_dir/prismtrace"

if [[ ! -f "$source_bin" ]]; then
  echo "error: expected PrismTrace binary at $source_bin" >&2
  exit 1
fi

mkdir -p "$target_dir"
install -m 0755 "$source_bin" "$target_bin"

echo "Installed PrismTrace to $target_bin"
echo "Run: $target_bin --discover"
