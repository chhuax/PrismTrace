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

if [[ -e "$target_dir" ]]; then
  if [[ ! -w "$target_dir" ]]; then
    echo "error: install destination is not writable: $target_dir" >&2
    echo 'hint: rerun with --prefix "$HOME/.local" or PREFIX="$HOME/.local", or use sudo for a system-wide install.' >&2
    exit 1
  fi
else
  if [[ -d "$prefix" ]]; then
    writable_check_path="$prefix"
  else
    writable_check_path="$(dirname "$prefix")"
  fi

  if [[ ! -w "$writable_check_path" ]]; then
    echo "error: cannot create install destination under $prefix (not writable: $writable_check_path)" >&2
    echo 'hint: rerun with --prefix "$HOME/.local" or PREFIX="$HOME/.local", or use sudo for a system-wide install.' >&2
    exit 1
  fi
fi

mkdir -p "$target_dir"
install -m 0755 "$source_bin" "$target_bin"

echo "Installed PrismTrace to $target_bin"
echo "Run: $target_bin --discover"
