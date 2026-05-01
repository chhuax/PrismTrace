#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Package a PrismTrace macOS alpha release archive.

Usage:
  scripts/package-release.sh [options]

Options:
  --version <version>    Release version. Defaults to workspace.package.version.
  --target <triple>      Target triple. Defaults to the local rustc host triple.
  --dist-dir <dir>       Output directory. Defaults to target/release-kit.
  --binary <path>        Existing prismtrace binary to package.
  --skip-build           Do not run cargo build; requires --binary or target/release/prismtrace.
  -h, --help             Show this help.
USAGE
}

workspace_version() {
  cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json,sys; data=json.load(sys.stdin); print(next(pkg["version"] for pkg in data["packages"] if pkg["name"] == "prismtrace-host"))'
}

host_triple() {
  rustc -vV | awk '/^host:/ { print $2 }'
}

version=""
target=""
dist_dir="target/release-kit"
binary_path=""
skip_build=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      if [[ $# -lt 2 || "$2" == --* ]]; then
        echo "error: missing value after --version" >&2
        exit 2
      fi
      version="$2"
      shift 2
      ;;
    --target)
      if [[ $# -lt 2 || "$2" == --* ]]; then
        echo "error: missing value after --target" >&2
        exit 2
      fi
      target="$2"
      shift 2
      ;;
    --dist-dir)
      if [[ $# -lt 2 || "$2" == --* ]]; then
        echo "error: missing value after --dist-dir" >&2
        exit 2
      fi
      dist_dir="$2"
      shift 2
      ;;
    --binary)
      if [[ $# -lt 2 || "$2" == --* ]]; then
        echo "error: missing value after --binary" >&2
        exit 2
      fi
      binary_path="$2"
      shift 2
      ;;
    --skip-build)
      skip_build=1
      shift
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

version="${version:-$(workspace_version)}"
target="${target:-$(host_triple)}"
archive_name="prismtrace-${version}-${target}"
stage_dir="$dist_dir/$archive_name"

if [[ "$skip_build" -eq 0 ]]; then
  cargo build -p prismtrace-host --bin prismtrace --release --target "$target"
fi

binary_path="${binary_path:-target/$target/release/prismtrace}"

if [[ ! -f "$binary_path" ]]; then
  echo "error: prismtrace binary not found at $binary_path" >&2
  exit 1
fi

rm -rf "$stage_dir"
mkdir -p "$stage_dir/bin"

install -m 0755 "$binary_path" "$stage_dir/bin/prismtrace"
install -m 0755 "scripts/install-prismtrace.sh" "$stage_dir/install.sh"
install -m 0644 "README.md" "$stage_dir/README.md"
install -m 0644 "LICENSE" "$stage_dir/LICENSE"

(
  cd "$stage_dir"
  shasum -a 256 bin/prismtrace install.sh README.md LICENSE > SHA256SUMS
)

tarball="$dist_dir/$archive_name.tar.gz"
rm -f "$tarball" "$tarball.sha256"
tar -czf "$tarball" -C "$dist_dir" "$archive_name"
shasum -a 256 "$tarball" > "$tarball.sha256"

echo "Created $tarball"
echo "Created $tarball.sha256"
