// SPDX-License-Identifier: GPL-3.0-or-later
//! Frakassets pet sprites (CC BY 4.0, see assets/pets/LICENSE.txt) for the idle Island.
//! Pure parts: choosing the pet (`resolve`), tag -> frame lookup (`Layout`), shine-over-lit
//! compositing (`over`). Sheets are embedded, decoded once at start into per-frame runs of
//! one colour, and drawn as nearest-neighbour rectangles (no filtering, so no blur).
use cosmic::iced::{mouse, Color, Point, Rectangle, Size};
use cosmic::widget::canvas::{self, Frame, Geometry};

use crate::pet::{Act, Cfg, Pet};

/// Sprite cell edge in px.
pub const CELL: i32 = 32;
/// Canvas width at 1x in the pill; the sprite walks inside it (cells have ~3px of empty margin each side).
pub const CANVAS_W: u8 = 48;
/// Canvas height at 1x: the pill height the gator pill had (cell bottom-aligned, top 2px of a hop clipped).
pub const CANVAS_H: u8 = 30;
const MARGIN: u8 = 3;

pub struct Animal {
    pub word: &'static str,
    json: &'static str,
    lit: &'static [u8],
    shine: &'static [u8],
}

macro_rules! animal {
    ($w:literal, $d:literal) => {
        Animal {
            word: $w,
            json: include_str!(concat!("../assets/pets/", $d, "/", $d, "_32px_scroller.json")),
            lit: include_bytes!(concat!("../assets/pets/", $d, "/", $d, "_32px_scroller_outline_lit.png")),
            shine: include_bytes!(concat!("../assets/pets/", $d, "/", $d, "_32px_scroller_outline_shine.png")),
        }
    };
}

pub const ANIMALS: [Animal; 10] = [
    animal!("cat", "cat-orange"),
    animal!("puppy", "puppy-shiba"),
    animal!("fox", "fox-red"),
    animal!("bunny", "bunny-white"),
    animal!("hamster", "hamster-golden"),
    animal!("hedgehog", "hedgehog-brown"),
    animal!("piglet", "piglet-pink"),
    animal!("duckling", "duckling-yellow"),
    animal!("frog", "frog-green"),
    animal!("turtle", "turtle-green"),
];

/// What the user asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Off,
    Gator,
    Random,
    Animal(usize),
}

/// Nothing set, or an unknown word.
pub const DEFAULT: Choice = Choice::Random;

fn parse(word: &str) -> Option<Choice> {
    let w = word.trim().lines().next().unwrap_or("").trim().to_lowercase();
    let w = w.strip_prefix("glasskit_pet=").unwrap_or(&w).trim(); // tolerate a legacy file holding `GLASSKIT_PET=turtle`
    Some(match w {
        "0" | "false" | "off" => Choice::Off,
        "gator" => Choice::Gator,
        "random" => Choice::Random,
        _ => Choice::Animal(ANIMALS.iter().position(|a| a.word == w)?),
    })
}

/// `GLASSKIT_PET` (env) beats island.conf's `pet`, which beats the legacy `~/.config/glasskit/pet`
/// file, which beats the default; a value that does not parse is skipped.
pub fn resolve(env: Option<&str>, conf: Option<&str>, legacy: Option<&str>) -> Choice {
    env.and_then(parse).or_else(|| conf.and_then(parse)).or_else(|| legacy.and_then(parse)).unwrap_or(DEFAULT)
}

pub fn choice(conf: &crate::conf::Conf) -> Choice {
    let legacy = crate::conf::dir().and_then(|d| std::fs::read_to_string(d.join("pet")).ok());
    resolve(std::env::var("GLASSKIT_PET").ok().as_deref(), conf.pet.as_deref(), legacy.as_deref())
}

/// Frame tags and frame origins of an Aseprite JSON hash sheet (`frames` array + `meta.frameTags`).
#[derive(Debug, PartialEq, Eq)]
pub struct Layout {
    tags: Vec<(String, usize, usize)>,
    /// Top-left of each frame in the sheet.
    origins: Vec<(u32, u32)>,
}

