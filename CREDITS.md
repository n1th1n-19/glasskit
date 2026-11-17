# Credits

glasskit's own code (widgets, scripts, generators, splash, lock-screen patch) is GPL-3.0-or-later.

glasskit downloads or builds on these projects. None of them is bundled in this repository:

| Project | Used for | License |
|---|---|---|
| [Material Symbols](https://github.com/google/material-design-icons) (Google) | Icon artwork, downloaded by `glasskit-icons-build` at install time | Apache-2.0 |
| [Nunito](https://github.com/google/fonts/tree/main/ofl/nunito) (Vernon Adams et al.) | UI font, downloaded at install time | SIL OFL 1.1 |
| [WhiteSur-kde](https://github.com/vinceliuice/WhiteSur-kde) (vinceliuice) | Base Plasma style that `glasskit-glass-style` copies on your machine | GPL-3.0 |
| [KDE Plasma](https://invent.kde.org/plasma) | The stock lock screen that `glasskit-lockscreen-sync` patches on your machine | GPL-2.0-or-later |
| [kwin-winlike-gestures](https://github.com/LiberaTeMetuMortis/kwin-winlike-gestures) | Optional touchpad gestures; the rebuild hook only | see project |

Techniques: Plasma FrameSvg 9-slice frames, KDE `current-color-scheme` icon recolouring, and the film-grain dissolve (keep a pixel when random < blurred alpha) used for the splash.
