// SPDX-License-Identifier: GPL-3.0-or-later
//! Glasskit Island: a COSMIC panel applet that shows, in priority order,
//! notification > volume > charge/low battery > now playing > idle dot.
mod conf;
mod pet;
mod sprites;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use cosmic::app::{Core, Task};
use cosmic::iced::window::Id;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{Alignment, Background, Border, Color, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{self, column, row};
use cosmic::Element;
use ember_core::control::media::{self, Cmd, Player};
use ember_core::control::sound::{self, Kind};

const ID: &str = "io.github.n1th1n_19.Glasskit.Island";
const ACCENT: Color = Color::from_rgb(0.231, 0.510, 0.965); // #3b82f6
const GREEN: Color = Color::from_rgb(0.204, 0.827, 0.600); // #34d399
const RED: Color = Color::from_rgb(0.973, 0.443, 0.443); // #f87171
const NOTIFY_FOR: Duration = Duration::from_secs(4);
const VOLUME_FOR: Duration = Duration::from_millis(1500);
const CHARGE_FOR: Duration = Duration::from_secs(3);
const IGNORE_NOTIFY: Duration = Duration::from_secs(3);

/// What the island shows, highest priority first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Notify,
    Volume,
    Charge,
    Low,
    Media,
    Idle,
}

/// Which sources currently have something to say.
#[derive(Debug, Clone, Copy, Default)]
pub struct Active {
    pub notify: bool,
    pub volume: bool,
    pub charge: bool,
    pub low: bool,
    pub media: bool,
}

/// notify > volume > charge/low-battery > media > idle dot.
pub fn pick(a: Active) -> Mode {
    if a.notify {
        Mode::Notify
    } else if a.volume {
        Mode::Volume
    } else if a.low {
        Mode::Low
    } else if a.charge {
        Mode::Charge
    } else if a.media {
        Mode::Media
    } else {
        Mode::Idle
    }
}

#[derive(Debug, Clone, Default)]
pub struct Snap {
    volume: Option<(f32, bool)>,
    battery: Option<(f32, bool)>, // percent, charging
    player: Option<Player>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    PetTick,
    /// The pet in the popup was clicked.
    PetClick,
    Poll,
    Snap(Snap),
    Notify(String, String),
    Media(Cmd),
    PopupClosed(Id),
    Surface(cosmic::surface::Action<Message>),
}

struct Island {
    core: Core,
    popup: Option<Id>,
    start: Instant,
    snap: Snap,
    seen: bool,
    notify: Option<(String, Instant)>,
    volume_at: Option<Instant>,
    charge_at: Option<Instant>,
    low_at: Option<Instant>,
    pet: pet::Pet,
    /// The popup's own pet (same look, own state), animated only while the card shows it.
    popup_pet: pet::Pet,
    look: Look,
    conf: conf::Conf,
}

/// The popup card shows the pet instead of album art only when nothing plays and a pet is on.
pub fn show_pet(media: bool, pet_on: bool) -> bool {
    !media && pet_on
}

/// What the idle pill shows; `Dot` (GLASSKIT_PET=0 / `off`) is the plain accent dot.
enum Look {
    Dot,
    Gator,
    Animal(sprites::Sprites),
}

impl Look {
    /// State-machine ranges (pill, popup), already tuned by island.conf.
    fn cfgs(&self, c: &conf::Conf) -> (pet::Cfg, pet::Cfg) {
        let (pill, pop) = match self {
            Look::Dot | Look::Gator => (pet::gator(pet::GRID_W), pet::gator(POP_GRID_W)),
            Look::Animal(s) => (s.cfg(sprites::CANVAS_W), s.cfg(POP_CANVAS_W)),
        };
        (pill.tune(c), pop.tune(c))
    }
}

/// Pick the look from config; any failure to load an animal falls back to the gator.
fn look(seed: u32, c: &conf::Conf) -> (Look, pet::Pet, pet::Pet) {
    let animal = |i: usize| sprites::Sprites::load(&sprites::ANIMALS[i]).map(Look::Animal);
    let l = match sprites::choice(c) {
        sprites::Choice::Off => Look::Dot,
        sprites::Choice::Gator => Look::Gator,
        sprites::Choice::Animal(i) => animal(i).unwrap_or(Look::Gator),
        sprites::Choice::Random => animal(seed as usize / 7 % sprites::ANIMALS.len()).unwrap_or(Look::Gator),
    };
    let (a, b) = l.cfgs(c);
    let (pill, pop) = (pet::Pet::with(seed, a), pet::Pet::with(seed, a).twin(b));
    (l, pill, pop)
}

