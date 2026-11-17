// SPDX-License-Identifier: GPL-3.0-or-later
//! Idle pet for the Island: a pure state machine (`Pet::step`, one call per animation
//! tick) plus the drawn gator. The Frakassets animals (see `sprites.rs`) reuse the same
//! state machine with a different `Cfg`; the gator's sprites are the const rows below.
use cosmic::iced::{mouse, Color, Point, Rectangle, Size};
use cosmic::widget::canvas::{self, Frame, Geometry};

use crate::conf::Conf;

/// Default continuous idle ticks (~60s at 10 fps) before the pet lies down.
pub const SLEEP_AFTER: u32 = 600;
/// Canvas grid in sprite pixels (sprite is 16 wide, hop/z marks need headroom).
pub const GRID_W: u8 = 21;
pub const GRID_H: u8 = 10;
const SPR_W: u8 = 16;
#[cfg(test)]
pub const SPR_W_GRID: u8 = SPR_W;
#[cfg(test)]
const X_MAX: u8 = GRID_W - SPR_W;

// '.' empty, o outline, g body, l belly/highlight, d dark scute, e eye. Faces right.
const STAND: [&str; 8] = [
    "........ooo.....",
    ".......ogeggo...",
    "..d.d.ogggggggoo",
    ".ooooooggggggggo",
    "oggggggggggllllo",
    ".ogggggggggoooo.",
    "..ollllllllo....",
    "..gg.gg..gg.gg..",
];
const WALK1: [&str; 8] = [
    "........ooo.....",
    ".......ogeggo...",
    "..d.d.ogggggggoo",
    ".ooooooggggggggo",
    "oggggggggggllllo",
    ".ogggggggggoooo.",
    "..ollllllllo....",
    ".gg..gg...gg..gg",
];
const WALK2: [&str; 8] = [
    "........ooo.....",
    ".......ogeggo...",
    "..d.d.ogggggggoo",
    ".ooooooggggggggo",
    "oggggggggggllllo",
    ".ogggggggggoooo.",
    "..ollllllllo....",
    "...gg.gg.gg.gg..",
];
const SLEEP: [&str; 5] = [
    "...d.d.d..ooo...",
    ".ooooooooggggggo",
    "ogggggggggoggggo",
    ".olllllllllllloo",
    "..oooooooooooo..",
];
const Z: [&str; 3] = ["zzz", ".z.", "zzz"];

fn color(c: u8) -> Option<Color> {
    let rgb = |r: u8, g: u8, b: u8| Some(Color::from_rgb8(r, g, b));
    match c {
        b'o' => rgb(0x0c, 0x15, 0x14),
        b'g' => rgb(0x1f, 0x8a, 0x7a),
        b'l' | b'z' => rgb(0x4c, 0xc2, 0xae),
        b'd' => rgb(0x16, 0x64, 0x56),
        b'e' => rgb(0xa3, 0xe6, 0x35),
        _ => None,
    }
}

/// Per-look tuning of the state machine: how far it may walk, how often it moves, how long a hop lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cfg {
    /// Rightmost x (inclusive), in px.
    pub x_max: u8,
    /// Move `move_px` px every this many walk ticks.
    pub move_every: u32,
    pub move_px: u8,
    /// Ticks one hop (the `Happy` act) lasts.
    pub hop_ticks: u32,
    /// Idle ticks before lying down; 0 = never.
    pub sleep_after: u32,
    /// Chance in percent that a rest ends in a hop (the rest of the 60..100 band stays rest).
    pub hop_pct: u32,
}

impl Cfg {
    /// This look with the user's island.conf speed / hop / sleep settings applied.
    pub fn tune(mut self, c: &Conf) -> Cfg {
        use crate::conf::{Hops, Speed};
        (self.move_every, self.move_px) = match (c.walk_speed, self.move_every) {
            (Speed::Slow, e) => (e * 2, 1),
            (Speed::Normal, e) => (e, 1),
            (Speed::Fast, 1) => (1, 2),
            (Speed::Fast, e) => (e - 1, 1),
        };
        self.hop_pct = match c.hop_frequency {
            Hops::Never => 0,
            Hops::Rare => 5,
            Hops::Normal => 15,
            Hops::Often => 30,
        };
        self.sleep_after = c.sleep_after_seconds * c.animation_fps;
        self
    }
}

