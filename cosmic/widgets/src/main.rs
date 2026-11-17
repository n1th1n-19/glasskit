// SPDX-License-Identifier: GPL-3.0-or-later
//! glasskit-widgets: one home-screen card per process (`--card NAME`), a wlr layer
//! surface on the Bottom layer (above wallpaper, below windows, no keyboard focus).
mod calendar;
mod mediacard;
// The island's pet engine, shared by path (not copied). Only parts of it are used here.
#[allow(dead_code)]
#[path = "../../island/src/conf.rs"]
mod conf;
#[allow(dead_code)]
#[path = "../../island/src/pet.rs"]
mod pet;
mod petcard;
mod pomodoro;
#[allow(dead_code)]
#[path = "../../island/src/sprites.rs"]
mod sprites;
mod style;

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use cosmic::cctk::sctk::output::OutputInfo;

use cosmic::app::{Core, Settings, Task};
use cosmic::cctk::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{IcedMargin, IcedOutput, SctkLayerSurfaceSettings};
use cosmic::iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use cosmic::iced::{Color, Length, Limits, Point, Radians, Rectangle, Subscription, mouse};
use cosmic::iced::{Alignment as VAlign, alignment::Vertical};
use cosmic::iced::advanced::text::Alignment;
use cosmic::iced::window::Id;
use cosmic::cctk::sctk::reexports::client::protocol::wl_output::WlOutput;
use cosmic::iced::event::wayland::{Event as WlEvent, OutputEvent};
use cosmic::iced::platform_specific::shell::commands::layer_surface::get_layer_surface;
use cosmic::widget::{self, button, container, text};
use cosmic::{Element, Renderer, Theme};
use ember_core::calendar::{MONTHS, WEEKDAYS, now};
use ember_core::control::{bluetooth, media, network, sound};
use ember_core::{Sampler, Snapshot};
use pomodoro::{Phase, State};
use style::*;

const GAP: u32 = 16;
const EDGE: i32 = 48;
const TOP: i32 = 48; // clears the top panel
const W: u32 = 360;