impl Layout {
    pub fn parse(json: &str) -> Option<Layout> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let origins = v["frames"].as_array()?.iter().map(|f| Some((f["frame"]["x"].as_u64()? as u32, f["frame"]["y"].as_u64()? as u32))).collect::<Option<Vec<_>>>()?;
        let tags = v["meta"]["frameTags"].as_array()?.iter().map(|t| Some((t["name"].as_str()?.to_string(), t["from"].as_u64()? as usize, t["to"].as_u64()? as usize))).collect::<Option<Vec<_>>>()?;
        Some(Layout { tags, origins })
    }

    /// Frame origins of a tag such as `Walk_East`, in play order.
    pub fn frames(&self, tag: &str) -> Option<&[(u32, u32)]> {
        let (_, from, to) = self.tags.iter().find(|t| t.0 == tag)?;
        self.origins.get(*from..=*to)
    }
}

struct Img {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
}

fn decode(bytes: &[u8]) -> Option<Img> {
    let mut d = png::Decoder::new(bytes);
    d.set_transformations(png::Transformations::EXPAND);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let i = r.next_frame(&mut buf).ok()?;
    buf.truncate(i.buffer_size());
    let rgba = match i.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => return None,
    };
    Some(Img { w: i.width as usize, h: i.height as usize, rgba })
}

/// `src` over `dst`, straight (non-premultiplied) RGBA.
pub fn over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let (sa, da) = (f32::from(src[3]) / 255.0, f32::from(dst[3]) / 255.0);
    let oa = sa + da * (1.0 - sa);
    if oa <= 0.0 {
        return [0; 4];
    }
    let c = |i: usize| ((f32::from(src[i]) * sa + f32::from(dst[i]) * da * (1.0 - sa)) / oa).round() as u8;
    [c(0), c(1), c(2), (oa * 255.0).round() as u8]
}

/// A horizontal run of one colour inside a cell.
#[derive(Clone, Copy)]
struct Run {
    x: u8,
    y: u8,
    w: u8,
    col: Color,
}

/// One cell of `lit` with `shine` composited over it, as runs.
fn runs(lit: &Img, shine: &Img, (ox, oy): (u32, u32)) -> Vec<Run> {
    let px = |im: &Img, x: usize, y: usize| -> [u8; 4] {
        let i = ((oy as usize + y) * im.w + ox as usize + x) * 4;
        if oy as usize + y >= im.h || ox as usize + x >= im.w { [0; 4] } else { im.rgba[i..i + 4].try_into().unwrap() }
    };
    let mut out = Vec::new();
    for y in 0..CELL as usize {
        let row: Vec<[u8; 4]> = (0..CELL as usize).map(|x| over(px(lit, x, y), px(shine, x, y))).collect();
        let mut x = 0;
        while x < row.len() {
            let c = row[x];
            let n = row[x..].iter().take_while(|p| **p == c).count();
            if c[3] > 0 {
                out.push(Run { x: x as u8, y: y as u8, w: n as u8, col: Color::from_rgba8(c[0], c[1], c[2], f32::from(c[3]) / 255.0) });
            }
            x += n;
        }
    }
    out
}

const KINDS: [&str; 4] = ["Idle", "Walk", "Sleep", "Happy"];

fn kind(a: Act) -> usize {
    match a {
        Act::Stand => 0,
        Act::Walk => 1,
        Act::Sleep => 2,
        Act::Hop => 3,
    }
}

/// Which frame of a looping animation of `len` frames shows at `age` ticks (one frame per tick).
pub fn frame_index(age: u32, len: usize) -> usize {
    age as usize % len.max(1)
}

pub struct Sprites {
    /// [kind][0 = East, 1 = West][frame]
    anims: Vec<[Vec<Vec<Run>>; 2]>,
}

