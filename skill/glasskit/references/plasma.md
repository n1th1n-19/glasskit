# KDE Plasma 6: internals and gotchas

Learned by building glasskit on Plasma 6.8 (Fedora 45, Wayland). Each item cost a debugging round.

## Panels and widgets

- **Scripting**: `org.kde.PlasmaShell.evaluateScript` runs JS with `panels()`, `desktops()`, `p.addWidget(id)`, `w.currentConfigGroup=[...]`, `w.writeConfig(k,v)`, `p.height`, `p.floating`, `p.opacity` (`"opaque"|"translucent"|"adaptive"`), `p.hiding` (`"none"|"autohide"|"dodgewindows"`), `p.lengthMode` (`"fill"|"fit"`). gdbus mangles `\n` in the script, so avoid escapes.
- **Widget order** lives in the panel's `[General] AppletOrder` (`;`-separated applet ids). After you add or remove widgets, plasmashell rewrites the order on exit and **your write is lost**. Reliable sequence: make the structural change, restart plasmashell, then write `AppletOrder` again and restart once more.
- **Restart limit**: `systemctl --user restart plasma-plasmashell.service` more than ~5 times in 10 s triggers systemd's start limit, and the desktop disappears. Recover with `systemctl --user reset-failed plasma-plasmashell.service && systemctl --user start plasma-plasmashell.service`. Wait about 20 s between restarts.
- **Desktop widget positions** are in `plasma-org.kde.plasma.desktop-appletsrc`, under the desktop containment's group, as `ItemGeometries-<W>x<H>` and `ItemGeometriesHorizontal`: `Applet-<id>:x,y,w,h,0;`. Positions use a 16 px grid, sizes include an 8 px margin (360 wide → 368), and y is offset by the top panel. The script `w.geometry` setter is ignored. To move widgets, stop plasmashell, edit both keys, then start it.
- **Panel thickness vs content**: the Plasma style's `widgets/panel-background` `hint-top-margin` / `hint-bottom-margin` eat the panel height. 8 px of padding on a 28 px bar leaves widgets about 12 px. Use 2–3 px so widgets scale with the bar.
- A widget that should fill the bar needs `Layout.fillHeight: true` on its compact representation. Size everything from `height`, never from fixed px.
- **Opaque panels make popups opaque**: an `opaque` panel's popups use `solid/dialogs/background`. For glass popups set the panel to `translucent`, and make the style's `widgets/panel-background` fill nearly opaque if the bar should look solid.

## Plasma style SVGs (FrameSvg)

- Frames are 9-slice: `topleft top topright left center right bottomleft bottom bottomright`, with optional prefixes (`north-`, `south-`, `mask-`, `shadow-`) and margin rects (`hint-*-margin`, `hint-*-inset`).
- `mask-*` pieces define the **blur region**. Make them the same rounded shape, or the blur shows square corners.
- `shadow-*` pieces are drawn outside the frame. Draw them in a **light** colour to get a glow. Size the reach with `shadow-hint-*-margin`.
- The renderer is **SVG Tiny 1.2**: no nested `<svg>` (silently skipped: the panel turns invisible), no filters (no blur or glow filters), unreliable clip paths. Draw each piece in place with plain paths. Fake soft effects with stacked 1 px strokes of falling opacity.
- Check `journalctl` for "Skipping a nested svg element". Clear caches after edits: `~/.cache/plasma-svgelements*`, `~/.cache/plasma_theme_*`, `~/.cache/ksvg-elements*`.
- A Plasma style's own `icons/` folder overrides the icon theme for tray icons. Rename it to let the icon theme win.
- Quarter-circle corners: check the SVG arc sweep flag by rendering a preview (`magick -density 96 frame.svg out.png`). A wrong flag gives concave "star" corners.

## Icon themes

- `index.theme` with `Inherits=breeze-dark,hicolor`. Unmapped names fall back to Breeze.
- To recolour, put KDE's hook in each SVG: `<style id="current-color-scheme">.ColorScheme-Text{color:#fcfcfc;}</style>` and `class="ColorScheme-Text" fill="currentColor"` on the paths.
- KIconLoader checks for an **exact size** across the whole inherit chain before it scales. Small fixed-size dirs (16/22/24) therefore won't hijack large icons, such as Dolphin's 48 px folders. GTK uses the spec lookup and may scale, so give folder and drive names `-symbolic` variants only.
- Apply with `/usr/libexec/plasma-changeicons <Theme>`. Verify with `kiconfinder6 <name>`.
- If an icon theme symlinks into another (`status -> ../WhiteSur-dark/status`), it inherits everything in that folder, including unwanted icons (an Apple logo as `start-here`). Symlink file by file and filter.

