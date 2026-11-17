# glasskit for COSMIC

The glasskit look for the COSMIC desktop (Pop!_OS 24.04): an Island in the top bar with a pixel pet, Clock and Status applets, desktop widgets, a launcher, a frosted dark theme, a floating panel and dock, filled icons, a cursor, touchpad gestures and a boot splash. Everything installs per user, no root. Each piece is its own **part**, so you take only what you want.

```bash
git clone https://github.com/n1th1n-19/glasskit && cd glasskit/cosmic && ./glasskit-cosmic install island launcher
```

`install` also links `~/.local/bin/glasskit-cosmic` (and records the repo path in `~/.config/glasskit/repo`), so afterwards `glasskit-cosmic update` works from anywhere.

## Usage

```
glasskit-cosmic install <parts...|all>   add parts (build, install, apply)
glasskit-cosmic update                   update every installed part (git pull, rebuild, reinstall)
glasskit-cosmic update <parts...>        update only some parts
glasskit-cosmic uninstall                remove everything glasskit installed
glasskit-cosmic uninstall <parts...>     remove only some parts, e.g. glasskit-cosmic uninstall widgets cursor
glasskit-cosmic list                     all parts: installed or not, version, commit
glasskit-cosmic status                   what is installed and whether it is current
glasskit-cosmic version | help
```

`install`, `update` and `uninstall` accept `--yes` (no questions) and `--dry-run` (print what would run, change nothing). `--no-build` reuses an existing cargo build, `--with-shortcut` adds `launcher-shortcut` to `all`, `--purge` lets `uninstall` delete your `island.conf` (without it, the file is kept).

Examples:

```bash
glasskit-cosmic install all --dry-run          # see everything it would do
glasskit-cosmic install island applets panel   # Island + Clock/Status, then the layout that uses them
glasskit-cosmic update                         # git pull --ff-only, rebuild and reinstall what changed
glasskit-cosmic update island --yes
glasskit-cosmic uninstall widgets cursor       # only those two
glasskit-cosmic uninstall                      # all of it; your settings come back from the backups
```

## Parts

| Part | What it is | What it touches |
|---|---|---|
| `island` | The Island top-bar applet: music, notifications, volume and charging in a pill, and an idle pixel pet ([README-pet.md](island/README-pet.md)) | `~/.local/bin/glasskit-island`, its `.desktop` file, `~/.config/glasskit/island.conf` (made once from [island.conf.example](island/island.conf.example), never overwritten) |
| `applets` | Clock and Status panel applets (`clock` and `status` are accepted as names, they share one binary and install together) | `~/.local/bin/glasskit-applets`, two `.desktop` files |
| `widgets` | The seven desktop cards: clock, calendar, system rings, media, toggles, storage and network, Pomodoro | `~/.local/bin/glasskit-widgets`, seven autostart entries in `~/.config/autostart` |
| `launcher` | `glasskit-launcher`, a glass app launcher popup (run it again to close it) | `~/.local/bin/glasskit-launcher` |
| `launcher-shortcut` | Binds Super to the launcher | Adds one entry to COSMIC's custom shortcuts, after a backup. Not part of `all` unless you pass `--with-shortcut`. Super alone may overlap COSMIC's own launcher binding |
| `theme` | Accent `#3b82f6`, background `#0c0e12`, frosted panels, rounder corners, Nunito font | The dark theme builder and Tk settings in `~/.config/cosmic`, after a backup. Nunito comes from your system, the copy in this repo, or a download, always sha256-checked |
| `panel` | Floating pill top bar (workspaces and app button left, Island in the middle, tiling, clock, status, notifications, power right) and a floating bottom dock. The app button shows its icon. Only applets that are installed are listed, glasskit's own Clock/Status when present, COSMIC's stock ones otherwise | Panel, Dock and PanelButton settings in `~/.config/cosmic`, after a backup |
| `icons` | `glasskit-filled`: UI icons from Material Symbols Rounded, filled, symbolic only (app icons stay as they are) | `~/.local/share/icons/glasskit-filled`, the icon theme setting |
| `cursor` | Bibata-Modern-Ice 2.0.7 from the upstream GitHub release. The archive's sha256 and every path inside it are checked before extracting. Log out and in afterwards | `~/.icons/Bibata-Modern-Ice`, `~/.icons/default`, `~/.config/environment.d/glasskit-cursor.conf` |
| `gestures` | Windows-style gestures through libinput-gestures and ydotool 0.1.8: 3 fingers up = overview, down = launcher, left/right = Alt+Tab / Alt+Shift+Tab | `~/.config/libinput-gestures.conf` (an existing one is backed up), an autostart entry |
| `splash` | The grain boot splash. It builds the frames in `~/.cache/glasskit/splash` (your clone stays clean) and **prints** the sudo commands from [splash/INSTALL.md](splash/INSTALL.md). glasskit never runs sudo | Only that cache folder |