/// Pet scale in the pill: (animal px per sprite px, gator px per grid px). `auto` keeps the
/// size-based pick; an explicit n gives the animal n and the gator 3n (the same pill height).
fn panel_scales(size: f32, set: Option<u8>) -> (f32, f32) {
    match set {
        Some(n) => (f32::from(n), 3.0 * f32::from(n)), // same 30n px pill height for both
        None => (if size >= 40.0 { 2.0 } else { 1.0 }, pet::scale(size)),
    }
}

impl Island {
    fn within(t: Option<Instant>, d: Duration) -> bool {
        t.is_some_and(|t| t.elapsed() < d)
    }

    fn mode(&self) -> Mode {
        pick(Active {
            notify: self.notify.as_ref().is_some_and(|(_, t)| t.elapsed() < NOTIFY_FOR),
            volume: Self::within(self.volume_at, VOLUME_FOR),
            charge: Self::within(self.charge_at, CHARGE_FOR),
            low: Self::within(self.low_at, CHARGE_FOR),
            media: self.media(),
        })
    }

    fn media(&self) -> bool {
        self.snap.player.as_ref().is_some_and(|p| !p.title.is_empty())
    }

    /// A pet is on and the popup may show it when nothing plays.
    fn popup_pet_on(&self) -> bool {
        self.conf.popup_pet && !matches!(self.look, Look::Dot)
    }

    /// The open popup is currently showing the pet.
    fn popup_pet_live(&self) -> bool {
        self.popup.is_some() && show_pet(self.media(), self.popup_pet_on())
    }

    /// The idle pill shows the pet (not the accent dot).
    fn panel_pet(&self) -> bool {
        self.conf.pet_in_panel && !matches!(self.look, Look::Dot)
    }

    /// The pet's canvas `cw` sprite px (gator: `gw` grid px) wide at `k` (animals: px per sprite px; gator: `gator_k` per grid px), if the look has one.
    fn pet_canvas<'a>(look: &'a Look, p: &'a pet::Pet, (k, gator_k): (f32, f32), (cw, gw): (u8, u8)) -> Option<Element<'a, Message>> {
        match look {
            Look::Dot => None,
            Look::Animal(sp) => Some(widget::canvas(sprites::View { pet: p, sprites: sp, k }).width(Length::Fixed(f32::from(cw) * k)).height(Length::Fixed(f32::from(sprites::CANVAS_H) * k)).into()),
            Look::Gator => Some(widget::canvas(p).width(Length::Fixed(f32::from(gw) * gator_k)).height(Length::Fixed(f32::from(pet::GRID_H) * gator_k)).into()),
        }
    }

    fn label(&self) -> (String, Color) {
        let s = &self.snap;
        match self.mode() {
            Mode::Notify => (self.notify.as_ref().map(|(t, _)| t.chars().take(36).collect()).unwrap_or_default(), Color::WHITE),
            Mode::Volume => match s.volume {
                Some((_, true)) => ("Muted".into(), Color::WHITE),
                Some((v, _)) => (format!("Volume {:.0}%", v * 100.0), ACCENT),
                None => (String::new(), Color::WHITE),
            },
            Mode::Charge => (format!("Charging {:.0}%", s.battery.map_or(0.0, |b| b.0)), GREEN),
            Mode::Low => (format!("Low battery {:.0}%", s.battery.map_or(0.0, |b| b.0)), RED),
            Mode::Media => {
                let p = s.player.as_ref().unwrap();
                let t = if p.artist.is_empty() { p.title.clone() } else { format!("{} - {}", p.title, p.artist) };
                (format!("{}{}", if p.playing { "" } else { "|| " }, t.chars().take(36).collect::<String>()), Color::WHITE)
            }
            Mode::Idle => ("\u{25CF}".into(), ACCENT),
        }
    }
}

fn glass(radius: f32, alpha: f32, rgb: [f32; 3]) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |_| widget::container::Style {
        text_color: Some(Color::from_rgb(0.957, 0.961, 0.969)),
        background: Some(Background::Color(Color::from_rgba(rgb[0], rgb[1], rgb[2], alpha))),
        border: Border { color: Color::from_rgb(0.149, 0.165, 0.2), width: 1.0, radius: radius.into() },
        ..Default::default()
    })
}

