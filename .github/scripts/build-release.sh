#!/usr/bin/env bash
set -euo pipefail

case "$TARGET" in
  x86_64-pc-windows-msvc) flags='-C control-flow-guard -C link-arg=/CETCOMPAT' ;;
  aarch64-pc-windows-msvc) flags='-C control-flow-guard' ;;
  *-apple-darwin) flags='-Z stack-protector=strong -C link-arg=-Wl,-dead_strip_dylibs' ;;
  *-linux-gnu) flags='-Z stack-protector=strong' ;;
  *-linux-musl) flags='-Z stack-protector=strong -C target-feature=+crt-static' ;;
  *) printf 'Unsupported release target %s\n' "$TARGET" >&2; exit 1 ;;
esac

rustup toolchain install "$RELEASE_TOOLCHAIN" --profile minimal --target "$TARGET" --component rust-src
RUSTFLAGS="$flags -Z location-detail=none" CARGO_PROFILE_RELEASE_PANIC=immediate-abort \
  cargo +"$RELEASE_TOOLCHAIN" build --locked --release --target "$TARGET" --bin caffeinate \
    -Z build-std=std,panic_abort -Z build-std-features=optimize_for_size -Z panic-immediate-abort
