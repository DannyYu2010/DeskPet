#!/usr/bin/env bash
#
# Run the dev build on real macOS and capture everything an agent needs.
#
# The agent working on this repo reaches the filesystem but cannot execute on
# macOS, so the loop is: you run this, it writes logs/latest.log, the agent
# reads that file directly. No copy-pasting terminal output.
#
#   ./tools/run-macos-dev.sh
#
# Leave it running. Ctrl-C when you are done looking at the pet.

set -u

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$REPO/logs"

# A non-interactive shell does not read your login profile, so a rustup install
# is invisible unless we go looking for it. Check the usual homes before
# declaring cargo missing.
if ! command -v cargo >/dev/null 2>&1; then
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
fi
for d in "$HOME/.cargo/bin" /opt/homebrew/bin /usr/local/bin; do
  case ":$PATH:" in *":$d:"*) ;; *) [ -d "$d" ] && PATH="$PATH:$d" ;; esac
done
export PATH

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust, then re-run this script:"
  echo
  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
  echo "  source \"$HOME/.cargo/env\""
  echo
  exit 127
fi
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG="$REPO/logs/dev-$STAMP.log"

{
  echo "=== environment ==="
  sw_vers 2>&1
  echo "arch: $(uname -m)"
  echo "cargo: $(cargo --version 2>&1 || echo MISSING)"
  echo "rustc: $(rustc --version 2>&1 || echo MISSING)"
  echo "node:  $(node --version 2>&1 || echo MISSING)"
  echo "xcode-select: $(xcode-select -p 2>&1 || echo MISSING)"
  echo
  echo "=== npm run tauri dev ==="
} >"$LOG" 2>&1

cd "$REPO/apps/desktop" || exit 1
npm run tauri dev 2>&1 | tee -a "$LOG"
STATUS="${PIPESTATUS[0]}"

echo "=== exited with status $STATUS ===" | tee -a "$LOG"
rm -f "$REPO/logs/latest.log"
cp "$LOG" "$REPO/logs/latest.log"
echo
echo "Full log: $LOG"
echo "Agent reads: logs/latest.log"
