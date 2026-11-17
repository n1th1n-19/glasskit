---
name: glasskit
description: Install, adapt or rebuild the glasskit desktop look (frosted glass panels and popups, a top-bar "island" that shows music, notifications, volume and battery, bold rounded desktop widgets, filled rounded UI icons, a grain boot splash, a big-clock lock screen). Use when the user wants to style or rice a Linux desktop, macOS or Windows; add desktop or panel widgets; make panels or popups glassy, translucent, blurred or glowing; add a dynamic island; change icon style to filled or rounded; customise a boot splash or lock screen; or install glasskit. Works on KDE Plasma 6 directly, and gives build recipes for GNOME, COSMIC (Pop!_OS), Hyprland/wlroots, macOS and Windows.
---

# glasskit

A desktop look built from four ideas: **glass surfaces** (translucent, blurred, rounded, a soft light edge), **one accent** (`#3b82f6` on near-black `#0c0e12`), **one rounded type family** (Nunito) and **filled rounded icons**. The glasskit repo (local: `~/Documents/Marie/glasskit`, run `install.sh`) ships a working KDE Plasma 6 implementation. On other platforms, rebuild the same pieces with that platform's tools.

## First: find out where you are

Run these before you plan anything. They decide the whole approach.

```bash
echo "$XDG_CURRENT_DESKTOP $XDG_SESSION_TYPE"; cat /etc/os-release | head -3
plasmashell --version 2>/dev/null; gnome-shell --version 2>/dev/null; cosmic-panel --version 2>/dev/null; hyprctl version 2>/dev/null | head -1
sw_vers 2>/dev/null; ver 2>/dev/null
```

| Result | Path |
|---|---|
| KDE Plasma 6 | Install the repo (below), then adapt. Read [references/plasma.md](references/plasma.md) before you touch any Plasma config. |
| KDE Plasma 5 | The widgets need porting (Plasma 5 QML imports differ). Say so, and offer an upgrade or a port. |
| GNOME, COSMIC (Pop!_OS), Hyprland/Sway, macOS, Windows | Rebuild with native tools. Follow [references/porting.md](references/porting.md). |

## Plasma 6: install and adapt

1. Clone and install: `./install.sh` (add `--apply --word NAME` to switch the look now and set the splash wordmark). It is user-level, with no sudo.
2. Optional third-party bases: WhiteSur-kde (for `glasskit-glass-style`), Kvantum, a launcher and a dock. These need the user's own install command, because some need sudo or run downloaded scripts. Read any downloaded script before you suggest running it.
3. Place widgets with Plasma scripting over D-Bus (`gdbus call --session --dest org.kde.plasmashell --object-path /PlasmaShell --method org.kde.PlasmaShell.evaluateScript '<js>'`). Read the panel-order rules in references/plasma.md first: the order you write is often lost on restart.
4. Verify every change: read the value back, check `journalctl --user -u plasma-plasmashell` for QML errors, and test the lock screen only with `kscreenlocker_greet --testing`.

## Everywhere: how to work with the user

- **Show a preview before you build visual things.** Render candidates (contact sheets, a small HTML page) and let the user pick. Taste is personal, and a wrong guess wastes their time.
- **Check what changed before you apply.** A Global Theme switch resets colours, icons, Plasma style, decorations, cursor, splash and wallpaper. Keep a re-apply script (`glasskit-theme-apply`).
- **Never leave the session broken.** Back up configs before you edit them. For the lock screen, prefer a path that falls back to the stock UI. Never restart plasmashell more than about four times a minute, because systemd rate-limits it.
- **Respect "no AI art" requests.** For artwork, use real photos with known licences (Wikimedia Commons public domain, Unsplash, Pexels), or hand-authored vector drawing that uses the photo only for proportions. Don't apply filter effects when the user asked for drawing.
- **Be honest about limits.** Private Plasma modules can break on updates. Third-party code isn't yours to relicense.

## Reference files

- [references/plasma.md](references/plasma.md): Plasma 6 internals and gotchas (panels, FrameSvg, icon themes, lock screen, notifications, QML pitfalls)
- [references/porting.md](references/porting.md): the same look on GNOME, COSMIC, Hyprland, macOS and Windows
- [references/design.md](references/design.md): tokens, sizes, motion and the widget catalogue to rebuild
