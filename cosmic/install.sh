#!/bin/sh
# Build and install the Glasskit Island COSMIC applet (user-local, no sudo).
set -e
here=$(cd "$(dirname "$0")" && pwd)
# GLASSKIT_NO_BUILD=1 (glasskit-cosmic --no-build) reuses an existing build.
if [ "${GLASSKIT_NO_BUILD:-0}" = 1 ] && [ -x "$here/island/target/release/glasskit-island" ]; then
    echo "skipping build (--no-build)"
else
    cargo build --release --manifest-path "$here/island/Cargo.toml"
fi
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m755 "$here/island/target/release/glasskit-island" "$HOME/.local/bin/glasskit-island"
install -m644 "$here/res/io.github.n1th1n_19.Glasskit.Island.desktop" "$HOME/.local/share/applications/"
# Settings template: never overwrite an existing island.conf. A legacy one-word
# ~/.config/glasskit/pet file keeps winning, so carry its value into the new file.
conf="${XDG_CONFIG_HOME:-$HOME/.config}/glasskit"
if [ ! -e "$conf/island.conf" ]; then
    mkdir -p "$conf"
    cp "$here/island/island.conf.example" "$conf/island.conf"
    if [ -f "$conf/pet" ]; then
        word=$(head -n1 "$conf/pet" | tr '[:upper:]' '[:lower:]' | sed 's/^glasskit_pet=//' | tr -cd 'a-z0-9')
        [ -n "$word" ] && sed -i "s/^pet = .*/pet = $word/" "$conf/island.conf"
    fi
    echo "created $conf/island.conf (edit it to change the pet and its behaviour)"
fi
echo "installed: add 'Glasskit Island' via COSMIC Settings > Desktop > Panel > Applets"
