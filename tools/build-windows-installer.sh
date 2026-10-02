#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DESKTOP="$ROOT/apps/desktop"
TARGET="x86_64-pc-windows-msvc"

export PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH"

python3 "$ROOT/tools/verify-windows-package.py"

# Tauri expects each external binary to carry its target triple while
# bundling; it removes the triple in the installed application directory.
mkdir -p "$DESKTOP/src-tauri/binaries"
cargo xwin build --release --target "$TARGET" -p deskpet-assetpipe
cp "$ROOT/target/$TARGET/release/deskpet-assetpipe.exe" \
  "$DESKTOP/src-tauri/binaries/deskpet-assetpipe-$TARGET.exe"

cd "$DESKTOP"
npm run tauri build -- \
  --runner cargo-xwin \
  --target "$TARGET" \
  --bundles nsis

echo "Windows installer:"
find "$ROOT/target/$TARGET/release/bundle/nsis" -maxdepth 1 -name '*-setup.exe' -print
