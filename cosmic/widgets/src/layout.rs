// SPDX-License-Identifier: GPL-3.0-or-later
//! Card layout, all pure: `widgets.conf` parse / in-place edit, reference-px <-> output-px
//! conversion, and the drag maths (clamp, snap, anchor flip).
//!
//! Offsets in the file are REFERENCE px (1080p, scale 1.0); `to_px` multiplies by the output
//! scale. `anchor` says which screen edge `x` is measured from; `y` is always from the top.
use std::collections::HashMap;

/// Offsets are clamped to this many reference px (a typo cannot park a card in orbit).
pub const MAX_OFFSET: f32 = 8192.0;
/// Snap grid in reference px.
pub const GRID: f32 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

/// A card's placement in reference px.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Place {
    pub side: Side,
    pub x: f32,
    pub y: f32,
    pub visible: bool,
}

/// What the file says about one card; `None` = not set, use the default.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Over {
    pub side: Option<Side>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub visible: Option<bool>,
}

// ------------------------------------------------------------------ parse

struct Entry {
    line: usize,
    card: String,
    key: String,
    val: String,
    dotted: bool,
}

/// Every `key = value` line with the card it belongs to (`[card]` section or `card.key` prefix),
/// plus the `[card]` header lines. Comment lines (`#`, `##`) are skipped; `#` after a value too.
fn scan(text: &str) -> (Vec<Entry>, Vec<(String, usize)>, Vec<String>) {
    let (mut entries, mut heads, mut warn) = (Vec::new(), Vec::new(), Vec::new());
    let mut cur: Option<String> = None;
    for (i, raw) in text.lines().enumerate() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let t = t.split('#').next().unwrap_or("").trim();
        if let Some(name) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let name = name.trim().to_ascii_lowercase();
            heads.push((name.clone(), i));
            cur = Some(name);
            continue;
        }
        let Some((k, v)) = t.split_once('=') else {
            warn.push(format!("line {}: not `key = value`", i + 1));
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
        let (card, key, dotted) = match k.split_once('.') {
            Some((c, key)) => (c.trim().to_string(), key.trim().to_string(), true),
            None => match &cur {
                Some(c) => (c.clone(), k, false),
                None => {
                    warn.push(format!("line {}: `{k}` outside any [card] section", i + 1));
                    continue;
                }
            },
        };
        entries.push(Entry { line: i, card, key, val: v, dotted });
    }
    (entries, heads, warn)
}

