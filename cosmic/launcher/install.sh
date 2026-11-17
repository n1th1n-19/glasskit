#!/usr/bin/env bash
# Build glasskit-launcher and copy it to ~/.local/bin. No sudo.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="$HOME/.local/bin"
# GLASSKIT_NO_BUILD=1 (glasskit-cosmic --no-build) reuses an existing build.
if [ "${GLASSKIT_NO_BUILD:-0}" = 1 ] && [ -x "$here/target/release/glasskit-launcher" ]; then
  echo "skipping build (--no-build)"
else
  cargo build --release --manifest-path "$here/Cargo.toml"
fi
mkdir -p "$bin"
install -m755 "$here/target/release/glasskit-launcher" "$bin/glasskit-launcher"
echo "installed $bin/glasskit-launcher (run it again to close it; bind it to Super in COSMIC settings)"