impl Sprites {
    pub fn load(a: &Animal) -> Option<Sprites> {
        let layout = Layout::parse(a.json)?;
        let (lit, shine) = (decode(a.lit)?, decode(a.shine)?);
        let side = |k: &str, s: &str| -> Option<Vec<Vec<Run>>> {
            Some(layout.frames(&format!("{k}_{s}"))?.iter().map(|o| runs(&lit, &shine, *o)).collect())
        };
        let anims = KINDS.iter().map(|k| Some([side(k, "East")?, side(k, "West")?])).collect::<Option<Vec<_>>>()?;
        Some(Sprites { anims })
    }

    /// Ticks one hop lasts: the whole `Happy` loop once.
    pub fn hop_ticks(&self) -> u32 {
        self.anims[3][0].len() as u32
    }

    /// State-machine range for a canvas `canvas_w` sprite px wide (pill 48, popup wider).
    pub fn cfg(&self, canvas_w: u8) -> Cfg {
        Cfg { x_max: canvas_w - CELL as u8 + 2 * MARGIN, move_every: 1, move_px: 1, hop_ticks: self.hop_ticks(), sleep_after: crate::pet::SLEEP_AFTER, hop_pct: 15 }
    }

    fn frame(&self, act: Act, right: bool, age: u32) -> &[Run] {
        let f = &self.anims[kind(act)][usize::from(!right)];
        &f[frame_index(age, f.len())]
    }
}

/// Draws a `Pet` with `Sprites` at integer scale `k`.
pub struct View<'a> {
    pub pet: &'a Pet,
    pub sprites: &'a Sprites,
    pub k: f32,
}