/// The drawn gator on a canvas `grid_w` sprite pixels wide.
pub const fn gator(grid_w: u8) -> Cfg {
    Cfg { x_max: grid_w - SPR_W, move_every: 3, move_px: 1, hop_ticks: 4, sleep_after: SLEEP_AFTER, hop_pct: 15 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Stand,
    Walk,
    Hop,
    Sleep,
}

#[derive(Clone, Debug)]
pub struct Pet {
    pub act: Act,
    /// Sprite left edge in grid pixels, 0..=X_MAX.
    pub x: u8,
    pub right: bool,
    cfg: Cfg,
    /// Ticks since the current act began.
    pub age: u32,
    /// Ticks until the current act ends (unused while sleeping).
    left: u32,
    /// Continuous idle ticks.
    idle: u32,
    blink_in: u32,
    blink: u32,
    rng: u32,
}

impl Pet {
    #[cfg(test)]
    pub fn new(seed: u32) -> Self {
        Self::with(seed, gator(GRID_W))
    }

    pub fn with(seed: u32, cfg: Cfg) -> Self {
        let mut p = Pet { act: Act::Stand, x: cfg.x_max / 2, right: true, cfg, age: 0, left: 0, idle: 0, blink_in: 0, blink: 0, rng: seed | 1 };
        p.left = p.roll(10, 40);
        p.blink_in = p.roll(30, 70);
        p
    }

    /// A fresh pet of another range / tuning with its own random sequence (the popup's copy of the pill pet).
    pub fn twin(&self, cfg: Cfg) -> Pet {
        Pet::with(self.rng.rotate_left(13) ^ 0x9e37_79b9, cfg)
    }

    /// A click: wake up if asleep, then hop.
    pub fn poke(&mut self) {
        if self.act == Act::Sleep {
            self.reset();
        }
        self.set(Act::Hop, self.cfg.hop_ticks);
    }

    /// Back to a fresh, awake gator (called when a higher-priority state shows).
    pub fn reset(&mut self) {
        *self = Pet::with(self.rng, self.cfg);
    }

    fn roll(&mut self, lo: u32, hi: u32) -> u32 {
        // xorshift32
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        lo + self.rng % (hi - lo + 1)
    }

    fn set(&mut self, act: Act, left: u32) {
        self.act = act;
        self.age = 0;
        self.left = left;
    }

    /// Advance one idle tick.
    pub fn step(&mut self) {
        self.idle += 1;
        self.age += 1;
        if self.act == Act::Sleep {
            return;
        }
        if self.cfg.sleep_after > 0 && self.idle >= self.cfg.sleep_after {
            self.set(Act::Sleep, 0);
            return;
        }
        if self.blink > 0 {
            self.blink -= 1;
        } else if self.blink_in > 0 {
            self.blink_in -= 1;
        } else {
            self.blink = 2;
            self.blink_in = self.roll(30, 70);
        }
        if self.act == Act::Walk && self.age % self.cfg.move_every == 0 {
            if self.right && self.x < self.cfg.x_max {
                self.x = self.x.saturating_add(self.cfg.move_px).min(self.cfg.x_max);
            } else if !self.right && self.x > 0 {
                self.x = self.x.saturating_sub(self.cfg.move_px);
            } else {
                self.right = !self.right; // turn at the edge
            }
        }
        self.left = self.left.saturating_sub(1);
        if self.left == 0 {
            self.next();
        }
    }

    fn next(&mut self) {
        let r = self.roll(0, 99);
        if self.act == Act::Stand && r < 60 {
            let n = self.roll(15, 45);
            self.set(Act::Walk, n);
        } else if self.act == Act::Stand && r < 60 + self.cfg.hop_pct {
            self.set(Act::Hop, self.cfg.hop_ticks);
        } else {
            let n = self.roll(10, 40);
            self.set(Act::Stand, n);
        }
    }

    fn lift(&self) -> u8 {
        u8::from(self.act == Act::Hop && (1..=2).contains(&self.age))
    }

    fn z_phase(&self) -> u32 {
        (self.age / 4) % 3
    }

    fn blinking(&self) -> bool {
        self.blink > 0
    }

    fn rows(&self) -> &'static [&'static str] {
        match self.act {
            Act::Sleep => &SLEEP,
            Act::Walk if (self.age / 3) % 2 == 0 => &WALK1,
            Act::Walk => &WALK2,
            _ => &STAND,
        }
    }
}

/// Integer pixel scale for a given text size (keeps the pill about as tall as the old text pill).
pub fn scale(text_px: f32) -> f32 {
    if text_px * 1.4 + 4.0 >= 29.0 { 3.0 } else { 2.0 }
}

