set -euo pipefail
rustup toolchain install "$TOOLCHAIN" --profile minimal --target "$TARGET" --component rust-src
suffix=
case "$PLATFORM" in
  macos) flags='-Z stack-protector=strong -C link-arg=-Wl,-dead_strip_dylibs' ;;
  windows) flags='-C control-flow-guard -C link-arg=/CETCOMPAT'; suffix=.exe ;;
  glibc) flags='-Z stack-protector=strong' ;;
  musl) flags='-Z stack-protector=strong -C target-feature=+crt-static' ;;
esac
out="target/$TARGET/probe"
mkdir -p "$out"
build() {
  name=$1
  shift
  cargo +"$TOOLCHAIN" build --locked --release --target "$TARGET" --bin caffeinate "$@"
  cp "target/$TARGET/release/caffeinate$suffix" "$out/$name$suffix"
  printf '%s bytes=%s\n' "$name" "$(wc -c < "$out/$name$suffix" | tr -d ' ')"
}
export RUSTFLAGS="$flags"
build baseline
export RUSTFLAGS="$flags -Z location-detail=none"
args=(-Z build-std=std,panic_abort -Z build-std-features=optimize_for_size)
build rebuilt "${args[@]}"
export CARGO_PROFILE_RELEASE_PANIC=immediate-abort
args+=(-Z panic-immediate-abort)
build immediate-z "${args[@]}"
CARGO_PROFILE_RELEASE_OPT_LEVEL=s build immediate-s "${args[@]}"
RUSTFLAGS="$RUSTFLAGS -Z fmt-debug=none" build immediate-no-debug "${args[@]}"
if [[ "$PLATFORM" == macos ]]; then
  system=/usr/bin/caffeinate
  printf 'Apple universal bytes=%s, architectures=%s\n' "$(stat -f '%z' "$system")" "$(lipo -archs "$system")"
  lipo -thin "$APPLE_ARCH" "$system" -output "$out/apple" || cp "$system" "$out/apple"
  printf 'Apple comparison slice bytes=%s\n' "$(stat -f '%z' "$out/apple")"
  for name in baseline rebuilt immediate-z immediate-s immediate-no-debug; do
    printf 'Deployment target for %s\n' "$name"
    otool -l "$out/$name" | awk '/cmd LC_BUILD_VERSION|cmd LC_VERSION_MIN_MACOSX/{n=5} n{print; n--}'
  done
fi