/// Layout is designed for 1080p; `sc` scales every dimension by the chosen output's factor.
/// Set once, before the layer surface is created.
static SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32
pub fn sc(x: f32) -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed)) * x
}
fn scu(x: u32) -> u32 {
    sc(x as f32).round() as u32
}
/// s = clamp(min(h/1080, w/1920), 0.5, 1.5)
fn scale_for(w: i32, h: i32) -> f32 {
    (h as f32 / 1080.0).min(w as f32 / 1920.0).clamp(0.5, 1.5)
}
fn scale_of(i: &OutputInfo) -> Option<f32> {
    let (w, h) = i.logical_size.filter(|s| s.0 > 0 && s.1 > 0).or_else(|| i.modes.iter().find(|m| m.current).map(|m| m.dimensions))?;
    Some(scale_for(w, h))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Card {
    Clock,
    Calendar,
    System,
    Media,
    Toggles,
    Storage,
    Focus,
    Pet,
}

impl Card {
    const ALL: [(&'static str, Card); 8] = [
        ("clock", Card::Clock),
        ("calendar", Card::Calendar),
        ("system", Card::System),
        ("media", Card::Media),
        ("toggles", Card::Toggles),
        ("storage", Card::Storage),
        ("focus", Card::Focus),
        ("pet", Card::Pet),
    ];
    fn height(self) -> u32 {
        scu(self.base_h())
    }
    /// Height at the 1080p reference.
    fn base_h(self) -> u32 {
        match self {
            Card::Clock => 192,
            Card::Calendar => 304,
            Card::System => 240,
            Card::Media => 160,
            Card::Toggles => 204,
            Card::Storage => 224,
            Card::Focus => 170,
            Card::Pet => 170,
        }
    }
    /// (left column?, cards above it in the same column)
    fn slot(self) -> (bool, &'static [Card]) {
        match self {
            Card::Clock => (true, &[]),
            Card::Calendar => (true, &[Card::Clock]),
            Card::System => (true, &[Card::Clock, Card::Calendar]),
            Card::Pet => (true, &[Card::Clock, Card::Calendar, Card::System]),
            Card::Media => (false, &[]),
            Card::Toggles => (false, &[Card::Media]),
            Card::Storage => (false, &[Card::Media, Card::Toggles]),
            Card::Focus => (false, &[Card::Media, Card::Toggles, Card::Storage]),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Tog {
    Wifi,
    Bt,
    Mute,
    Dnd,
}

#[derive(Debug, Clone, Default)]
struct Toggles {
    wifi: bool,
    bt: bool,
    mute: bool,
    dnd: bool,
    adapter: Option<String>,
    sink: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    Output(Option<String>, WlOutput, Option<f32>, bool),
    Media(Option<media::Player>, Option<mediacard::Extra>),
    Art(String, Option<widget::image::Handle>),
    Seek(f32),
    Frame,
    MediaCmd(media::Cmd),
    Toggles(Toggles),
    Flip(Tog),
    FocusToggle,
    FocusSkip,
    FocusReset,
    Anim,
    Poke,
    PetSwitch(i32),
}

struct App {
    core: Core,
    card: Card,
    sampler: Sampler,
    snap: Snapshot,
    media: mediacard::MediaState,
    tg: Toggles,
    focus: State,
    ticks: u32,
    made: bool,
    first: Option<(WlOutput, f32)>,
    scale: f32,
    made_on: Option<String>,
    conf: conf::Conf,
    pet: Option<petcard::PetState>,
}

impl App {
    /// Create the card's layer surface on `output` (once).
    fn make(&mut self, output: IcedOutput, scale: f32, name: Option<String>) -> Task<Message> {
        if std::mem::replace(&mut self.made, true) {
            return Task::none();
        }
        SCALE.store(scale.to_bits(), Ordering::Relaxed);
        self.scale = scale;
        self.made_on = name;
        if self.card == Card::Pet {
            self.pet = Some(petcard::PetState::new(None, &self.conf));
        }
        let (left, above) = self.card.slot();
        let top = scu(TOP as u32) as i32 + above.iter().map(|c| (c.height() + scu(GAP)) as i32).sum::<i32>();
        let anchor = Anchor::TOP | if left { Anchor::LEFT } else { Anchor::RIGHT };
        let edge = scu(EDGE as u32) as i32;
        let w = scu(W);
        let margin = IcedMargin { top, right: if left { 0 } else { edge }, bottom: 0, left: if left { edge } else { 0 } };
        let h = self.card.height();
        get_layer_surface(SctkLayerSurfaceSettings {
            id: Id::unique(),
            layer: Layer::Bottom,
            keyboard_interactivity: KeyboardInteractivity::None,
            anchor,
            output,
            namespace: "glasskit-widget".into(),
            margin,
            size: Some((Some(w), Some(h))),
            exclusive_zone: 0,
            size_limits: Limits::NONE.min_width(1.0).min_height(1.0).max_width(w as f32).max_height(h as f32),
            ..Default::default()
        })
    }
}

fn dnd_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.config/cosmic/com.system76.CosmicNotifications/v1/do_not_disturb").into()
}

async fn read_toggles() -> Toggles {
    let mut t = Toggles::default();
    t.wifi = tokio::task::spawn_blocking(network::read).await.ok().and_then(Result::ok).is_some_and(|n| n.wifi_enabled);
    if let Ok(b) = bluetooth::read().await {
        t.bt = b.powered;
        t.adapter = b.adapter;
    }
    if let Ok(Ok(devs)) = tokio::task::spawn_blocking(|| sound::list(sound::Kind::Output)).await {
        if let Some(d) = devs.into_iter().find(|d| d.default) {
            t.mute = d.muted;
            t.sink = Some(d.name);
        }
    }
    t.dnd = std::fs::read_to_string(dnd_path()).is_ok_and(|s| s.trim() == "true");
    t
}

fn poll(card: Card) -> Task<Message> {
    match card {
        Card::Media => cosmic::task::future(async {
            let (p, e) = mediacard::read().await;
            Message::Media(p, e)
        }),
        Card::Toggles => cosmic::task::future(async { Message::Toggles(read_toggles().await) }),
        _ => Task::none(),
    }
}

/// Run a blocking/async setter, then re-read the toggles.
fn then_reread<F: std::future::Future + Send + 'static>(f: F) -> Task<Message> {
    cosmic::task::future(async move {
        let _ = f.await;
        Message::Toggles(read_toggles().await)
    })
}

fn notify(body: &'static str) {
    let _ = std::process::Command::new("notify-send").args(["-a", "glasskit", "Focus", body]).spawn();
}

impl cosmic::Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Card;
    type Message = Message;
    const APP_ID: &'static str = "io.glasskit.Widgets";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, card: Card) -> (Self, Task<Message>) {
        let mut sampler = Sampler::new();
        std::thread::sleep(Duration::from_millis(200));
        let snap = sampler.sample();
        let app = App { core, card, sampler, snap, media: mediacard::MediaState::new(), tg: Toggles::default(), focus: State::new(), ticks: 0, made: false, first: None, scale: 1.0, made_on: None, conf: conf::load(), pet: None };
        (app, poll(card))
    }

    fn subscription(&self) -> Subscription<Message> {
        let events = cosmic::iced::event::listen_raw(|e, _, _| match e {
            cosmic::iced::Event::PlatformSpecific(cosmic::iced::event::PlatformSpecific::Wayland(WlEvent::Output(
                OutputEvent::Created(Some(i)),
                o,
            ))) => Some(Message::Output(i.name.clone(), o, scale_of(&i), false)),
            cosmic::iced::Event::PlatformSpecific(cosmic::iced::event::PlatformSpecific::Wayland(WlEvent::Output(OutputEvent::InfoUpdate(i), o))) => {
                Some(Message::Output(i.name.clone(), o, i.logical_size.and_then(|_| scale_of(&i)), true))
            }
            _ => None,
        });
        let mut subs = vec![cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick), events];
        if self.card == Card::Media && self.media.moving() {
            subs.push(cosmic::iced::time::every(Duration::from_millis(250)).map(|_| Message::Frame));
        }
        if let Some(p) = &self.pet {
            // ~animation_fps while awake; a sleeper only wakes once a second (z marks)
            let ms = if p.asleep() { 1000 } else { 1000 / u64::from(self.conf.animation_fps.max(1)) };
            subs.push(cosmic::iced::time::every(Duration::from_millis(ms)).map(|_| Message::Anim));
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Output(name, o, s, update) => {
                // Resolution changed on our output: exit so autostart/the user relaunches at the new scale
                // (ponytail: no live set_size/set_margin resize; all layout is baked into view + surface).
                if self.made && update && self.made_on.is_some() && name == self.made_on && s.is_some_and(|s| (s - self.scale).abs() > 1e-3) {
                    std::process::exit(0);
                }
                let want = std::env::var("GLASSKIT_OUTPUT").unwrap_or_else(|_| "eDP-1".into());
                if name.as_deref() == Some(want.as_str()) {
                    if let Some(s) = s {
                        return self.make(IcedOutput::Output(o), s, name);
                    }
                    // no size yet: wait for an InfoUpdate (or the 2s fallback)
                    return Task::none();
                }
                if let Some(s) = s {
                    self.first.get_or_insert((o, s));
                }
            }
            Message::Tick => {
                self.ticks += 1;
                // wanted output never appeared: fall back to first seen / active
                if self.ticks >= 2 && !self.made {
                    let (o, s) = self.first.take().map_or((IcedOutput::Active, 1.0), |(o, s)| (IcedOutput::Output(o), s));
                    return Task::batch([self.make(o, s, None), poll(self.card)]);
                }
                if matches!(self.card, Card::System | Card::Storage) {
                    self.snap = self.sampler.sample();
                }
                if self.card == Card::Focus {
                    let (s, ended) = self.focus.tick();
                    self.focus = s;
                    if ended {
                        notify(if s.phase == Phase::Break { "Work done. Take 5." } else { "Break over. Back to it." });
                    }
                }
                // media polls every second (position); the rest every other tick
                if self.ticks % 2 == 0 || self.card == Card::Media {
                    return poll(self.card);
                }
            }
            Message::Media(p, e) => {
                if let Some(url) = self.media.apply(p, e) {
                    return cosmic::task::future(async move { Message::Art(url.clone(), mediacard::load_art(url).await) });
                }
            }
            Message::Art(url, h) => self.media.set_art(&url, h),
            Message::Frame => {}
            Message::Seek(f) => {
                if let Some((bus, tid, to)) = self.media.seek(f) {
                    return cosmic::task::future(async move {
                        mediacard::seek(bus, tid, to).await;
                        let (p, e) = mediacard::read().await;
                        Message::Media(p, e)
                    });
                }
            }
            Message::MediaCmd(c) => {
                if let Some(p) = &self.media.player {
                    let bus = p.bus.clone();
                    return cosmic::task::future(async move {
                        let _ = media::control(&bus, c).await;
                        let (p, e) = mediacard::read().await;
                        Message::Media(p, e)
                    });
                }
            }
            Message::Toggles(t) => self.tg = t,
            Message::Flip(which) => {
                let t = self.tg.clone();
                return match which {
                    Tog::Wifi => then_reread(async move { tokio::task::spawn_blocking(move || network::set_wifi(!t.wifi)).await }),
                    Tog::Bt => match t.adapter {
                        Some(a) => then_reread(async move { bluetooth::set_powered(&a, !t.bt).await }),
                        None => Task::none(),
                    },
                    Tog::Mute => match t.sink {
                        Some(s) => then_reread(async move { tokio::task::spawn_blocking(move || sound::set_mute(sound::Kind::Output, &s, !t.mute)).await }),
                        None => Task::none(),
                    },
                    // ponytail: assumes cosmic-notifications watches this key file; unverified on-device
                    Tog::Dnd => then_reread(async move {
                        let p = dnd_path();
                        if let Some(d) = p.parent() {
                            let _ = std::fs::create_dir_all(d);
                        }
                        std::fs::write(p, if t.dnd { "false" } else { "true" })
                    }),
                };
            }
            Message::FocusToggle => self.focus = self.focus.toggle(),
            Message::FocusSkip => self.focus = self.focus.skip(),
            Message::FocusReset => self.focus = self.focus.reset(),
            Message::Anim => {
                if let Some(p) = &mut self.pet {
                    p.tick();
                }
            }
            Message::Poke => {
                if let Some(p) = &mut self.pet {
                    p.pet.poke();
                }
            }
            Message::PetSwitch(d) => {
                if let Some(p) = &self.pet {
                    let i = petcard::cycle(p.idx, d);
                    self.pet = Some(petcard::PetState::new(Some(i), &self.conf));
                    if let Err(e) = petcard::save_pet(petcard::name(i)) {
                        eprintln!("glasskit-widgets: could not write island.conf: {e}");
                    }
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        self.card_view()
    }
    fn view_window(&self, _: Id) -> Element<'_, Message> {
        self.card_view()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::iced::theme::Style { background_color: Color::TRANSPARENT, text_color: TEXT, icon_color: TEXT })
    }
}

struct Ring {
    frac: f32,
    color: Color,
    label: String,
}

impl canvas::Program<Message, Theme, Renderer> for Ring {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, b: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, b.size());
        let c = Point::new(b.width / 2.0, b.height / 2.0);
        let r = b.width.min(b.height) / 2.0 - sc(6.0);
        let st = |col| Stroke::default().with_color(col).with_width(sc(8.0).max(1.0)).with_line_cap(canvas::LineCap::Round);
        f.stroke(&Path::circle(c, r), st(TRACK));
        let a0 = -std::f32::consts::FRAC_PI_2;
        let end = a0 + std::f32::consts::TAU * self.frac.clamp(0.0, 1.0);
        if self.frac > 0.005 {
            let arc = Path::new(|p| p.arc(canvas::path::Arc { center: c, radius: r, start_angle: Radians(a0), end_angle: Radians(end) }));
            f.stroke(&arc, st(self.color));
        }
        f.fill_text(Text { content: self.label.clone(), position: c, size: sc(16.0).into(), font: FONT_BOLD, color: TEXT, align_x: Alignment::Center, align_y: Vertical::Center, ..Text::default() });
        vec![f.into_geometry()]
    }
}

