// SPDX-License-Identifier: GPL-3.0-or-later
//! `~/.config/glasskit/island.conf`: `key = value` lines, `#` comments, blank lines ok.
//! Pure parsing: anything unknown or invalid keeps its default (and yields a warning line).
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    Slow,
    Normal,
    Fast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hops {
    Never,
    Rare,
    Normal,
    Often,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conf {
    /// Raw `pet` word; validated by `sprites::resolve`.
    pub pet: Option<String>,
    pub popup_pet: bool,
    pub pet_in_panel: bool,
    /// Desktop pet card (glasskit-widgets `--card pet`); read there, ignored by the island.
    pub pet_widget: bool,
    pub sleep_after_seconds: u32,
    pub walk_speed: Speed,
    pub hop_frequency: Hops,
    /// None = auto.
    pub scale: Option<u8>,
    pub animation_fps: u32,
}

impl Default for Conf {
    fn default() -> Self {
        Conf { pet: None, popup_pet: true, pet_in_panel: true, pet_widget: true, sleep_after_seconds: 60, walk_speed: Speed::Normal, hop_frequency: Hops::Normal, scale: None, animation_fps: 10 }
    }
}

fn flag(v: &str) -> Option<bool> {
    match v {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// Parse the file text; returns the settings and one message per ignored line.
pub fn parse(text: &str) -> (Conf, Vec<String>) {
    let (mut c, mut warn) = (Conf::default(), Vec::new());
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            warn.push(format!("line {}: expected key = value", n + 1));
            continue;
        };
        let (k, v) = (k.trim().to_lowercase(), v.split('#').next().unwrap_or("").trim().to_lowercase());
        let num = v.parse::<u32>().ok();
        let ok = match k.as_str() {
            "pet" => { c.pet = Some(v.clone()).filter(|s| !s.is_empty()); true }
            "popup_pet" => flag(&v).map(|b| c.popup_pet = b).is_some(),
            "pet_in_panel" => flag(&v).map(|b| c.pet_in_panel = b).is_some(),
            "pet_widget" => flag(&v).map(|b| c.pet_widget = b).is_some(),
            "sleep_after_seconds" => num.map(|n| c.sleep_after_seconds = n.min(86_400)).is_some(),
            "animation_fps" => num.map(|n| c.animation_fps = n.clamp(4, 15)).is_some(),
            "walk_speed" => ["slow", "normal", "fast"].iter().position(|w| *w == v).map(|i| c.walk_speed = [Speed::Slow, Speed::Normal, Speed::Fast][i]).is_some(),
            "hop_frequency" => ["never", "rare", "normal", "often"].iter().position(|w| *w == v).map(|i| c.hop_frequency = [Hops::Never, Hops::Rare, Hops::Normal, Hops::Often][i]).is_some(),
            "scale" if v == "auto" => { c.scale = None; true }
            "scale" => num.map(|n| c.scale = Some(n.clamp(1, 3) as u8)).is_some(),
            _ => { warn.push(format!("line {}: unknown key '{k}'", n + 1)); continue; }
        };
        if !ok {
            warn.push(format!("line {}: bad value '{v}' for {k}, using the default", n + 1));
        }
    }
    (c, warn)
}

/// `$XDG_CONFIG_HOME/glasskit` or `~/.config/glasskit`.
pub fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("glasskit"))
}

/// Read island.conf (missing file = defaults); problems go to stderr, never fail.
pub fn load() -> Conf {
    let Some(text) = dir().and_then(|d| std::fs::read_to_string(d.join("island.conf")).ok()) else { return Conf::default() };
    let (c, warn) = parse(&text);
    for w in warn {
        eprintln!("glasskit-island: island.conf {w}");
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_comments_give_defaults() {
        assert_eq!(parse(""), (Conf::default(), vec![]));
        assert_eq!(parse("# pet = cat\n\n   \n  # x = y\n"), (Conf::default(), vec![]));
    }

    #[test]
    fn every_key_parses_with_whitespace_case_and_inline_comments() {
        let t = "  Pet =  FROG  # comment\npopup_pet=no\npet_in_panel = False\npet_widget = off\nsleep_after_seconds = 0\n walk_speed = Fast\nhop_frequency=often\nscale = 2\nanimation_fps = 12\n";
        let (c, w) = parse(t);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c, Conf { pet: Some("frog".into()), popup_pet: false, pet_in_panel: false, pet_widget: false, sleep_after_seconds: 0, walk_speed: Speed::Fast, hop_frequency: Hops::Often, scale: Some(2), animation_fps: 12 });
        assert_eq!(parse("scale = auto").0.scale, None);
    }

    #[test]
    fn unknown_keys_and_bad_values_keep_defaults_with_a_warning() {
        let (c, w) = parse("colour = red\nwalk_speed = warp\nhop_frequency = 3\npopup_pet = maybe\nsleep_after_seconds = soon\nscale = big\nno equals here\n");
        assert_eq!(c, Conf::default());
        assert_eq!(w.len(), 7);
    }

    #[test]
    fn numbers_are_clamped() {
        let c = parse("animation_fps = 1\nscale = 9\nsleep_after_seconds = 99999999").0;
        assert_eq!((c.animation_fps, c.scale, c.sleep_after_seconds), (4, Some(3), 86_400));
        assert_eq!(parse("animation_fps = 500\nscale = 0").0.animation_fps, 15);
        assert_eq!(parse("scale = 0").0.scale, Some(1));
    }

    #[test]
    fn example_template_parses_without_warnings() {
        let (c, w) = parse(include_str!("../island.conf.example"));
        assert!(w.is_empty(), "{w:?}");
        assert!(c.pet_widget);
    }

    #[test]
    fn empty_pet_is_unset() {
        assert_eq!(parse("pet =").0.pet, None);
    }
}
