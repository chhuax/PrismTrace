#!/usr/bin/env bash

set -euo pipefail

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

fake_bin="$tmp_dir/prismtrace"
cat > "$fake_bin" <<'SCRIPT'
#!/usr/bin/env bash
echo "prismtrace smoke"
SCRIPT
chmod +x "$fake_bin"

dist_dir="$tmp_dir/dist"
scripts/package-release.sh \
  --skip-build \
  --binary "$fake_bin" \
  --version "0.0.0-smoke" \
  --target "aarch64-apple-darwin" \
  --dist-dir "$dist_dir"

archive="$dist_dir/prismtrace-0.0.0-smoke-aarch64-apple-darwin.tar.gz"
extract_dir="$tmp_dir/extract"
mkdir -p "$extract_dir"
tar -xzf "$archive" -C "$extract_dir"

package_dir="$extract_dir/prismtrace-0.0.0-smoke-aarch64-apple-darwin"

test -x "$package_dir/bin/prismtrace"
test -x "$package_dir/install.sh"
test -f "$package_dir/SHA256SUMS"
test -f "$package_dir/README.md"
test -f "$package_dir/LICENSE"

(
  cd "$package_dir"
  shasum -a 256 -c SHA256SUMS
)

install_prefix="$tmp_dir/prefix"
"$package_dir/install.sh" --prefix "$install_prefix"
"$install_prefix/bin/prismtrace" | grep -q "prismtrace smoke"

echo "release kit smoke test passed"