fn ring<'a>(frac: Option<f32>, color: Color, label: &str, caption: &str) -> Element<'a, Message> {
    let f = frac.unwrap_or(0.0);
    let l = frac.map_or("--".to_string(), |v| format!("{:.0}", v * 100.0));
    widget::Column::new()
        .align_x(VAlign::Center)
        .spacing(sc(6.0))
        .push(widget::canvas(Ring { frac: f, color, label: l }).width(sc(64.0)).height(sc(64.0)))
        .push(muted(label))
        .push(muted(caption))
        .into()
}

fn pill<'a>(label: &str, on: bool, msg: Message, color: Color) -> Element<'a, Message> {
    button::custom(container(body(label)).center_x(Length::Fill).padding([sc(8.0), 0.0]))
        .width(Length::Fill)
        .on_press(msg)
        .class(cosmic::theme::Button::Custom {
            active: Box::new(move |_, _| pill_style(on, color)),
            hovered: Box::new(move |_, _| pill_style(true, color)),
            pressed: Box::new(move |_, _| pill_style(true, color)),
            disabled: Box::new(move |_| pill_style(false, color)),
        })
        .into()
}

fn fmt_rate(b: Option<f64>) -> String {
    match b {
        None => "--".into(),
        Some(v) if v >= 1e6 => format!("{:.1} MB/s", v / 1e6),
        Some(v) => format!("{:.0} KB/s", v / 1e3),
    }
}

