#!/usr/bin/env bash
# Build Dhun's macOS binaries as a universal (arm64 + x86_64) binary that runs
# on both Apple Silicon and Intel Macs.
#
# One-time prerequisites (macOS):
#   brew install rustup   # or have the Xcode command-line tools + rustup
#   rustup default stable
# The script adds the two Apple targets itself.
#
# Extra args pass through to cargo, e.g. `scripts/build-mac.sh --locked`.
set -euo pipefail

ARCHS=(aarch64-apple-darwin x86_64-apple-darwin)
BINS=(dhun dhun-app)

# Homebrew installs rustup keg-only; add it to PATH if present.
if [ -d /opt/homebrew/opt/rustup/bin ]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

if command -v rustup >/dev/null 2>&1; then
  for arch in "${ARCHS[@]}"; do
    rustup target add "$arch" >/dev/null
  done
  CARGO=(rustup run stable cargo)
else
  CARGO=(cargo)
fi

cd "$(dirname "$0")/.."

for arch in "${ARCHS[@]}"; do
  "${CARGO[@]}" build --release --target "$arch" -p dhun-cli -p dhun-app "$@"
done

OUT="target/apple-universal/release"
mkdir -p "$OUT"
for bin in "${BINS[@]}"; do
  lipo -create -output "$OUT/$bin" \
    "target/aarch64-apple-darwin/release/$bin" \
    "target/x86_64-apple-darwin/release/$bin"
done

echo
echo "Built universal macOS binaries:"
for bin in "${BINS[@]}"; do
  file "$OUT/$bin"
done
ls -lh "$OUT/dhun" "$OUT/dhun-app"