impl<M> canvas::Program<M, cosmic::Theme> for View<'_> {
    type State = ();

    fn draw(&self, _: &(), renderer: &cosmic::Renderer, _: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let k = self.k;
        let ox = (i32::from(self.pet.x) - i32::from(MARGIN)) as f32;
        let oy = f32::from(CANVAS_H) - CELL as f32; // bottom-aligned
        for r in self.sprites.frame(self.pet.act, self.pet.right, self.pet.age) {
            f.fill_rectangle(Point::new((ox + f32::from(r.x)) * k, (oy + f32::from(r.y)) * k), Size::new(f32::from(r.w) * k, k), r.col);
        }
        vec![f.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_words() {
        for (i, a) in ANIMALS.iter().enumerate() {
            assert_eq!(parse(a.word), Some(Choice::Animal(i)));
            assert_eq!(parse(&format!(" {}\n", a.word.to_uppercase())), Some(Choice::Animal(i)));
        }
        assert_eq!(parse("gator"), Some(Choice::Gator));
        assert_eq!(parse("random"), Some(Choice::Random));
        for w in ["off", "0", "false"] {
            assert_eq!(parse(w), Some(Choice::Off));
        }
        assert_eq!(parse("dragon"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("frog\n# comment"), parse("frog"));
    }

    #[test]
    fn resolve_precedence_and_defaults() {
        let frog = Choice::Animal(8);
        assert_eq!(resolve(None, None, None), DEFAULT);
        assert_eq!(DEFAULT, Choice::Random);
        assert_eq!(resolve(None, None, Some("frog")), frog);
        assert_eq!(resolve(None, Some("frog"), None), frog);
        assert_eq!(resolve(Some("cat"), Some("frog"), Some("fox")), Choice::Animal(0)); // env > conf > legacy
        assert_eq!(resolve(None, Some("frog"), Some("fox")), frog);
        assert_eq!(resolve(None, None, Some("gator")), Choice::Gator);
        assert_eq!(resolve(Some("0"), Some("frog"), None), Choice::Off);
        assert_eq!(resolve(None, None, Some("off")), Choice::Off);
        assert_eq!(resolve(None, Some("nope"), Some("gator")), Choice::Gator); // unparsable conf falls to legacy
        assert_eq!(resolve(Some("nope"), None, None), DEFAULT);
        assert_eq!(resolve(Some("nope"), Some("FROG"), None), frog); // unknown env is skipped, case-insensitive
        assert_eq!(resolve(Some(""), None, Some("random")), Choice::Random);
        assert_eq!(resolve(None, None, Some("GLASSKIT_PET=turtle\n")), Choice::Animal(9)); // legacy shell-style file
    }

    const EXAMPLE: &str = include_str!("../island.conf.example");

    #[test]
    fn example_conf_is_all_defaults_and_lists_every_pet() {
        let (c, warn) = crate::conf::parse(EXAMPLE);
        assert!(warn.is_empty(), "{warn:?}");
        assert_eq!(c, crate::conf::Conf { pet: Some("random".into()), ..Default::default() });
        let names = ANIMALS.iter().map(|a| a.word).chain(["gator", "random", "off"]);
        for n in names {
            assert!(EXAMPLE.contains(&format!("\n#   {n} ")), "{n} not listed");
            assert!(parse(n).is_some(), "{n} not accepted");
            assert_eq!(resolve(None, Some(&n.to_uppercase()), None), parse(n).unwrap());
        }
    }

    const SAMPLE: &str = r#"{"frames":[
        {"frame":{"x":0,"y":0,"w":32,"h":32}},{"frame":{"x":32,"y":0,"w":32,"h":32}},
        {"frame":{"x":0,"y":32,"w":32,"h":32}},{"frame":{"x":32,"y":32,"w":32,"h":32}},{"frame":{"x":64,"y":32,"w":32,"h":32}}],
        "meta":{"frameTags":[{"name":"Idle_East","from":0,"to":1},{"name":"Walk_West","from":2,"to":4}]}}"#;

    #[test]
    fn tag_to_frames() {
        let l = Layout::parse(SAMPLE).unwrap();
        assert_eq!(l.frames("Idle_East"), Some(&[(0, 0), (32, 0)][..]));
        assert_eq!(l.frames("Walk_West"), Some(&[(0, 32), (32, 32), (64, 32)][..]));
        assert_eq!(l.frames("Walk_East"), None);
        assert!(Layout::parse("{").is_none());
        assert!(Layout::parse(r#"{"frames":[],"meta":{"frameTags":[{"name":"A","from":0,"to":3}]}}"#).unwrap().frames("A").is_none());
    }

    #[test]
    fn compositing() {
        assert_eq!(over([10, 20, 30, 255], [0, 0, 0, 0]), [10, 20, 30, 255]);
        assert_eq!(over([10, 20, 30, 255], [200, 200, 200, 255]), [200, 200, 200, 255]);
        assert_eq!(over([0; 4], [0; 4]), [0; 4]);
        assert_eq!(over([0, 0, 0, 255], [255, 255, 255, 128]), [128, 128, 128, 255]);
    }

    #[test]
    fn frames_loop() {
        assert_eq!((0..5).map(|a| frame_index(a, 3)).collect::<Vec<_>>(), [0, 1, 2, 0, 1]);
        assert_eq!(frame_index(7, 0), 0);
    }

    #[test]
    fn every_pet_folder_has_an_entry() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pets");
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.path().is_dir()) {
            let name = e.file_name().into_string().unwrap();
            assert!(ANIMALS.iter().any(|a| name.split('-').next() == Some(a.word)), "{name}");
            n += 1;
        }
        assert_eq!(n, ANIMALS.len());
    }

    #[test]
    fn every_embedded_animal_loads_with_all_tags() {
        for a in &ANIMALS {
            let s = Sprites::load(a).unwrap_or_else(|| panic!("{} failed to load", a.word));
            for k in &s.anims {
                for side in k {
                    assert!(!side.is_empty() && side.iter().any(|f| !f.is_empty()), "{}", a.word);
                }
            }
            assert!((6..=8).contains(&s.hop_ticks()), "{}", a.word);
        }
    }
}
