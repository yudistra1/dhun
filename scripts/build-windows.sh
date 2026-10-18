#!/usr/bin/env bash
# Cross-compile Dhun's Windows .exe binaries (x86_64-pc-windows-gnu).
#
# One-time prerequisites:
#   brew install mingw-w64 rustup   # Linux: install mingw-w64 + rustup via your package manager
#   rustup default stable
#   rustup target add x86_64-pc-windows-gnu
#
# The cross-linker is pinned in .cargo/config.toml. Extra args are passed through
# to cargo, e.g. `scripts/build-windows.sh --locked`.
set -euo pipefail

TARGET="x86_64-pc-windows-gnu"

# Homebrew installs rustup keg-only; add it to PATH if present.
if [ -d /opt/homebrew/opt/rustup/bin ]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

# Prefer the rustup-managed toolchain (it owns the cross target); fall back to
# plain cargo if rustup is not installed.
if command -v rustup >/dev/null 2>&1; then
  rustup target add "$TARGET" >/dev/null
  CARGO=(rustup run stable cargo)
else
  CARGO=(cargo)
fi

cd "$(dirname "$0")/.."
"${CARGO[@]}" build --release --target "$TARGET" -p dhun-cli -p dhun-app "$@"

echo
echo "Built Windows binaries:"
ls -lh "target/$TARGET/release/"dhun.exe "target/$TARGET/release/"dhun-app.exe
