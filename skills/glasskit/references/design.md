# glasskit design

## Tokens

| Token | Value | Use |
|---|---|---|
| ink | `#0c0e12` | Backgrounds, splash, lock screen |
| card | `rgba(20,23,28,0.86)` = `Qt.rgba(0.08,0.09,0.11,0.86)` | Desktop widget cards |
| glass | `#101216` at 55% + blur | Top bar, popups (popups `#121419` at 58%) |
| line | `#262a33` | Card borders (1 px) |
| track | `#22262e` | Ring and bar tracks, idle buttons |
| text | `#f4f5f7` / muted `#8b93a3` | Primary / secondary text |
| accent | `#3b82f6` | The only accent: active toggles, progress, dot |
| semantic | green `#34d399`, orange `#fb923c`, purple `#a78bfa`, red `#f87171` | Charging, warning/temperature, RAM, low battery |
| glow | `#9cc2ff`, peak opacity 0.16, 10 px | Bar edge glow |

- Font: Nunito (UI 10 pt, toolbar 9 pt Medium, titles Bold, big numbers Black). JetBrains Mono for code. Use tabular figures for anything that ticks.
- Cursor: Bibata Modern Ice (white, rounded tips); Bibata Modern Classic on light setups.
- Radius: widget cards 28 px, popups 18 px, bar 12 px, buttons fully round.
- No gradients, no decorative blobs, no emoji as icons. Glow comes from stacked strokes, not gradient fills.
- Motion: springy width changes (`OutBack`, overshoot 1.1, about 420 ms), opacity cross-fades 200 ms, nothing bouncing at rest except music equaliser bars.

## Widget catalogue (rebuild these on any platform)

| Widget | Size | Content |
|---|---|---|
| Clock | 360×192 | Big time (Black), date (Bold), day-progress bar in accent |
| Calendar | 360×304 | Month name, Monday-first grid, today in an accent circle |
| System | 360×240 | Four rings: CPU (accent), RAM (purple), battery (green), temperature (orange) |
| Media | 360×144 | Art (radius 18), title, artist, prev / play (accent, larger) / next |
| Toggles | 360×176 | Round 52 px buttons: Wi-Fi, Bluetooth, Focus/DND, Night Light, Mute; accent when on |
| Storage & Network | 360×224 | Used % (Black), "X free of Y", bar (orange above 85%), ↓/↑ speeds |
| Focus | 360×170 | 25/5 ring countdown, phase label, start/reset/skip, notification at the end |
| Island (bar) | bar height | Idle dot → media (art, title, equaliser) → notify (icon, bold title + grey body) → volume bar → charge / low-battery flash; orange mic dot; click opens a media card with a progress bar |
| Battery Pill (bar) | bar height | Battery glyph rotated 90° (terminal right) + %; green when charging, orange at 15% or below |

Layout: two columns 48 px from the screen edges, vertically centred between the bar and the dock, 16 px gaps, both columns the same height. Keep the centre clear for the wallpaper.

## Splash

The wordmark is built from grain: keep a pixel if random < blurred alpha × reveal, with a crisp solid core. The full stop is in the accent colour and blinks (its keep-probability dims) until stage 6, then holds and fades.