fn gb(b: u64) -> f64 {
    b as f64 / 1e9
}

impl App {
    fn card_view(&self) -> Element<'_, Message> {
        let n = now();
        let body: Element<Message> = match self.card {
            Card::Clock => {
                let (h12, ampm) = (if n.hour % 12 == 0 { 12 } else { n.hour % 12 }, if n.hour < 12 { "AM" } else { "PM" });
                widget::Column::new()
                    .spacing(sc(4.0))
                    .push(text(format!("{h12}:{:02} {ampm}", n.minute)).font(FONT_BOLD).size(sc(64.0)).class(cosmic::theme::Text::Color(TEXT)))
                    .push(muted(format!("{}, {} {}", WEEKDAYS[n.weekday as usize], MONTHS[n.month as usize - 1], n.day)))
                    .into()
            }
            Card::Calendar => {
                let mut col = widget::Column::new().spacing(sc(6.0)).push(heading(format!("{} {}", MONTHS[n.month as usize - 1], n.year)));
                let cell = |s: String, hot: bool, dim: bool| {
                    container(text(s).size(sc(14.0)).font(FONT).class(cosmic::theme::Text::Color(if hot { Color::WHITE } else if dim { MUTED } else { TEXT })))
                        .width(Length::Fill)
                        .center_x(Length::Fill)
                        .padding([sc(4.0), 0.0])
                        .class(cosmic::theme::Container::custom(move |_| today_style(hot)))
                };
                let mut hdr = widget::Row::new();
                for d in ["M", "T", "W", "T", "F", "S", "S"] {
                    hdr = hdr.push(cell(d.into(), false, true));
                }
                col = col.push(hdr);
                for w in calendar::grid(n.year, n.month) {
                    let mut r = widget::Row::new();
                    for d in w {
                        r = r.push(cell(d.map_or(String::new(), |d| d.to_string()), d == Some(n.day), false));
                    }
                    col = col.push(r);
                }
                col.into()
            }
            Card::System => {
                let s = &self.snap;
                let bat = s.batteries.first().and_then(|b| b.percent).map(|p| p / 100.0);
                let temp = s.cpu_temp().map(|t| t / 100.0);
                let rings = widget::Row::new()
                    .spacing(sc(12.0))
                    .push(ring(s.cpu_percent.map(|v| v / 100.0), ACCENT, "CPU", &s.cpu_percent.map_or("--".into(), |v| format!("{v:.0}%"))))
                    .push(ring(s.mem_percent().map(|v| v / 100.0), PURPLE, "RAM", &format!("{:.1} GB", gb(s.mem_used))))
                    .push(ring(bat, GREEN, "BAT", &bat.map_or("--".into(), |v| format!("{:.0}%", v * 100.0))))
                    .push(ring(temp, ORANGE, "TEMP", &s.cpu_temp().map_or("--".into(), |t| format!("{t:.0}°C"))));
                widget::Column::new().spacing(sc(14.0)).push(heading("System")).push(rings).into()
            }
            Card::Media => self.media.view(),
            Card::Toggles => {
                let t = &self.tg;
                widget::Column::new()
                    .spacing(sc(10.0))
                    .push(heading("Quick"))
                    .push(widget::Row::new().spacing(sc(10.0)).push(pill("Wi-Fi", t.wifi, Message::Flip(Tog::Wifi), ACCENT)).push(pill("Bluetooth", t.bt, Message::Flip(Tog::Bt), ACCENT)))
                    .push(widget::Row::new().spacing(sc(10.0)).push(pill("Mute", t.mute, Message::Flip(Tog::Mute), ORANGE)).push(pill("Do Not Disturb", t.dnd, Message::Flip(Tog::Dnd), PURPLE)))
                    .into()
            }
            Card::Storage => {
                let s = &self.snap;
                let root = s.disks.iter().find(|d| d.mount == "/").or(s.disks.first());
                let (used, total) = root.map_or((0, 1), |d| (d.used, d.total.max(1)));
                let bar = widget::determinate_linear(used as f32 / total as f32);
                widget::Column::new()
                    .spacing(sc(10.0))
                    .push(heading("Storage"))
                    .push(body(format!("{:.0} of {:.0} GB used", gb(used), gb(total))))
                    .push(bar)
                    .push(heading("Network"))
                    .push(body(format!("Down {}", fmt_rate(s.net_rx))))
                    .push(body(format!("Up {}", fmt_rate(s.net_tx))))
                    .into()
            }
            Card::Pet => match &self.pet {
                Some(p) => p.view(),
                None => widget::Space::new().into(),
            },
            Card::Focus => {
                let f = self.focus;
                let ctl = |l: &str, m| button::custom(body(l)).padding([sc(6.0), sc(12.0)]).on_press(m).class(cosmic::theme::Button::Text);
                widget::Column::new()
                    .spacing(sc(8.0))
                    .push(heading(if f.phase == Phase::Work { "Focus" } else { "Break" }))
                    .push(text(format!("{:02}:{:02}", f.remaining / 60, f.remaining % 60)).font(FONT_BOLD).size(sc(48.0)).class(cosmic::theme::Text::Color(TEXT)))
                    .push(
                        widget::Row::new()
                            .push(ctl(if f.running { "Pause" } else { "Start" }, Message::FocusToggle))
                            .push(ctl("Skip", Message::FocusSkip))
                            .push(ctl("Reset", Message::FocusReset)),
                    )
                    .into()
            }
        };
        let pad = if self.card == Card::Pet { [sc(petcard::PAD_Y), sc(petcard::PAD_X)] } else { [sc(24.0); 2] };
        container(body)
            .padding(pad)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(|_| card_style()))
            .into()
    }
}