`all` = `island applets widgets launcher theme panel icons gestures`. `cursor`, `splash` and `launcher-shortcut` only install when you name them (`launcher-shortcut` also with `--with-shortcut`). Parts always install in a fixed order, whatever order you type them in. Installing a part twice is safe.

## Safe by design

- Before touching `~/.config/cosmic`, `theme`, `panel` and `launcher-shortcut` save the settings they edit in `~/.local/state/glasskit/backups/` (`<tag>-orig.tgz` is made once and kept, plus the newest 5 timestamped ones). Restore by hand with `glasskit-restore theme|panel|shortcuts` (add `--latest` for the newest backup instead of the original, `--list` to see them).
- `uninstall` removes only files glasskit made: binaries in `~/.local/bin`, `.desktop` files, autostart entries, the icon theme, and a cursor or font only if glasskit put it there. For `theme` and `panel` it restores your backed-up settings. It never touches Ember or other applets. Your `island.conf` is kept unless you answer yes at the prompt or pass `--purge`.
- `update` runs `git pull --ff-only` in the clone. If the clone has local changes it warns and skips the pull, so nothing of yours is discarded. It rebuilds and reinstalls only the parts whose files changed, and prints `already up to date` when nothing did. `status` shows the recorded commit of each part against the clone's.
- `--dry-run` prints `would run:` lines and changes nothing.
- The installed list is `~/.local/state/glasskit/cosmic-installed`: one part per line with the commit it was installed from.
- COSMIC watches its config files, so panel and theme changes show at once and nothing is restarted. If the colours do not change, toggle Dark/Light once in COSMIC Settings > Appearance.

## Scripts in `bin/`

`theme` and `panel` link these into `~/.local/bin`; you can also run them yourself.

| Script | Purpose |
|---|---|
| `glasskit-theme-apply [--colors] [--font] [--icons]` | Re-apply the look after a theme change in COSMIC Settings resets it |
| `glasskit-panel-apply` | Re-apply the panel and dock layout |
| `glasskit-gestures-apply` | Write the gestures config |
| `glasskit-restore <theme\|panel\|shortcuts>` | Put back the settings saved before glasskit's first edit |
| `glasskit-icons-build-cosmic` | Build the `glasskit-filled` icon theme |

## Requirements

- COSMIC 1.11 on Pop!_OS 24.04 (cosmic-session 1.11.0 is what this was tested on). Other versions may work, but panel config keys can change between releases.
- `git`, and Rust with `cargo` ([rustup](https://rustup.rs)) for `island`, `applets`, `widgets` and `launcher`, plus a C compiler and the `-dev` packages libcosmic needs (`build-essential pkg-config`, then whatever the build asks for, usually `libxkbcommon-dev libwayland-dev libssl-dev libfontconfig-dev`). The first build takes a few minutes. Cargo fetches `libcosmic` and the `ember-core` library from GitHub; this does not install Ember.
- `python3` for `icons`, and `python3-pil` (Pillow) for `splash`.
- `curl` or `gh`, plus `tar` and `xz`, for `cursor`. `curl` for a Nunito download if you have no copy.
- For `gestures`: `sudo apt install libinput-tools ydotool` (glasskit prints this line if they are missing and never runs it), the `libinput-gestures` command, your user in the `input` group, and write access to `/dev/uinput`. The key names are those of ydotool 0.1.8, the Ubuntu 24.04 package; ydotool 1.x uses different key syntax.

## Known limits

- Ember's own widget (`Ember.Widget` autostart) overlaps the glasskit cards. Disable one of them.
- `panel` rewrites the layout of the top bar and the dock; applets from other projects in those bars are not carried over (they come back with `uninstall panel` or `glasskit-restore panel`). Re-run `glasskit-cosmic install panel` after adding `island` or `applets` so the layout picks them up.
- `theme` edits the dark theme only.
- `clock` and `status` are one binary, so they install and uninstall together, and `widgets` is installed as a whole (all seven cards).
- Cards already on screen stay until you log out after `uninstall widgets` (or `pkill glasskit-widgets`).
- The Island applet, widgets and launcher are built against `libcosmic` main, which moves quickly; a COSMIC update can need a rebuild (`glasskit-cosmic update` after the repo catches up).
- The boot splash needs sudo for the final steps, so you run them yourself.

## Credits

- Pixel pets by [Frakassets](https://frakassets.itch.io/free-pets-pack), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Only the 32 px scroller sheets are bundled, with their licence, in `island/assets/pets/`.
- [Bibata cursor](https://github.com/ful1e5/Bibata_Cursor) by ful1e5, GPL-3.0, downloaded at install time.
- [Nunito](https://github.com/google/fonts/tree/main/ofl/nunito) by Vernon Adams and others, SIL OFL 1.1. A copy is bundled in `splash/` for the splash and the theme.
- Icon artwork: [Material Symbols](https://github.com/google/material-design-icons) (Google), Apache-2.0, downloaded when `icons` is built.

glasskit's own code is GPL-3.0-or-later. See [../CREDITS.md](../CREDITS.md).
