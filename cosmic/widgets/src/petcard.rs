// SPDX-License-Identifier: GPL-3.0-or-later
//! The `pet` card: the island's pet engine (pet.rs / sprites.rs / conf.rs, shared by path)
//! drawn big on a glass card. Pure parts: pet cycle order, in-place `pet =` line edit.
use std::path::PathBuf;

use cosmic::iced::widget::canvas::{Frame, Geometry};
use cosmic::iced::widget::stack;
use cosmic::iced::{Alignment, Color, Length, Point, Rectangle, Vector, mouse};
use cosmic::widget::{self, button, canvas, container, mouse_area};
use cosmic::{Element, Renderer, Theme};

use crate::conf::{self, Conf};
use crate::pet::{self, Act, Pet};
use crate::sprites::{self, Sprites};
use crate::style::{FONT, MUTED};
use crate::{Message, W, sc, scu};

/// 10 animals (sprites.rs order) + the drawn gator.
const COUNT: usize = sprites::ANIMALS.len() + 1;
const GATOR: usize = COUNT - 1;
/// Card padding, 1080p px.
pub const PAD_X: f32 = 10.0;
pub const PAD_Y: f32 = 6.0;

pub fn name(i: usize) -> &'static str {
    sprites::ANIMALS.get(i).map_or("gator", |a| a.word)
}

/// Previous (-1) / next (+1) pet index, wrapping around all 11.
pub fn cycle(i: usize, dir: i32) -> usize {
    (i as i32 + dir).rem_euclid(COUNT as i32) as usize
}

/// `text` with the (last, i.e. effective) `pet =` setting replaced in place, keeping its inline
/// comment and line ending; appended when there is none. Every other line is untouched.
pub fn set_pet_line(text: &str, name: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let hit = lines.iter().rposition(|l| {
        let t = l.trim_start();
        !t.starts_with('#') && t.split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case("pet"))
    });
    let Some(i) = hit else {
        let nl = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
        return format!("{text}{nl}pet = {name}\n");
    };
    let l = lines[i];
    let body = l.trim_end_matches(['\r', '\n']);
    let (eol, eq) = (&l[body.len()..], body.find('=').unwrap_or(0));
    let tail = body[eq..].find('#').map_or(String::new(), |h| format!("  {}", &body[eq + h..]));
    let new = format!("{}= {name}{tail}{eol}", &body[..eq]);
    lines.iter().enumerate().map(|(j, s)| if j == i { new.as_str() } else { s }).collect()
}

const TEMPLATE: &str = include_str!("../../island/island.conf.example");

fn conf_path() -> Option<PathBuf> {
    conf::dir().map(|d| d.join("island.conf"))
}

/// Write the choice to island.conf (created from the template when missing). Never clobbers a file it cannot read.
pub fn save_pet(name: &str) -> std::io::Result<()> {
    let path = conf_path().ok_or_else(|| std::io::Error::other("no config dir"))?;
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => TEMPLATE.to_string(),
        Err(e) => return Err(e),
    };
    std::fs::create_dir_all(path.parent().unwrap_or(&path))?;
    let tmp = path.with_extension("conf.tmp");
    std::fs::write(&tmp, set_pet_line(&text, name))?;
    std::fs::rename(tmp, path)
}

fn seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() ^ d.as_secs() as u32)
}

pub struct PetState {
    pub idx: usize,
    pub pet: Pet,
    sprites: Option<Sprites>,
    /// Screen px per sprite px (animals) / per 1/2 grid px (gator).
    k: f32,
    /// Canvas width in sprite px (animals) / gator grid px.
    cw: u8,
}

impl PetState {
    /// `idx` None = what island.conf / env / legacy file say (random animal for `random`).
    pub fn new(idx: Option<usize>, conf: &Conf) -> PetState {
        let seed = seed();
        let idx = idx.unwrap_or_else(|| match sprites::choice(conf) {
            sprites::Choice::Animal(i) => i,
            sprites::Choice::Random => seed as usize / 7 % sprites::ANIMALS.len(),
            _ => GATOR, // gator, or `off` (the card still shows something)
        });
        let k = sc(4.0).round().max(1.0);
        let inner = (scu(W) - 2 * scu(PAD_X as u32)) as f32;
        let sp = sprites::ANIMALS.get(idx).and_then(Sprites::load);
        let (idx, cfg, cw) = match &sp {
            Some(s) => {
                let cw = (inner / k).min(255.0) as u8;
                (idx, s.cfg(cw).tune(conf), cw)
            }
            None => {
                let cw = (inner / (2.0 * k)).clamp(17.0, 255.0) as u8; // gator grid px are 2k screen px
                (GATOR, pet::gator(cw).tune(conf), cw)
            }
        };
        PetState { idx, pet: Pet::with(seed, cfg), sprites: sp, k, cw }
    }

    pub fn asleep(&self) -> bool {
        self.pet.act == Act::Sleep
    }

    pub fn caption(&self) -> String {
        let n = name(self.idx);
        format!("{}{} · {}", n[..1].to_uppercase(), &n[1..], if self.asleep() { "sleeping" } else { "awake" })
    }

    /// One animation tick; a sleeper advances a few ticks at once so its z marks still drift on a slow clock.
    pub fn tick(&mut self) {
        for _ in 0..if self.asleep() { 3 } else { 1 } {
            self.pet.step();
        }
    }

