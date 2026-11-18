# glasskit: instructions for Claude Code

This repo installs the glasskit desktop on KDE Plasma 6: Island, battery pill, desktop widgets, glass Plasma style, filled icons, Bibata cursor, grain splash, big-clock lock screen and optional touchpad gestures. Before installing, **check this machine, then install and adapt the setup to its hardware**. The skill in `skills/glasskit/` has the full Plasma gotchas (`references/plasma.md`).

Work in this order. Don't skip the checks. Show the user a short summary of what you found and what you will change before you change anything.

## 1. Pre-flight checks

Run these and keep the results:

```bash
# system
cat /etc/os-release | grep -E '^(NAME|VERSION_ID)='; plasmashell --version; kwin_wayland --version 2>/dev/null
echo "$XDG_SESSION_TYPE $XDG_CURRENT_DESKTOP"; whoami; echo "$HOME"; df -h "$HOME" | tail -1
# displays: count, resolution, refresh rate, scale, primary
kscreen-doctor -o
# memory, CPU, GPU
free -h; nproc; lscpu | grep -E 'Model name|^CPU\(s\)'
lspci -k | grep -A3 -E 'VGA|3D|Display'; command -v nvidia-smi >/dev/null && nvidia-smi --query-gpu=name,driver_version --format=csv,noheader
# power and input
ls /sys/class/power_supply/; cat /sys/class/power_supply/BAT*/charge_control_end_threshold 2>/dev/null
ls /sys/class/firmware-attributes/ 2>/dev/null
grep -i -E 'touchpad' /proc/bus/input/devices | sort -u
gdbus call --system --dest net.hadess.PowerProfiles --object-path /net/hadess/PowerProfiles --method org.freedesktop.DBus.Properties.Get net.hadess.PowerProfiles ActiveProfile
```

Stop and ask the user before continuing if:
- it isn't Plasma 6 on Wayland, or Plasma isn't 6.8.x. The source laptop ran 6.8.3: Island, Media and the lock-screen cards use private Plasma modules that can change between versions, so read the journal after restoring.
- free space in `$HOME` is under 2 GB.
- the user already has a customised Plasma setup they may want to keep: back up `~/.config/plasma-org.kde.plasma.desktop-appletsrc`, `kdeglobals` and `kwinrc` first.

## 2. Install

Run `./install.sh` (user-level, no sudo). Add `--apply --word NAME` to switch the look and set the splash wordmark. `--lockscreen`, `--island-popups` and `--gestures` are opt-in, so ask the user about each one. `--gestures` needs a terminal for sudo, so ask the user to run that part themselves. Then add the widgets (Add Widgets → search "Glasskit") or place them with Plasma scripting, adapt them as below, and have the user log out and back in.

## 3. Adapt to this hardware

The reference layout was built on a **single 1920×1080 60 Hz display at scale 1.0**, with a battery, a touchpad, an Intel i5-8350U (4 cores / 8 threads), 15 GB RAM and Intel UHD 620 graphics only (no discrete GPU). The full glass effects ran fine on that, so anything at least that strong can keep them. Reference layout, in logical pixels:
- Top bar: 44 px, floating, translucent glass, auto-hide. Bottom dock (if any): 104 px.
- Desktop widgets: two columns, cards 368 px wide (360 + 8 margin), 48 px from the left and right edges, 16 px gaps. Left column: Clock 192, Calendar 304, Media 144. Right column: Toggles 176, System 240, Storage 224. Each column is 672 px tall, centred between the bar and the dock.

### Displays
- **Resolution differs from 1920×1080:** recompute the widget positions. For a logical width `W` and height `H`: left x = 48, right x = `W - 48 - 368`. Top of both columns = `round16((H - 44 - 104 - 672) / 2)`, stored as that value. Plasma adds the bar height itself, which is why the reference layout stores 128 for an on-screen 172. Write `ItemGeometries-<W>x<H>` **and** `ItemGeometriesHorizontal` in the desktop containment group of `~/.config/plasma-org.kde.plasma.desktop-appletsrc`, as `Applet-<id>:x,y,w,h,0;`. Stop plasmashell first and start it after, because it rewrites the file on exit. Applet ids: read them from the same file (`plugin=org.glasskit.*`).
- **Small screens (logical height under 900):** a 672 px column won't fit between the bars. Drop Calendar from the left column, or put all six widgets in one row of three columns. Ask the user which.
- **HiDPI (scale above 1):** positions and panel heights are in logical pixels, so use logical `W×H` (physical divided by scale). If the scale is 1.0 on a high-density screen (for example 2560×1600 at 13–14 inches), suggest 1.25–1.5 with `kscreen-doctor output.<name>.scale.<n>`. Get the user's OK first.
- **Multiple displays:** keep the top bar, Island, dock and desktop widgets on the **primary** display only. Panels have a `lastScreen`/`screen` entry in `plasma-org.kde.plasma.desktop-appletsrc` and `plasmashellrc`. Check that they point at the primary output in `kscreen-doctor -o`. Leave secondary screens clean unless the user wants a bar there.
- **Refresh rate:** at 90 Hz or higher keep the animation speed (`kdeglobals [KDE] AnimationDurationFactor=0.7`). On 60 Hz panels that feel choppy, offer 0.5.

