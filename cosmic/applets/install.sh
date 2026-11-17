#!/bin/sh
# Build and install the Glasskit Clock, Status and Space COSMIC applets (user-local, no sudo).
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
for n in Clock Status Space.S Space Space.L Space.XL; do
    install -m644 "$here/../res/io.github.n1th1n_19.Glasskit.$n.desktop" "$HOME/.local/share/applications/"
done
echo "installed: add 'Glasskit Clock' / 'Glasskit Status' / 'Glasskit Space (N px)' via COSMIC Settings > Desktop > Panel > Applets"
