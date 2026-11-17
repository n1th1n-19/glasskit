#!/bin/sh
# glasskit installer: user-level only, never uses sudo.
# Usage: ./install.sh [--apply] [--lockscreen] [--island-popups] [--word TEXT]
#   --apply           switch colours, Plasma style, icons, splash and fonts to glasskit now
#   --lockscreen      big-clock lock screen (rebuilds a user copy of the desktop shell at each login)
#   --island-popups   let Island show notifications instead of Plasma's popups
#   --word TEXT       wordmark for the boot splash (default: glasskit)
set -eu
cd "$(dirname "$(readlink -f "$0")")"
APPLY=0; LOCK=0; POPUPS=0; WORD=glasskit
while [ $# -gt 0 ]; do
    case "$1" in
        --apply) APPLY=1 ;; --lockscreen) LOCK=1 ;; --island-popups) POPUPS=1 ;;
        --word) shift; WORD=${1:?--word needs text} ;;
        -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac; shift
done
say() { printf '\n==> %s\n' "$*"; }

say "Checking Plasma"
ver=$(plasmashell --version 2>/dev/null | awk '{print $2}')
case "$ver" in 6.*) echo "Plasma $ver" ;; *) echo "glasskit needs KDE Plasma 6 (found: ${ver:-none})" >&2; exit 1 ;; esac
for t in kwriteconfig6 gdbus python3 fc-match; do command -v $t >/dev/null || { echo "missing: $t" >&2; exit 1; }; done
python3 -c "import PIL" 2>/dev/null || echo "note: python3-pillow missing, the splash will be skipped"

say "Widgets"
mkdir -p "$HOME/.local/share/plasma/plasmoids"
for d in plasmoids/*; do rm -rf "$HOME/.local/share/plasma/plasmoids/$(basename "$d")"; cp -r "$d" "$HOME/.local/share/plasma/plasmoids/"; echo "  $(basename "$d")"; done

say "Commands -> ~/.local/bin"
mkdir -p "$HOME/.local/bin"
for f in bin/glasskit-*; do case "$f" in *.actions) continue ;; esac; install -m755 "$f" "$HOME/.local/bin/"; done
mkdir -p "$HOME/.local/share/glasskit/splash"; cp splash/Splash.qml "$HOME/.local/share/glasskit/splash/"

say "Fonts (Nunito, user-level)"
if fc-list | grep -qi 'Nunito'; then echo "  already installed"; else
    mkdir -p "$HOME/.local/share/fonts/nunito"
    for f in 'Nunito%5Bwght%5D.ttf' 'Nunito-Italic%5Bwght%5D.ttf'; do
        curl -fsSL -o "$HOME/.local/share/fonts/nunito/$(printf %s "$f" | sed 's/%5B/[/;s/%5D/]/')" "https://github.com/google/fonts/raw/main/ofl/nunito/$f"
    done; fc-cache -f "$HOME/.local/share/fonts/nunito" >/dev/null; echo "  installed"
fi

say "Filled UI icons (Material Symbols Rounded, Apache-2.0)"
python3 -I "$HOME/.local/bin/glasskit-icons-build"

say "Glass Plasma style"
if [ -d "$HOME/.local/share/plasma/desktoptheme/WhiteSur-dark" ] || [ -d /usr/share/plasma/desktoptheme/WhiteSur-dark ]; then
    "$HOME/.local/bin/glasskit-glass-style"
else echo "  WhiteSur-kde not installed: skipping (see README, then run glasskit-glass-style)"; fi

say "Boot splash"
if python3 -c "import PIL" 2>/dev/null; then python3 -I "$HOME/.local/bin/glasskit-splash-build" --word "$WORD"; fi

say "Konsole glass profile"
mkdir -p "$HOME/.local/share/konsole"; cp konsole/* "$HOME/.local/share/konsole/"

if [ $LOCK = 1 ]; then
    say "Lock screen (big clock)"
    mkdir -p "$HOME/.config/plasma-workspace/env"
    printf '#!/bin/sh\n"$HOME/.local/bin/glasskit-lockscreen-sync"\n' > "$HOME/.config/plasma-workspace/env/glasskit-lockscreen.sh"
    chmod +x "$HOME/.config/plasma-workspace/env/glasskit-lockscreen.sh"
    python3 -I "$HOME/.local/bin/glasskit-lockscreen-sync"
fi
[ $POPUPS = 1 ] && { say "Island notifications"; "$HOME/.local/bin/glasskit-island-popups" off; }
[ $APPLY = 1 ] && { say "Applying the look"; "$HOME/.local/bin/glasskit-theme-apply"; }

say "Done"
cat <<'EOF'
Add widgets: right-click the desktop or a panel > Add Widgets > search "Glasskit".
  Desktop: Clock, Calendar, System, Media, Toggles, Storage & Network, Focus
  Top bar: Island (centre it between two flexible spacers), Battery Pill
Re-apply the look after a Global Theme change: glasskit-theme-apply
Remove everything: ./uninstall.sh
EOF