### Performance (RAM, CPU, GPU)
- **8 GB RAM or less, 4 CPU threads or fewer, or an old iGPU:** lighten the effects. Set `kwinrc [Effect-blur] BlurStrength` 10 → 6, `[Plugins] contrastEnabled=false` and `magiclampEnabled=false` (use the default minimise animation), and turn off blur in the Kvantum theme (`~/.config/Kvantum/WhiteSurDark/WhiteSurDark.kvconfig`: `blurring=false`, `popup_blurring=false`). Keep Island and the widgets, which are cheap. Don't add an animated wallpaper.
- **NVIDIA proprietary driver on Wayland:** after the restore, check that blur works and nothing flickers (open the launcher and a tray popup). If panels flicker or blur is missing, make sure `nvidia-drm.modeset=1` is set (`cat /sys/module/nvidia_drm/parameters/modeset`), and only then consider lowering translucency.
- **Strong GPU and 16 GB RAM or more:** keep everything as restored.

### Power and input
- **No battery (desktop PC):** don't add the Battery Pill, remove the BAT ring from the System widget (or tell the user it will show 0%), and ignore battery settings. Island's charge faces never trigger, which is fine.
- **Battery present:** the restored AC power profile never dims or sleeps the screen. Keep it unless the user says otherwise. Charge limit: if `charge_control_end_threshold` exists, offer to set 80% (needs root: `echo 80 | sudo tee …`, made persistent with a udev rule or TLP). On HP machines check `/sys/class/firmware-attributes/hp-bioscfg/attributes/Battery Health Manager`. Never change firmware settings without asking.
- **No touchpad:** skip the touchpad settings and don't build the gesture plugin.
- **Touchpad present:** suggest tap to click, natural scrolling and two-finger right-click (kcminputrc `[Libinput][<vendor>][<product>][<name>]`, ids from `/proc/bus/input/devices`). Gestures (optional, `glasskit-gestures`) need the plugin built for this exact KWin version.

## 4. Verify, then report

- `journalctl --user -u plasma-plasmashell --since "-5min" | grep -i -E 'error|org.glasskit'` should show no QML errors.
- Lock screen: test only with `/usr/libexec/kscreenlocker_greet --testing` (no real lock). There should be no "falling back to built-in locker" line in the journal.
- Take one screenshot (`spectacle --background --nonotify --fullscreen --output /tmp/check.png`) and look at it: bar, Island, dock and widgets should be visible and not overlapping.
- Tell the user what you changed for this hardware, what you skipped, and what still needs them (log out and in).

## Rules learned while building glasskit (don't relearn them)

- Plasma rewrites `plasma-org.kde.plasma.desktop-appletsrc` on exit. Edit panel order and widget geometry only while plasmashell is stopped, or write the change and restart twice.
- Don't restart plasmashell more than about four times a minute, because systemd rate-limits it. Recover with `systemctl --user reset-failed plasma-plasmashell.service && systemctl --user start plasma-plasmashell.service`.
- Plasma's cursor list only scans `~/.icons`. Switch to another cursor theme and back to apply one live.
- A Global Theme switch resets colours, icons, Plasma style, decorations, cursor, splash and wallpaper. `glasskit-theme-apply` restores them.
- The lock screen lives in a user copy of the desktop shell package (`~/.local/share/plasma/shells/org.kde.plasma.desktop`), rebuilt at each login by `glasskit-lockscreen-sync`. Never point `plasmashellrc [Shell] ShellPackage` at another package: that also replaces the desktop.
- The rest of the gotchas are in `skills/glasskit/references/plasma.md`.