    /// (pet canvas w, h, shadow centre x in the canvas, floor y, shadow half width) in screen px.
    fn geom(&self) -> (f32, f32, f32, f32, f32) {
        let (k, x) = (self.k, f32::from(self.pet.x));
        match self.sprites {
            Some(_) => (f32::from(self.cw) * k, f32::from(sprites::CANVAS_H) * k, (x - 3.0 + 16.0) * k, f32::from(sprites::CANVAS_H) * k - 3.0 * k, 7.0 * k),
            None => {
                let s = 2.0 * k;
                (f32::from(self.cw) * s, f32::from(pet::GRID_H) * s, (x + 8.0) * s, f32::from(pet::GRID_H) * s - 0.5 * s, 6.0 * s)
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let (w, h, cx, fy, rx) = self.geom();
        let area_h = f32::from(sprites::CANVAS_H) * self.k;
        let inner = (scu(W) - 2 * scu(PAD_X as u32)) as f32;
        let art: Element<Message> = match &self.sprites {
            Some(s) => widget::canvas(sprites::View { pet: &self.pet, sprites: s, k: self.k }).width(w).height(h).into(),
            None => widget::canvas(&self.pet).width(w).height(h).into(),
        };
        let shadow = widget::canvas(Shadow { cx: cx + (inner - w) / 2.0, fy: fy + area_h - h, rx, ry: rx * 0.22 }).width(Length::Fill).height(area_h);
        let spot = container(mouse_area(art).on_press(Message::Poke)).width(Length::Fill).height(area_h).align_x(Alignment::Center).align_y(cosmic::iced::alignment::Vertical::Bottom);
        let arrow = |l: &'static str, d: i32| button::custom(cosmic::widget::text(l).font(crate::style::FONT_BOLD).size(sc(22.0)).class(cosmic::theme::Text::Color(MUTED))).padding([0.0, sc(10.0)]).on_press(Message::PetSwitch(d)).class(cosmic::theme::Button::Text);
        let cap = cosmic::widget::text(self.caption()).font(FONT).size(sc(13.0)).class(cosmic::theme::Text::Color(MUTED));
        widget::Column::new()
            .push(widget::Space::new().height(Length::Fill))
            .push(stack([shadow.into(), spot.into()]))
            .push(
                widget::Row::new()
                    .align_y(Alignment::Center)
                    .push(arrow("‹", -1))
                    .push(container(cap).width(Length::Fill).center_x(Length::Fill))
                    .push(arrow("›", 1)),
            )
            .into()
    }
}

/// Soft blob under the pet: two stacked flat ellipses.
struct Shadow {
    cx: f32,
    fy: f32,
    rx: f32,
    ry: f32,
}

impl canvas::Program<Message, Theme, Renderer> for Shadow {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, b: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, b.size());
        for (grow, a) in [(1.0, 0.14), (0.7, 0.2)] {
            f.with_save(|f| {
                f.translate(Vector::new(self.cx, self.fy));
                f.scale_nonuniform(Vector::new(self.rx * grow, self.ry * grow));
                f.fill(&canvas::Path::circle(Point::ORIGIN, 1.0), Color::from_rgba(0.0, 0.0, 0.0, a));
            });
        }
        vec![f.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_wraps_across_all_eleven() {
        assert_eq!(COUNT, 11);
        let names: Vec<_> = (0..COUNT).map(name).collect();
        assert_eq!(names, ["cat", "puppy", "fox", "bunny", "hamster", "hedgehog", "piglet", "duckling", "frog", "turtle", "gator"]);
        assert_eq!(name(cycle(0, -1)), "gator");
        assert_eq!(name(cycle(GATOR, 1)), "cat");
        let mut i = 0;
        let fwd: Vec<_> = (0..COUNT).map(|_| { let n = name(i); i = cycle(i, 1); n }).collect();
        assert_eq!(fwd, names);
        assert_eq!(i, 0);
        for i in 0..COUNT {
            assert_eq!(cycle(cycle(i, 1), -1), i);
        }
    }

    #[test]
    fn pet_line_is_replaced_in_place() {
        let t = "# pet = cat\npet_in_panel = true\n  Pet =  FROG   # my favourite\r\nwalk_speed = fast\n";
        assert_eq!(set_pet_line(t, "fox"), "# pet = cat\npet_in_panel = true\n  Pet = fox  # my favourite\r\nwalk_speed = fast\n");
        // plain line, no trailing newline
        assert_eq!(set_pet_line("a = 1\npet = random", "gator"), "a = 1\npet = gator");
        // the last duplicate is the effective one
        assert_eq!(set_pet_line("pet = cat\nx = 1\npet = frog\n", "fox"), "pet = cat\nx = 1\npet = fox\n");
    }

    #[test]
    fn pet_line_is_appended_when_missing() {
        assert_eq!(set_pet_line("", "cat"), "pet = cat\n");
        assert_eq!(set_pet_line("# pet = frog\nscale = 2", "cat"), "# pet = frog\nscale = 2\npet = cat\n");
        assert_eq!(set_pet_line("pet_in_panel = no\n", "cat"), "pet_in_panel = no\npet = cat\n");
    }

    #[test]
    fn template_edit_changes_one_line_and_still_parses_clean() {
        let out = set_pet_line(TEMPLATE, "turtle");
        let diff: Vec<_> = TEMPLATE.lines().zip(out.lines()).filter(|(a, b)| a != b).collect();
        assert_eq!(diff, [("pet = random", "pet = turtle")]);
        assert_eq!(TEMPLATE.lines().count(), out.lines().count());
        let (c, w) = conf::parse(&out);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(c.pet.as_deref(), Some("turtle"));
    }
}