fn flag(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// card name -> what the file sets. Later lines win. Problems become warnings, never errors.
/// Unknown cards / keys are kept out of the result but never touched in the file.
pub fn parse(text: &str, cards: &[&str]) -> (HashMap<String, Over>, Vec<String>) {
    let (entries, _, mut warn) = scan(text);
    let mut out: HashMap<String, Over> = HashMap::new();
    for e in entries {
        let n = e.line + 1;
        if !cards.contains(&e.card.as_str()) {
            warn.push(format!("line {n}: unknown card `{}`", e.card));
            continue;
        }
        let o = out.entry(e.card.clone()).or_default();
        match e.key.as_str() {
            "anchor" => match e.val.to_ascii_lowercase().as_str() {
                "left" => o.side = Some(Side::Left),
                "right" => o.side = Some(Side::Right),
                _ => warn.push(format!("line {n}: anchor must be left or right")),
            },
            "x" | "y" => match e.val.parse::<f32>().ok().filter(|v| v.is_finite()) {
                Some(v) => {
                    let v = v.clamp(0.0, MAX_OFFSET);
                    if e.key == "x" { o.x = Some(v) } else { o.y = Some(v) }
                }
                None => warn.push(format!("line {n}: `{}` is not a number", e.key)),
            },
            "visible" => match flag(&e.val) {
                Some(b) => o.visible = Some(b),
                None => warn.push(format!("line {n}: visible must be true or false")),
            },
            k => warn.push(format!("line {n}: unknown key `{k}`")),
        }
    }
    (out, warn)
}

// ------------------------------------------------------------------ serialize / edit in place

/// Reference px as text: 2 decimals, trailing zeros trimmed (`48`, `12.5`).
pub fn fmt_num(v: f32) -> String {
    let s = format!("{:.2}", v.clamp(0.0, MAX_OFFSET));
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn place_kvs(p: &Place) -> Vec<(&'static str, String)> {
    vec![("anchor", p.side.name().into()), ("x", fmt_num(p.x)), ("y", fmt_num(p.y)), ("visible", p.visible.to_string())]
}

/// `text` with `card`'s `keys` set. An existing line keeps its key spelling, indentation, inline
/// comment and line ending (only the value changes); a missing key is added to the card's
/// `[card]` section; a missing section is appended. Every other line is untouched.
pub fn set_keys(text: &str, card: &str, kvs: &[(&str, String)]) -> String {
    let mut lines: Vec<String> = text.split_inclusive('\n').map(String::from).collect();
    for (key, val) in kvs {
        let (entries, heads, _) = scan(&lines.concat());
        if let Some(e) = entries.iter().rev().find(|e| e.card == card && e.key == *key) {
            let l = &lines[e.line];
            let body = l.trim_end_matches(['\r', '\n']);
            let eol = &l[body.len()..];
            let eq = body.find('=').unwrap_or(0);
            let tail = body[eq..].find('#').map_or(String::new(), |h| format!("  {}", &body[eq + h..]));
            lines[e.line] = format!("{}= {val}{tail}{eol}", &body[..eq]);
            continue;
        }
        let head = heads.iter().find(|h| h.0 == card).map(|h| h.1);
        match head {
            Some(h) => {
                let at = entries.iter().filter(|e| e.card == card && !e.dotted && e.line > h).map(|e| e.line).max().unwrap_or(h);
                if !lines[at].ends_with('\n') {
                    lines[at].push('\n');
                }
                lines.insert(at + 1, format!("{key} = {val}\n"));
            }
            None => {
                if let Some(last) = lines.last_mut().filter(|l| !l.ends_with('\n')) {
                    last.push('\n');
                }
                if !lines.is_empty() {
                    lines.push("\n".into());
                }
                lines.push(format!("[{card}]\n"));
                lines.push(format!("{key} = {val}\n"));
            }
        }
    }
    lines.concat()
}

// ------------------------------------------------------------------ px maths

/// A card's margins in OUTPUT px: `x` from the `side` edge, `y` from the top.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Px {
    pub side: Side,
    pub x: i32,
    pub y: i32,
}

#[cfg(test)]
pub fn to_px(p: &Place, s: f32) -> Px {
    Px { side: p.side, x: (p.x * s).round() as i32, y: (p.y * s).round() as i32 }
}

/// Inverse of `to_px` (to 0.01 ref px, so `to_px(to_ref(px)) == px` at any scale 0.5..=1.5).
pub fn to_ref(px: Px, s: f32) -> (f32, f32) {
    let r = |v: i32| (v as f32 / s * 100.0).round() / 100.0;
    (r(px.x), r(px.y))
}

/// Card and output size in output px.
#[derive(Clone, Copy, Debug)]
pub struct Dims {
    pub w: i32,
    pub h: i32,
    pub out_w: i32,
    pub out_h: i32,
}

/// Keep the card fully on the output.
pub fn clamp(p: Px, d: Dims) -> Px {
    Px { side: p.side, x: p.x.clamp(0, (d.out_w - d.w).max(0)), y: p.y.clamp(0, (d.out_h - d.h).max(0)) }
}

/// Left edge of the card in absolute output px.
pub fn left_edge(p: Px, d: Dims) -> i32 {
    match p.side {
        Side::Left => p.x,
        Side::Right => d.out_w - d.w - p.x,
    }
}

/// Switch the anchor when the card's centre crosses the horizontal midpoint, converting the
/// margin so the card does not move on screen. Exactly on the midpoint keeps the current side.
pub fn flip_side(p: Px, d: Dims) -> Px {
    let centre2 = 2 * left_edge(p, d) + d.w; // twice the centre, stays integral
    let want = if centre2 > d.out_w {
        Side::Right
    } else if centre2 < d.out_w {
        Side::Left
    } else {
        p.side
    };
    if want == p.side {
        return p;
    }
    Px { side: want, x: d.out_w - d.w - p.x, y: p.y }
}

/// Move by a pointer delta given in SURFACE coordinates (the surface moves with the card, so
/// the delta since the press point is exactly how far the card has to go).
pub fn drag_by(p: Px, dx: f32, dy: f32) -> Px {
    let dx = dx.round() as i32;
    Px { side: p.side, x: if p.side == Side::Left { p.x + dx } else { p.x - dx }, y: p.y + dy.round() as i32 }
}

/// One axis: a target within `thr` wins (nearest), otherwise round to the grid.
pub fn snap_axis(v: i32, targets: &[i32], grid: i32, thr: i32) -> i32 {
    match targets.iter().copied().filter(|t| (t - v).abs() <= thr).min_by_key(|t| (t - v).abs()) {
        Some(t) => t,
        None => ((v as f32 / grid as f32).round() as i32) * grid,
    }
}

/// Grid + screen-edge + default-margin snap. `edge`/`top` are the default margins in output px.
/// ponytail: no snapping to the other cards' edges (separate processes); read their
/// widgets.conf entries and add them to the targets if that is wanted.
pub fn snap(p: Px, d: Dims, grid: i32, edge: i32, top: i32) -> Px {
    let grid = grid.max(1);
    let thr = grid + grid / 2;
    let x = snap_axis(p.x, &[0, edge], grid, thr);
    let y = snap_axis(p.y, &[0, top, d.out_h - d.h], grid, thr);
    clamp(Px { side: p.side, x, y }, d)
}

/// One drag event: apply the delta, flip the anchor at the midpoint, snap, clamp. Flip happens
/// before snapping so the snap grid is measured from the edge the card now hangs on.
pub fn drag_step(p: Px, dx: f32, dy: f32, d: Dims, grid: i32, edge: i32, top: i32) -> Px {
    let raw = clamp(drag_by(p, dx, dy), d);
    snap(flip_side(raw, d), d, grid, edge, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARDS: [&str; 3] = ["clock", "calendar", "media"];
    const D: Dims = Dims { w: 360, h: 200, out_w: 1920, out_h: 1080 };

    fn px(side: Side, x: i32, y: i32) -> Px {
        Px { side, x, y }
    }

    #[test]
    fn parse_sections_dotted_comments_and_inline() {
        let t = "## header\n[clock]\nanchor = RIGHT  # why\nx = 10.5\n# y = 99\n\ncalendar.y = 300\n[media]\nvisible = no\n";
        let (m, w) = parse(t, &CARDS);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(m["clock"], Over { side: Some(Side::Right), x: Some(10.5), y: None, visible: None });
        assert_eq!(m["calendar"].y, Some(300.0));
        assert_eq!(m["media"].visible, Some(false));
    }

    #[test]
    fn parse_unknown_and_bad_values_warn_and_keep_defaults() {
        let t = "[clock]\nx = abc\ny = NaN\nanchor = middle\nvisible = maybe\ncolor = red\n[ghost]\nx = 1\nstray line\ntop = 3\n";
        let (m, w) = parse(t, &CARDS);
        assert_eq!(m.get("clock").copied().unwrap_or_default(), Over::default());
        assert_eq!(w.len(), 8, "{w:?}");
        assert!(!m.contains_key("ghost"));
    }

    #[test]
    fn numbers_are_clamped_and_last_line_wins() {
        let (m, _) = parse("[clock]\nx = -50\ny = 99999\n[clock]\nx = 7\n", &CARDS);
        assert_eq!(m["clock"].x, Some(7.0));
        assert_eq!(m["clock"].y, Some(MAX_OFFSET));
        let (m, _) = parse("clock.x = -5\n", &CARDS);
        assert_eq!(m["clock"].x, Some(0.0));
    }

    #[test]
    fn missing_card_or_key_means_unset() {
        let (m, _) = parse("[clock]\nx = 100\n", &CARDS);
        assert_eq!(m["clock"], Over { x: Some(100.0), ..Over::default() });
        assert!(!m.contains_key("media"));
    }

    #[test]
    fn set_keys_roundtrip_on_empty_text() {
        let p = Place { side: Side::Right, x: 12.5, y: 300.0, visible: false };
        let t = set_keys("", "media", &place_kvs(&p));
        assert_eq!(t, "[media]\nanchor = right\nx = 12.5\ny = 300\nvisible = false\n");
        let (m, w) = parse(&t, &CARDS);
        assert!(w.is_empty());
        let o = m["media"];
        assert_eq!(Place { side: o.side.unwrap(), x: o.x.unwrap(), y: o.y.unwrap(), visible: o.visible.unwrap() }, p);
    }

    #[test]
    fn set_keys_keeps_comments_other_lines_and_unknown_stuff() {
        let t = "## mine\n[clock]\nanchor = left  # keep me\nx = 1\nweird = yes\n\n[media]\nx = 5\ny = 6\ncalendar.x = 9\ngarbage\n";
        let p = Place { side: Side::Right, x: 20.0, y: 30.0, visible: true };
        let out = set_keys(t, "clock", &place_kvs(&p));
        assert!(out.starts_with("## mine\n[clock]\nanchor = right  # keep me\nx = 20\nweird = yes\ny = 30\nvisible = true\n\n[media]\n"));
        assert!(out.ends_with("[media]\nx = 5\ny = 6\ncalendar.x = 9\ngarbage\n"));
        // editing media leaves clock alone and a dotted line for another card is not a media key
        let out2 = set_keys(&out, "calendar", &[("x", "77".into())]);
        assert!(out2.contains("calendar.x = 77\n") && out2.contains("x = 20\n"));
        let (m, _) = parse(&out2, &CARDS);
        assert_eq!(m["calendar"].x, Some(77.0));
        assert_eq!(m["clock"].x, Some(20.0));
    }

    #[test]
    fn set_keys_adds_missing_key_and_missing_final_newline() {
        let out = set_keys("[clock]\nx = 1", "clock", &[("y", "2".into())]);
        assert_eq!(out, "[clock]\nx = 1\ny = 2\n");
        let out = set_keys("# only a comment", "clock", &[("x", "3".into())]);
        assert_eq!(out, "# only a comment\n\n[clock]\nx = 3\n");
        // CRLF files keep CRLF on replaced lines
        assert_eq!(set_keys("[clock]\r\nx = 1\r\n", "clock", &[("x", "2".into())]), "[clock]\r\nx = 2\r\n");
    }

    #[test]
    fn number_format() {
        assert_eq!(fmt_num(48.0), "48");
        assert_eq!(fmt_num(12.5), "12.5");
        assert_eq!(fmt_num(0.004), "0");
        assert_eq!(fmt_num(-3.0), "0");
    }

    #[test]
    fn ref_px_roundtrip_at_every_scale() {
        for s in [0.5, 0.7114, 1.0, 1.25, 1.5] {
            for v in 0..2000 {
                let p = px(Side::Left, v, v / 2 + 3);
                assert_eq!(to_px(&Place { side: Side::Left, x: to_ref(p, s).0, y: to_ref(p, s).1, visible: true }, s), p, "scale {s} v {v}");
                // through the file text too
                let (x, y) = to_ref(p, s);
                let (x2, y2) = (fmt_num(x).parse::<f32>().unwrap(), fmt_num(y).parse::<f32>().unwrap());
                assert_eq!(to_px(&Place { side: Side::Left, x: x2, y: y2, visible: true }, s), p);
            }
        }
        assert_eq!(to_px(&Place { side: Side::Right, x: 48.0, y: 48.0, visible: true }, 0.7114), px(Side::Right, 34, 34));
    }

    #[test]
    fn clamp_keeps_card_on_screen() {
        assert_eq!(clamp(px(Side::Left, -20, -5), D), px(Side::Left, 0, 0));
        assert_eq!(clamp(px(Side::Left, 5000, 5000), D), px(Side::Left, 1560, 880));
        assert_eq!(clamp(px(Side::Right, 1561, 881), D), px(Side::Right, 1560, 880));
    }

    #[test]
    fn drag_delta_direction_depends_on_anchor() {
        assert_eq!(drag_by(px(Side::Left, 100, 100), 10.0, -4.0), px(Side::Left, 110, 96));
        assert_eq!(drag_by(px(Side::Right, 100, 100), 10.0, -4.0), px(Side::Right, 90, 96));
    }

    #[test]
    fn flip_converts_margin_without_moving_the_card() {
        // left card at x=900 (centre 1080 > 960) -> right anchor, margin 1920-360-900 = 660
        let r = flip_side(px(Side::Left, 900, 50), D);
        assert_eq!(r, px(Side::Right, 660, 50));
        assert_eq!(left_edge(r, D), 900);
        // and back
        let l = flip_side(px(Side::Right, 700, 50), D); // left edge 860, centre 1040 -> stays right
        assert_eq!(l.side, Side::Right);
        let l = flip_side(px(Side::Right, 900, 50), D); // left edge 660, centre 840 < 960 -> left
        assert_eq!(l, px(Side::Left, 660, 50));
        // exactly on the midpoint: no flip, either way
        assert_eq!(flip_side(px(Side::Left, 780, 0), D).side, Side::Left);
        assert_eq!(flip_side(px(Side::Right, 780, 0), D).side, Side::Right);
    }

    #[test]
    fn snap_to_grid_edges_and_defaults() {
        assert_eq!(snap_axis(203, &[], 8, 12), 200);
        assert_eq!(snap_axis(205, &[], 8, 12), 208);
        assert_eq!(snap_axis(5, &[0, 48], 8, 12), 0);
        assert_eq!(snap_axis(44, &[0, 48], 8, 12), 48);
        assert_eq!(snap_axis(30, &[0, 48], 8, 12), 32); // nothing near: grid
        let s = snap(px(Side::Left, 52, 880 - 5), D, 8, 48, 48);
        assert_eq!(s, px(Side::Left, 48, 880)); // default EDGE, bottom edge
        let s = snap(px(Side::Right, 3, 45), D, 8, 48, 48);
        assert_eq!(s, px(Side::Right, 0, 48));
    }

    #[test]
    fn drag_step_end_to_end() {
        // dragging a left card right across the midpoint flips it and keeps it put on screen
        let p = px(Side::Left, 752, 48);
        let before = left_edge(p, D);
        let q = drag_step(p, 40.0, 0.0, D, 8, 48, 48);
        assert_eq!(q.side, Side::Right);
        assert!((left_edge(q, D) - (before + 40)).abs() <= 8);
        // dragging far off-screen stays on screen (a right card dragged to the far left becomes a left one)
        let q = drag_step(px(Side::Right, 100, 100), -9999.0, 9999.0, D, 8, 48, 48);
        assert_eq!(q, px(Side::Left, 0, 880));
        let q = drag_step(px(Side::Left, 100, 100), -9999.0, -9999.0, D, 8, 48, 48);
        assert_eq!(q, px(Side::Left, 0, 0));
        // no movement keeps a snapped card where it is (no drift)
        assert_eq!(drag_step(px(Side::Left, 48, 48), 0.0, 0.0, D, 8, 48, 48), px(Side::Left, 48, 48));
    }
}