fn main() -> cosmic::iced::Result {
    let args: Vec<String> = std::env::args().collect();
    let name = args.iter().position(|a| a == "--card").and_then(|i| args.get(i + 1));
    let Some(card) = name.and_then(|n| Card::ALL.iter().find(|(k, _)| k == n).map(|(_, c)| *c)) else {
        eprintln!("usage: glasskit-widgets --card clock|calendar|system|media|toggles|storage|focus|pet");
        std::process::exit(2);
    };
    if card == Card::Pet && !conf::load().pet_widget {
        eprintln!("glasskit-widgets: pet_widget = false in island.conf, not starting the pet card");
        return Ok(());
    }
    let settings = Settings::default().no_main_window(true).transparent(true);
    cosmic::app::run::<App>(settings, card)
}

#[cfg(test)]
mod tests {
    use super::{Card, GAP, TOP, scale_for};
    #[test]
    fn scale() {
        assert_eq!(scale_for(1920, 1080), 1.0);
        assert!((scale_for(1366, 768) - 0.7114).abs() < 1e-3);
        assert_eq!(scale_for(3840, 2160), 1.5);
        assert_eq!(scale_for(640, 360), 0.5);
    }

    /// Every column must end inside the screen (same rounding as `make`).
    #[test]
    fn columns_fit_the_screen() {
        for (w, h) in [(1920, 1080), (1366, 768), (2560, 1440), (1280, 720)] {
            let s = scale_for(w, h);
            let r = |x: u32| (x as f32 * s).round() as i32;
            for (_, c) in Card::ALL {
                let (_, above) = c.slot();
                let top = r(TOP as u32) + above.iter().map(|a| r(a.base_h()) + r(GAP)).sum::<i32>();
                assert!(top + r(c.base_h()) <= h, "{c:?} overflows {w}x{h}");
            }
        }
        assert_eq!(Card::ALL.len(), 8);
    }
}