async fn snapshot() -> Snap {
    let blocking = tokio::task::spawn_blocking(|| {
        let volume = sound::list(Kind::Output).ok().and_then(|l| l.into_iter().find(|d| d.default)).map(|d| (d.volume, d.muted));
        let battery = ember_core::battery::read_all().into_iter().find_map(|b| Some((b.percent?, b.status.as_deref() == Some("Charging"))));
        (volume, battery)
    })
    .await
    .unwrap_or_default();
    Snap { volume: blocking.0, battery: blocking.1, player: media::read().await.ok().flatten() }
}

/// Passive D-Bus monitor on the session bus: sees every Notify call without owning the name.
fn notifications() -> impl futures_util::Stream<Item = Message> {
    cosmic::iced::stream::channel(16, async |mut tx: cosmic::iced::futures::channel::mpsc::Sender<Message>| {
        use futures_util::{SinkExt, StreamExt};
        let Ok(conn) = zbus::Connection::session().await else { return };
        let Ok(rule) = zbus::MatchRule::builder().msg_type(zbus::message::Type::MethodCall).interface("org.freedesktop.Notifications").member("Notify").build() else { return };
        let Ok(mon) = zbus::fdo::MonitoringProxy::new(&conn).await else { return };
        if mon.become_monitor(&[rule], 0).await.is_err() {
            return;
        }
        let started = Instant::now();
        let mut stream = zbus::MessageStream::from(&conn);
        while let Some(Ok(m)) = stream.next().await {
            if started.elapsed() < IGNORE_NOTIFY {
                continue;
            }
            type Body = (String, u32, String, String, String, Vec<String>, HashMap<String, zbus::OwnedVariant>, i32);
            if let Ok((app, _, _, summary, _, _, _, _)) = m.body().deserialize::<Body>() {
                let _ = tx.send(Message::Notify(app, summary)).await;
            }
        }
    })
}

impl cosmic::Application for Island {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos());
        let conf = conf::load();
        let (look, pet, popup_pet) = look(seed, &conf);
        let s = Self { core, popup: None, start: Instant::now(), snap: Snap::default(), seen: false, notify: None, volume_at: None, charge_at: None, low_at: None, pet, popup_pet, look, conf };
        (s, cosmic::task::message(cosmic::Action::App(Message::Poll)))
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            cosmic::iced::time::every(Duration::from_millis(500)).map(|_| Message::Poll),
            cosmic::iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick),
            Subscription::run(notifications),
            // Animation clock exists only while the pill idles or the popup shows its pet: zero wakeups otherwise.
            if (self.panel_pet() && self.mode() == Mode::Idle) || self.popup_pet_live() { cosmic::iced::time::every(Duration::from_millis(1000 / u64::from(self.conf.animation_fps))).map(|_| Message::PetTick) } else { Subscription::none() },
        ])
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Tick => {
                if self.mode() != Mode::Idle {
                    self.pet.reset();
                }
            }
            Message::PetTick => {
                if self.mode() == Mode::Idle {
                    self.pet.step();
                } else {
                    self.pet.reset();
                }
                if self.popup_pet_live() {
                    self.popup_pet.step();
                } else {
                    self.popup_pet.reset();
                }
            }
            Message::PetClick => {
                if self.popup_pet_live() {
                    self.popup_pet.poke();
                }
            }
            Message::Poll => return cosmic::task::future(async { Message::Snap(snapshot().await) }),
            Message::Snap(s) => {
                if self.seen {
                    if let (Some(a), Some(b)) = (self.snap.volume, s.volume) {
                        if (a.0 - b.0).abs() > 0.005 || a.1 != b.1 {
                            self.volume_at = Some(Instant::now());
                        }
                    }
                    if let (Some(a), Some(b)) = (self.snap.battery, s.battery) {
                        if a.1 != b.1 {
                            self.charge_at = Some(Instant::now());
                        }
                        if !b.1 && b.0 <= 15.0 && a.0 > 15.0 {
                            self.low_at = Some(Instant::now());
                        }
                    }
                }
                self.seen = true;
                self.snap = s;
            }
            Message::Notify(app, summary) => {
                if self.start.elapsed() > IGNORE_NOTIFY {
                    self.notify = Some((if summary.is_empty() { app } else { format!("{app}: {summary}") }, Instant::now()));
                }
            }
            Message::Media(c) => {
                if let Some(p) = &self.snap.player {
                    let bus = p.bus.clone();
                    return cosmic::task::future(async move {
                        let _ = media::control(&bus, c).await;
                        Message::Poll
                    });
                }
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.popup_pet.reset();
                }
            }
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let applet = &self.core.applet;
        let have_popup = self.popup;
        let (text, color) = self.label();
        let size = f32::from(applet.suggested_size(true).0).max(14.0) * 0.8;
        let idle = self.mode() == Mode::Idle;
        // Animal: 1x (a 32px cell is about the panel height), 2x only on very large panels.
        let art = if idle && self.panel_pet() { Self::pet_canvas(&self.look, &self.pet, panel_scales(size, self.conf.scale), (sprites::CANVAS_W, pet::GRID_W)) } else { None };
        let pill = match art {
            Some(canvas) => widget::container(canvas).padding([0, 4]).class(glass(18.0, 0.55, [0.063, 0.071, 0.086])),
            None => widget::container(widget::text(text).size(size).class(cosmic::theme::Text::Color(color)))
                .padding([2, 14])
                .class(glass(18.0, 0.55, [0.063, 0.071, 0.086])),
        };
        let (px, py) = applet.suggested_padding(true);
        let btn = widget::button::custom(pill).class(cosmic::theme::Button::AppletIcon).padding([py, px]).on_press_with_rectangle(move |offset, bounds| match have_popup {
            Some(id) => Message::Surface(destroy_popup(id)),
            None => popup(offset, bounds),
        });
        applet.autosize_window(btn).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        "".into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

