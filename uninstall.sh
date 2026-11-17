#!/bin/sh
# Remove everything install.sh added. Widgets already placed on panels/desktop disappear with their package.
set -u
say() { printf '==> %s\n' "$*"; }
say "Turning Plasma notification popups back on"
[ -x "$HOME/.local/bin/glasskit-island-popups" ] && "$HOME/.local/bin/glasskit-island-popups" on
say "Removing lock-screen override"
rm -f "$HOME/.config/plasma-workspace/env/glasskit-lockscreen.sh"
rm -rf "$HOME/.local/share/plasma/shells/org.kde.plasma.desktop"
say "Removing widgets, splash, icons, glass style, commands"
rm -rf "$HOME"/.local/share/plasma/plasmoids/org.glasskit.*
rm -rf "$HOME/.local/share/plasma/look-and-feel/org.glasskit.splash"
rm -rf "$HOME/.local/share/icons/Glasskit-Filled" "$HOME/.cache/glasskit-filled-icons"
rm -rf "$HOME/.local/share/plasma/desktoptheme/WhiteSur-dark-glass"
for t in Bibata-Modern-Ice Bibata-Modern-Classic Bibata-Original-Classic; do
    [ -L "$HOME/.icons/$t" ] && rm -f "$HOME/.icons/$t"
    rm -rf "$HOME/.local/share/icons/${t:?}"
done
rm -f "$HOME"/.local/bin/glasskit-*
rm -rf "$HOME/.local/share/glasskit"
rm -f "$HOME/.local/share/konsole/Glass.profile" "$HOME/.local/share/konsole/GlassDark.colorscheme"
say "Done. Pick a new Plasma style, icons and splash in System Settings; Nunito font is left installed."