## Lock screen

- In Plasma 6 the lock screen is `lockscreen/LockScreenUi.qml` inside the **desktop shell package** (`/usr/share/plasma/shells/org.kde.plasma.desktop`), not the Look-and-Feel package.
- The greeter picks the package from `plasmashellrc [Shell] ShellPackage`, the **same key plasmashell uses for the desktop**. Pointing it at a lockscreen-only package breaks the desktop, and the panel layout file is named after the shell id.
- Override safely with a user copy at `~/.local/share/plasma/shells/org.kde.plasma.desktop`. It must hold **real files**: KPackage rejects symlinks that resolve outside the package ("Path traversal attempt"), and plasmashell would reject them too. Rebuild the copy from `/usr/share` before each login (`~/.config/plasma-workspace/env/*.sh`), and drop it if the patch anchor no longer matches.
- Test with `/usr/libexec/kscreenlocker_greet --testing` (add `--shell <id>` for another package). Logs go to the journal, not stdout. If the QML fails, the greeter falls back to its built-in locker ("falling back to built-in locker"), so unlocking still works.
- Useful context properties: `kscreenlocker_userName`, `lockScreenRoot.uiVisible` (true once the user presses a key). Battery: `org.kde.plasma.private.battery` → `BatteryControlModel`. Media: `org.kde.plasma.private.mpris`.

## Notifications

- Model: `org.kde.notificationmanager` → `Notifications { showNotifications; sortMode: SortByDate; sortOrder: Qt.DescendingOrder }`, read with `data(index, SummaryRole|BodyRole|ApplicationNameRole|IconNameRole|ImageRole|CreatedRole)`. Ignore inserts for the first few seconds (history replay) and items older than about 15 s.
- Do Not Disturb: `Settings.notificationsInhibitedUntil`, `notificationsInhibitedByApplication`. To toggle it, write `plasmanotifyrc [DoNotDisturb] Until` (KConfig datetime `Y,M,D,h,m,s`) with `kwriteconfig6 --notify`.
- There is **no global "no popups" switch**. Set `ShowPopups=false` per app (`[Applications][<desktop-id>]`) and per service (`[Services][<notifyrc name>]`). Senders without a desktop entry still pop up.

## QML pitfalls

- `Item.left` (and other anchor-line names) are FINAL. A custom `property int left` fails with "Cannot override FINAL property". Use `remaining` and similar names.
- plasmashell caches compiled QML. After you edit a widget, `rm -rf ~/.cache/plasmashell/qmlcache` and restart.
- Use `Plasma5Support.DataSource { engine: "executable" }` to run shell commands from QML. Disconnect each source in `onNewData`, and add a `#<timestamp>` suffix so the same command can re-run.
- Private modules (`org.kde.plasma.private.*`) are not API: pin to the Plasma version and test after updates.
- Sensors: `org.kde.ksysguard.sensors` → `Sensor { sensorId: "cpu/all/usage" }`. List ids with `kstatsviewer --list`. Battery sensor ids are machine-specific, so prefer `BatteryControlModel`.

## Other config facts

- Fonts: `kdeglobals [General] font/menuFont/toolBarFont/smallestReadableFont/fixed`, `[WM] activeFont`. Use `kwriteconfig6 --notify` so running apps update. GTK uses `~/.config/gtk-{3,4}.0/settings.ini gtk-font-name`.
- Power: `powerdevilrc [AC][Display] DimDisplayWhenIdle/TurnOffDisplayWhenIdle`, `[AC][SuspendAndShutdown] AutoSuspendAction=0`, then `org.kde.Solid.PowerManagement.reparseConfiguration`.
- Charge limit: `/sys/class/power_supply/BAT0/charge_control_end_threshold` if present. HP exposes it via `hp-bioscfg` at `/sys/class/firmware-attributes/hp-bioscfg/attributes/Battery Health Manager`.
- Splash: a Look-and-Feel package containing only `contents/splash/Splash.qml`. `stage` goes 1→6. Select it with `ksplashrc [KSplash] Theme`. Test with `ksplashqml --test --window <id>`.
- Wallpaper plugins are QML packages (`KPackageStructure: Plasma/Wallpaper`). Animated wallpapers keep the compositor busy (about 20% CPU in testing), so warn about battery.
- Kvantum translucency: `translucent_windows`, `blurring`, `reduce_window_opacity` in the theme's `.kvconfig`. GTK and Electron apps ignore it.
