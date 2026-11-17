# Rebuilding glasskit on other platforms

Rebuild the same pieces with each platform's native tools. The table names the tool for each piece. Check versions before you rely on an API, and read any third-party installer before you suggest it.

| Piece | GNOME | COSMIC (Pop!_OS) | Hyprland / Sway | macOS | Windows |
|---|---|---|---|---|---|
| Glass top bar + glow | Shell theme CSS (`#panel`), "Blur my Shell" extension | COSMIC panel settings (transparency); applets for the rest | Waybar CSS + compositor blur (`layerrule = blur, waybar`) | SketchyBar (`blur_radius`, `border_color`) | Windhawk / TranslucentTB + Rainmeter skin |
| Island | GNOME Shell extension (GJS, St widgets) | COSMIC applet in Rust (`libcosmic`, iced) | Eww or AGS widget on a layer-shell window | SketchyBar item with popup | Rainmeter skin, or a WinUI app |
| Desktop widgets | Extension (St actors on `Main.layoutManager`) or Conky | COSMIC applets, or a layer-shell overlay | Eww / AGS widgets | Übersicht widgets (HTML/JSX) | Rainmeter skins |
| Filled rounded icons | Icon theme with `-symbolic` SVGs (GTK recolours `fill:#bebebe`/`currentColor` symbolic icons) | Same freedesktop icon theme | Same, plus Waybar icon fonts | SF Symbols are built in; for the bar use SF Symbols `.fill` variants | Segoe Fluent Icons (filled variants) |
| Font | `gsettings set org.gnome.desktop.interface font-name 'Nunito 10'` | COSMIC Settings → Appearance → fonts | GTK settings.ini + bar CSS | Can't replace the system UI font; use it in widgets | Not system-wide; use it in widgets |
| Boot splash | Plymouth theme (script module), needs root | Plymouth | Plymouth | Not customisable | Not customisable |
| Lock screen | Hard to theme; an extension can restyle the unlock dialog | Not themeable yet | hyprlock / swaylock-effects config | Not themeable | Not themeable (wallpaper only) |
| Touchpad gestures | Built in (3/4-finger); remap with the "Gesture Improvements" extension | Built-in workspace/overview gestures; check Settings → Touchpad | `gesture = 3, horizontal, workspace` (Hyprland 0.51+) or `bindgesture` in Sway; libinput-gestures on X11 | Native (System Settings → Trackpad) | Native (Settings → Touchpad → Three-finger gestures) |
| Notifications in island | Extension hooks `Main.messageTray` sources | Watch `org.freedesktop.Notifications` with your own daemon | Replace the notification daemon (swaync, mako) and forward to Eww | Not possible (sandboxed) | Not supported |

## COSMIC (Pop!_OS 24.04 and later)

Pop!_OS replaced GNOME with **COSMIC**, System76's Rust desktop built on `libcosmic` (iced). Panel items are **applets**: Rust binaries declared through a `.desktop` file with `X-CosmicApplet=true`. Build them with `libcosmic`'s applet helpers. Theme colours and corner radius live in COSMIC Settings → Appearance, stored under `~/.config/cosmic/com.system76.CosmicTheme.*`. Blur support depends on the compositor version, so check it before you promise frosted glass. The island maps naturally to a panel applet with a popup.

## GNOME

Extensions are GJS: `metadata.json` plus `extension.js`, installed in `~/.local/share/gnome-shell/extensions/<uuid>`. Match the `shell-version` to `gnome-shell --version`. Panel islands live in `Main.panel._centerBox`. Media comes from the MPRIS D-Bus, and battery from UPower (`org.freedesktop.UPower`). On Wayland, the user must log out to load a new extension.

## Hyprland / Sway

A layer-shell bar (Waybar) and widgets (Eww or AGS) on top of compositor blur. Glass is CSS `background: rgba(16,18,22,.55)` plus a blur rule. Make the glow with `box-shadow: 0 0 10px rgba(156,194,255,.16)`, which works because these bars render real CSS. Media comes from `playerctl -F`, notifications from swaync's subscribe API.

## macOS

SketchyBar (bar and island) plus Übersicht (desktop widgets). Media comes from `nowplaying-cli`, and battery from `pmset -g batt`. You can't restyle system UI, the lock screen or the boot screen.

## Windows

Rainmeter skins (desktop widgets, an island-like bar) plus Windhawk mods or TranslucentTB for the taskbar. Media comes from the Windows media session (Rainmeter WebNowPlaying plugin). Get explicit consent before installing anything that hooks system processes.
