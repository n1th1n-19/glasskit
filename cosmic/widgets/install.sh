#!/usr/bin/env bash
# Build glasskit-widgets, install to ~/.local/bin, write one autostart entry per card. No sudo.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="$HOME/.local/bin"
auto="$HOME/.config/autostart"

# GLASSKIT_NO_BUILD=1 (glasskit-cosmic --no-build) reuses an existing build.
if [ "${GLASSKIT_NO_BUILD:-0}" = 1 ] && [ -x "$here/target/release/glasskit-widgets" ]; then
  echo "skipping build (--no-build)"
else
  cargo build --release --manifest-path "$here/Cargo.toml"
fi
mkdir -p "$bin" "$auto"
install -m755 "$here/target/release/glasskit-widgets" "$bin/glasskit-widgets"

for card in clock calendar system media toggles storage focus pet; do
  cat > "$auto/glasskit-widget-$card.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Glasskit widget ($card)
Exec=$bin/glasskit-widgets --card $card
X-GNOME-Autostart-enabled=true
NoDisplay=true
EOF
done

echo "installed. Cards start at next login (or run: glasskit-widgets --card clock)."
echo "NOTE: Ember's own widget (Ember.Widget autostart) overlaps these cards; disable it yourself if unwanted."