/// Click popup: media card.
fn popup(offset: cosmic::iced::Vector, bounds: Rectangle) -> Message {
    Message::Surface(app_popup::<Island>(
        |_| Default::default(),
        move |state: &mut Island| {
            let id = Id::unique();
            state.popup = Some(id);
            let mut s = state.core.applet.get_popup_settings(state.core.main_window_id().unwrap(), id, None, None, None);
            s.positioner.anchor_rect = Rectangle { x: (bounds.x - offset.x) as i32, y: (bounds.y - offset.y) as i32, width: bounds.width as i32, height: bounds.height as i32 };
            s.positioner.size_limits = cosmic::iced::Limits::NONE.min_width(POP_W).max_width(W).min_height(1.0).max_height(400.0);
            s
        },
        Some(Box::new(|state: &Island| Element::from(card(state)).map(cosmic::Action::App))),
    ))
}

const W: f32 = 320.0;
/// Pet scale in the popup (animals; the 32px cell becomes 128px).
const POP_K: f32 = 4.0;
/// Gator scale in the popup, picked to look about as big as a 4x animal.
const POP_GATOR_K: f32 = 7.0;
/// Popup card padding and the pet's walking width in sprite px: 62 * 4 = 248 inner width.
const POP_PAD: f32 = 16.0;
const POP_CANVAS_W: u8 = 62;
/// Gator grid width for the same inner width at 7x (35 * 7 = 245 of 248 px).
const POP_GRID_W: u8 = 35;
/// Compact pet card: 248 + 2 * 16 = 280 wide, 120 + 2 * 10 = 140 tall.
const POP_W: f32 = 280.0;
const POP_PAD_Y: f32 = 10.0;

fn card(s: &Island) -> Element<'_, Message> {
    let compact = if show_pet(s.media(), s.popup_pet_on()) { Island::pet_canvas(&s.look, &s.popup_pet, (POP_K, POP_GATOR_K), (POP_CANVAS_W, POP_GRID_W)) } else { None };
    if let Some(canvas) = compact {
        // Nothing playing: just the pet, centred in a small glass card; clicking it hops.
        let pet = widget::mouse_area(canvas).on_press(Message::PetClick);
        let inner = widget::container(pet).width(Length::Fill).height(Length::Fixed(f32::from(sprites::CANVAS_H) * POP_K)).align_x(Horizontal::Center).align_y(Vertical::Bottom);
        let framed = widget::container(inner).padding([POP_PAD_Y, POP_PAD]).width(Length::Fixed(POP_W)).class(glass(18.0, 0.58, [0.071, 0.078, 0.098]));
        return autosized(framed, POP_W);
    }
    let (title, artist, playing) = match &s.snap.player {
        Some(p) => (p.title.clone(), p.artist.clone(), p.playing),
        None => ("Nothing playing".into(), String::new(), false),
    };
    let play = widget::button::custom(widget::text(if playing { "Pause" } else { "Play" }).class(cosmic::theme::Text::Color(Color::WHITE)))
        .class(cosmic::theme::Button::Custom {
            active: Box::new(|_, _| accent_btn()),
            disabled: Box::new(|_| accent_btn()),
            hovered: Box::new(|_, _| accent_btn()),
            pressed: Box::new(|_, _| accent_btn()),
        })
        .padding([8, 20])
        .on_press(Message::Media(Cmd::PlayPause));
    let controls = row::with_capacity(3)
        .spacing(10)
        .align_y(Alignment::Center)
        .push(widget::button::standard("Prev").on_press(Message::Media(Cmd::Previous)))
        .push(play)
        .push(widget::button::standard("Next").on_press(Message::Media(Cmd::Next)));
    let body = column::with_capacity(3).spacing(10).align_x(Alignment::Center).push(widget::text::title4(title)).push(widget::text::caption(artist)).push(controls);
    let framed = widget::container(body).padding(16).width(Length::Fixed(W)).class(glass(18.0, 0.58, [0.071, 0.078, 0.098]));
    autosized(framed, W)
}

