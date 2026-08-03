#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

sanitizer="${FRANK_SANITIZER:-}"
case "$sanitizer" in
  address|thread) ;;
  *)
    echo 'FRANK_SANITIZER must be address or thread' >&2
    exit 2
    ;;
esac

command -v rustup >/dev/null 2>&1 || {
  echo 'rustup is required for nightly sanitizer tests' >&2
  exit 2
}
rustup run nightly rustc --version >/dev/null 2>&1 || {
  echo 'the nightly Rust toolchain is required for sanitizer tests' >&2
  exit 2
}

# Keep the sanitizer scope on the pure Rust correctness/security boundary. The
# native Tauri host is exercised by its platform smoke jobs and pulls in GUI
# system libraries that are unrelated to sanitizer coverage.
target="x86_64-unknown-linux-gnu"
packages=(
  frank-safeio
  frank-state
  frank-target
  frank-ledger
  frank-app
)
package_args=()
for package in "${packages[@]}"; do
  package_args+=(--package "$package")
done

flags="-Zsanitizer=$sanitizer"
if [[ "$sanitizer" == address ]]; then
  ASAN_OPTIONS="${ASAN_OPTIONS:-detect_leaks=1:halt_on_error=1}" \
    RUSTFLAGS="$flags" \
    RUSTDOCFLAGS="$flags" \
    cargo +nightly test -Zbuild-std --target "$target" "${package_args[@]}"
else
  TSAN_OPTIONS="${TSAN_OPTIONS:-halt_on_error=1}" \
    RUSTFLAGS="$flags" \
    RUSTDOCFLAGS="$flags" \
    cargo +nightly test -Zbuild-std --target "$target" "${package_args[@]}"
fi

printf 'nightly %s sanitizer suite: passed\n' "$sanitizer"
