#!/bin/sh
# Build and install the Glasskit Clock + Status COSMIC applets (user-local, no sudo).
set -e
here=$(cd "$(dirname "$0")" && pwd)
# GLASSKIT_NO_BUILD=1 (glasskit-cosmic --no-build) reuses an existing build.
if [ "${GLASSKIT_NO_BUILD:-0}" = 1 ] && [ -x "$here/target/release/glasskit-applets" ]; then
    echo "skipping build (--no-build)"
else
    cargo build --release --manifest-path "$here/Cargo.toml"
fi
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m755 "$here/target/release/glasskit-applets" "$HOME/.local/bin/glasskit-applets"
install -m644 "$here"/../res/io.github.n1th1n_19.Glasskit.Clock.desktop "$here"/../res/io.github.n1th1n_19.Glasskit.Status.desktop "$HOME/.local/share/applications/"
echo "installed: add 'Glasskit Clock' / 'Glasskit Status' via COSMIC Settings > Desktop > Panel > Applets"