/// Fill `rows` at grid offset (`ox`, `oy`), mirrored inside its own width when `flip`.
/// Horizontal runs of one colour are merged into a single rect.
fn blit(f: &mut Frame, s: f32, rows: &[&str], ox: i32, oy: i32, flip: bool, eye: u8) {
    for (y, row) in rows.iter().enumerate() {
        let mut b: Vec<u8> = row.bytes().map(|c| if c == b'e' { eye } else { c }).collect();
        if flip {
            b.reverse();
        }
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            let mut j = i;
            while j < b.len() && b[j] == c {
                j += 1;
            }
            if let Some(col) = color(c) {
                f.fill_rectangle(Point::new((ox + i as i32) as f32 * s, (oy + y as i32) as f32 * s), Size::new((j - i) as f32 * s, s), col);
            }
            i = j;
        }
    }
}

impl<M> canvas::Program<M, cosmic::Theme> for Pet {
    type State = ();

    fn draw(&self, _: &(), renderer: &cosmic::Renderer, _: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let s = (bounds.width / f32::from(self.cfg.x_max + SPR_W)).floor().max(1.0);
        let mut f = Frame::new(renderer, bounds.size());
        let (ox, flip) = (i32::from(self.x), !self.right);
        if self.act == Act::Sleep {
            blit(&mut f, s, &SLEEP, ox, i32::from(GRID_H) - SLEEP.len() as i32, flip, b'o');
            for k in 0..2 {
                let ph = (self.z_phase() + k) % 3;
                let lc = 9 + 3 * k as i32; // z column inside the sprite box, near the head
                let zx = if flip { SPR_W as i32 - 3 - lc } else { lc };
                blit(&mut f, s, &Z, ox + zx, 2 - ph as i32, false, b'e');
            }
        } else {
            let eye = if self.blinking() { b'o' } else { b'e' };
            blit(&mut f, s, self.rows(), ox, i32::from(GRID_H) - 8 - i32::from(self.lift()), flip, eye);
        }
        vec![f.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprites_are_well_formed() {
        for r in [&STAND[..], &WALK1, &WALK2, &SLEEP] {
            assert!(r.iter().all(|l| l.len() == usize::from(SPR_W) && l.bytes().all(|c| c == b'.' || color(c).is_some())));
        }
        assert!(Z.iter().all(|l| l.len() == 3));
    }

    fn walking(x: u8, right: bool) -> Pet {
        let mut p = Pet::new(1);
        p.x = x;
        p.right = right;
        p.set(Act::Walk, 1000);
        p
    }

    #[test]
    fn walk_moves_one_pixel_per_three_ticks() {
        let mut p = walking(2, true);
        for _ in 0..3 {
            p.step();
        }
        assert_eq!(p.x, 3);
    }

    #[test]
    fn walk_turns_at_both_edges_and_stays_in_bounds() {
        let mut p = walking(X_MAX, true);
        for _ in 0..3 {
            p.step();
        }
        assert!(!p.right && p.x == X_MAX);
        let mut p = walking(0, false);
        for _ in 0..3 {
            p.step();
        }
        assert!(p.right && p.x == 0);
        let mut p = walking(0, true);
        for _ in 0..3000 {
            p.step();
            assert!(p.x <= X_MAX);
            if p.idle >= SLEEP_AFTER - 1 {
                break;
            }
        }
    }

    #[test]
    fn sleeps_after_n_idle_ticks_and_stays_asleep() {
        let mut p = Pet::new(7);
        for _ in 0..SLEEP_AFTER - 1 {
            p.step();
            assert_ne!(p.act, Act::Sleep);
        }
        p.step();
        assert_eq!(p.act, Act::Sleep);
        for _ in 0..5000 {
            p.step();
        }
        assert_eq!(p.act, Act::Sleep);
    }

    #[test]
    fn reset_wakes_and_restarts_idle_clock() {
        let mut p = Pet::new(7);
        for _ in 0..SLEEP_AFTER {
            p.step();
        }
        assert_eq!(p.act, Act::Sleep);
        p.reset();
        assert_eq!((p.act, p.idle, p.age), (Act::Stand, 0, 0));
        for _ in 0..SLEEP_AFTER - 1 {
            p.step();
        }
        assert_ne!(p.act, Act::Sleep);
    }

    #[test]
    fn deterministic_for_a_seed_and_varied_across_acts() {
        let run = |seed| {
            let mut p = Pet::new(seed);
            (0..500).map(|_| { p.step(); (p.act, p.x, p.right) }).collect::<Vec<_>>()
        };
        assert_eq!(run(42), run(42));
        let acts: Vec<_> = (1..=5).flat_map(run).collect();
        for a in [Act::Stand, Act::Walk, Act::Hop] {
            assert!(acts.iter().any(|s| s.0 == a), "{a:?} never happened");
        }
    }

    #[test]
    fn blinks_every_few_seconds() {
        let mut p = Pet::new(3);
        let blinks = (0..SLEEP_AFTER - 1).filter(|_| { p.step(); p.blinking() }).count();
        assert!(blinks >= 10, "{blinks}");
    }

    #[test]
    fn twin_has_same_range_but_its_own_sequence() {
        let cfg = Cfg { x_max: 22, move_every: 1, move_px: 1, hop_ticks: 8, sleep_after: SLEEP_AFTER, hop_pct: 15 };
        let a = Pet::with(5, cfg);
        let mut t = a.twin(cfg);
        assert_eq!((t.x, t.cfg), (11, cfg));
        let mut a = a;
        let run = |p: &mut Pet| (0..300).map(|_| { p.step(); (p.act, p.x) }).collect::<Vec<_>>();
        assert_ne!(run(&mut a), run(&mut t));
    }

    #[test]
    fn hop_lifts_for_two_ticks_only() {
        let mut p = Pet::new(1);
        p.set(Act::Hop, 4);
        let lifts: Vec<u8> = (0..4).map(|_| { p.step(); p.lift() }).collect();
        assert_eq!(lifts, [1, 1, 0, 0]);
        assert_eq!(p.act, Act::Stand);
    }

    #[test]
    fn sprite_cfg_stays_in_bounds_and_hops_for_its_length() {
        let cfg = Cfg { x_max: 22, move_every: 1, move_px: 1, hop_ticks: 8, sleep_after: SLEEP_AFTER, hop_pct: 15 };
        let mut p = Pet::with(9, cfg);
        let mut hops = 0;
        for _ in 0..SLEEP_AFTER - 1 {
            p.step();
            assert!(p.x <= cfg.x_max);
            if p.act == Act::Hop {
                hops += 1;
            }
        }
        assert!(hops >= 8, "{hops}");
        p.reset();
        assert_eq!((p.act, p.x), (Act::Stand, 11));
        p.set(Act::Walk, 1000);
        p.step();
        assert_eq!(p.x, 12); // one px per tick
    }

    #[test]
    fn poke_hops_and_wakes_a_sleeper() {
        let mut p = Pet::new(7);
        p.poke();
        assert_eq!(p.act, Act::Hop);
        for _ in 0..SLEEP_AFTER {
            p.step();
        }
        assert_eq!(p.act, Act::Sleep);
        p.poke();
        assert_eq!((p.act, p.age, p.idle), (Act::Hop, 0, 0));
    }

    #[test]
    fn tune_maps_conf_to_cfg() {
        use crate::conf::{Hops, Speed};
        let base = Conf::default();
        assert_eq!(gator(GRID_W).tune(&base), gator(GRID_W)); // defaults change nothing
        let c = Conf { walk_speed: Speed::Fast, hop_frequency: Hops::Never, sleep_after_seconds: 5, animation_fps: 15, ..base.clone() };
        let g = gator(GRID_W).tune(&c);
        assert_eq!((g.move_every, g.move_px, g.hop_pct, g.sleep_after), (2, 1, 0, 75));
        let a = Cfg { move_every: 1, ..gator(GRID_W) };
        assert_eq!((a.tune(&c).move_every, a.tune(&c).move_px), (1, 2)); // 1 tick is the floor: fast = 2 px
        assert_eq!(a.tune(&Conf { walk_speed: Speed::Slow, ..base }).move_every, 2);
    }

    #[test]
    fn zero_sleep_never_sleeps_and_never_hop_never_hops() {
        let cfg = Cfg { sleep_after: 0, hop_pct: 0, ..gator(GRID_W) };
        let mut p = Pet::with(3, cfg);
        for _ in 0..20_000 {
            p.step();
            assert!(p.act != Act::Sleep && p.act != Act::Hop);
        }
        let mut p = Pet::with(3, Cfg { hop_pct: 30, ..cfg });
        assert!((0..5000).any(|_| { p.step(); p.act == Act::Hop }));
    }

    #[test]
    fn fast_walk_stays_in_bounds() {
        let mut p = Pet::with(1, Cfg { move_px: 2, move_every: 1, x_max: 5, ..gator(GRID_W) });
        p.x = 4;
        p.set(Act::Walk, 1000);
        p.step();
        assert_eq!(p.x, 5);
    }
}
