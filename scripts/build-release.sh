#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

version="$(cargo pkgid | sed 's/.*#//')"
dist="$ROOT/dist"
rm -rf "$dist"
mkdir -p "$dist"

build_target() {
  local target="$1"
  local cmd=(cargo build --release --locked --target "$target")
  case "$target" in
    *-unknown-linux-musl) cmd=(cargo zigbuild --release --locked --target "$target") ;;
  esac
  "${cmd[@]}"
  cp "target/$target/release/wezcmd" "$dist/wezcmd-$target"
}

build_target aarch64-unknown-linux-musl
build_target x86_64-unknown-linux-musl

case "$(uname -s)" in
  Darwin)
    rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
    build_target aarch64-apple-darwin
    build_target x86_64-apple-darwin
    ;;
  *)
    echo "Skipping Darwin targets on $(uname -s); GitHub Actions builds them on macOS." >&2
    ;;
esac

(
  cd "$dist"
  shasum -a 256 wezcmd-* > checksums.txt
)

echo "Built wezcmd $version assets in $dist"