/// The popup window follows the card's size, which is `w` wide.
fn autosized(card: widget::Container<'_, Message, cosmic::Theme>, w: f32) -> Element<'_, Message> {
    cosmic::widget::autosize::autosize(card, cosmic::widget::Id::new("cosmic-applet-autosize"))
        .limits(cosmic::iced::Limits::NONE.min_width(w).max_width(w).min_height(1.0).max_height(400.0))
        .into()
}

fn accent_btn() -> widget::button::Style {
    widget::button::Style { background: Some(Background::Color(ACCENT)), border_radius: 18.0.into(), text_color: Some(Color::WHITE), ..Default::default() }
}

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<Island>(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_shows_pet_only_when_idle_and_pet_on() {
        assert!(show_pet(false, true));
        assert!(!show_pet(true, true)); // playing: normal card
        assert!(!show_pet(false, false)); // pet off: current card
        assert!(!show_pet(true, false));
    }

    #[test]
    fn popup_card_layout_adds_up() {
        // pet canvas + padding = card: 280 x 140, walking range = the whole inner width
        assert_eq!(f32::from(POP_CANVAS_W) * POP_K + 2.0 * POP_PAD, POP_W);
        assert_eq!(f32::from(sprites::CANVAS_H) * POP_K + 2.0 * POP_PAD_Y, 140.0);
        let inner = POP_W - 2.0 * POP_PAD;
        assert!(f32::from(POP_GRID_W) * POP_GATOR_K <= inner && inner - f32::from(POP_GRID_W) * POP_GATOR_K < 8.0);
        assert_eq!((inner / f32::from(POP_GRID_W)).floor(), POP_GATOR_K); // gator draw() derives the same scale
        let conf = conf::Conf::default();
        let sp = sprites::Sprites::load(&sprites::ANIMALS[9]).unwrap();
        let (pill, pop) = (sp.cfg(sprites::CANVAS_W).tune(&conf), sp.cfg(POP_CANVAS_W).tune(&conf));
        assert!(pop.x_max > pill.x_max);
        // rightmost sprite pixel (cell edge minus its 3px margin) lands on the inner edge
        assert_eq!(i32::from(pop.x_max) - 3 + sprites::CELL - 3, i32::from(POP_CANVAS_W));
        let (_, g) = Look::Gator.cfgs(&conf);
        assert_eq!(g.x_max + pet::SPR_W_GRID, POP_GRID_W);
    }

    #[test]
    fn panel_scale_auto_or_explicit() {
        assert_eq!(panel_scales(20.0, None), (1.0, 3.0));
        assert_eq!(panel_scales(50.0, None), (2.0, 3.0)); // auto keeps its old size-based picks
        assert_eq!(panel_scales(50.0, Some(1)), (1.0, 3.0));
        assert_eq!(panel_scales(20.0, Some(3)), (3.0, 9.0));
    }

    #[test]
    fn priority_order() {
        let all = Active { notify: true, volume: true, charge: true, low: true, media: true };
        assert_eq!(pick(all), Mode::Notify);
        assert_eq!(pick(Active { notify: false, ..all }), Mode::Volume);
        assert_eq!(pick(Active { notify: false, volume: false, ..all }), Mode::Low);
        assert_eq!(pick(Active { notify: false, volume: false, low: false, ..all }), Mode::Charge);
        assert_eq!(pick(Active { media: true, ..Default::default() }), Mode::Media);
        assert_eq!(pick(Active::default()), Mode::Idle);
    }
}
