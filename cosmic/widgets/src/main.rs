// SPDX-License-Identifier: GPL-3.0-or-later
//! glasskit-widgets: one home-screen card per process (`--card NAME`), a wlr layer
//! surface on the Bottom layer (above wallpaper, below windows, no keyboard focus).
//! Cards are rearrangeable: `--edit` (or a right-click on any card) toggles edit mode, where the
//! cards sit on the Top layer, wear an outline and drag anywhere; positions live in widgets.conf.
mod calendar;
mod trim;
mod layout;
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
mod store;
mod style;

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use cosmic::cctk::sctk::output::OutputInfo;

use cosmic::app::{Core, Settings, Task};
use cosmic::cctk::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{IcedMargin, IcedOutput, SctkLayerSurfaceSettings};
use cosmic::iced::widget::canvas::{self, Frame, Geometry, LineDash, Path, Stroke, Text};
use cosmic::iced::widget::stack;
use cosmic::iced::{Color, Length, Limits, Point, Radians, Rectangle, Size, Subscription, mouse};
use cosmic::iced::{Alignment as VAlign, alignment::Vertical};
use cosmic::iced::advanced::text::Alignment;
use cosmic::iced::window::Id;
use cosmic::cctk::sctk::reexports::client::protocol::wl_output::WlOutput;
use cosmic::iced::event::wayland::{Event as WlEvent, OutputEvent};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{get_layer_surface, set_anchor, set_layer, set_margin};
use cosmic::widget::{self, button, container, text};
use cosmic::{Element, Renderer, Theme};
use ember_core::calendar::{MONTHS, WEEKDAYS, now};
use ember_core::control::{bluetooth, media, network, sound};
use ember_core::{Sampler, Snapshot};
use pomodoro::{Phase, State};
use layout::{Dims, Place, Px, Side};
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
/// GLASSKIT_DEBUG_INPUT=1: log every mouse event, message and surface setup to stderr.
fn debug_input() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("GLASSKIT_DEBUG_INPUT").is_some_and(|v| v != "0"))
}

/// s = clamp(min(h/1080, w/1920), 0.5, 1.5)
fn scale_for(w: i32, h: i32) -> f32 {
    (h as f32 / 1080.0).min(w as f32 / 1920.0).clamp(0.5, 1.5)
}
/// (scale, (width, height)) of an output.
type Geo = (f32, (i32, i32));
fn scale_of(i: &OutputInfo) -> Option<Geo> {
    let (w, h) = i.logical_size.filter(|s| s.0 > 0 && s.1 > 0).or_else(|| i.modes.iter().find(|m| m.current).map(|m| m.dimensions))?;
    Some((scale_for(w, h), (w, h)))
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
            Card::Focus => 200, // heading + 48px time + button row + 2x24 padding (170 clipped the buttons)
            Card::Pet => 170,
        }
    }
    fn name(self) -> &'static str {
        Card::ALL.iter().find(|(_, c)| *c == self).map_or("", |(n, _)| n)
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

/// Where a card sits with no widgets.conf entry: the original two columns, in output px (needs
/// SCALE set). Same rounding as before widgets.conf existed, so no file = no change.
fn default_px(c: Card) -> Px {
    let (left, above) = c.slot();
    Px { side: if left { Side::Left } else { Side::Right }, x: scu(EDGE as u32) as i32, y: scu(TOP as u32) as i32 + above.iter().map(|a| (a.height() + scu(GAP)) as i32).sum::<i32>() }
}

fn card_names() -> Vec<&'static str> {
    Card::ALL.iter().map(|(n, _)| *n).collect()
}

fn margin_of(p: Px) -> IcedMargin {
    IcedMargin { top: p.y, right: if p.side == Side::Right { p.x } else { 0 }, bottom: 0, left: if p.side == Side::Left { p.x } else { 0 } }
}

fn anchor_of(side: Side) -> Anchor {
    Anchor::TOP | if side == Side::Left { Anchor::LEFT } else { Anchor::RIGHT }
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
    Output(Option<String>, WlOutput, Option<Geo>, bool),
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
    /// every 250 ms: edit flag and reload stamp
    Poll,
    /// right-click on a card
    ToggleEdit,
    /// raw pointer position / left release (edit mode only)
    Ptr(Point),
    PtrUp,
    DragStart,
    Hide,
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
    first: Option<(WlOutput, Geo)>,
    scale: f32,
    made_on: Option<String>,
    conf: conf::Conf,
    pet: Option<petcard::PetState>,
    // --- rearranging
    edit: bool,
    surface: Option<Id>,
    out: (i32, i32),
    px: Px,
    /// last pointer position (surface coordinates) while in edit mode
    ptr: Option<Point>,
    /// `Some(None)` = pressed, waiting for the first pointer position; `Some(Some(p))` = press point
    drag: Option<Option<Point>>,
    last_move: Instant,
    reload: Option<String>,
}

impl App {
    /// Create the card's layer surface on `output` (once).
    fn make(&mut self, output: IcedOutput, geo: Geo, name: Option<String>) -> Task<Message> {
        if std::mem::replace(&mut self.made, true) {
            return Task::none();
        }
        let (scale, out) = geo;
        SCALE.store(scale.to_bits(), Ordering::Relaxed);
        self.scale = scale;
        self.out = out;
        self.made_on = name;
        if self.card == Card::Pet {
            self.pet = Some(petcard::PetState::new(None, &self.conf));
        }
        self.px = self.px_of(self.saved(true));
        let w = scu(W);
        let h = self.card.height();
        let id = Id::unique();
        self.surface = Some(id);
        if debug_input() {
            eprintln!("[input] {}: layer surface {id:?} layer={:?} keyboard=None edit={} size={w}x{h}", self.card.name(), if self.edit { Layer::Top } else { Layer::Bottom }, self.edit);
        }
        get_layer_surface(SctkLayerSurfaceSettings {
            id,
            layer: if self.edit { Layer::Top } else { Layer::Bottom },
            keyboard_interactivity: KeyboardInteractivity::None,
            anchor: anchor_of(self.px.side),
            output,
            namespace: "glasskit-widget".into(),
            margin: margin_of(self.px),
            size: Some((Some(w), Some(h))),
            exclusive_zone: 0,
            size_limits: Limits::NONE.min_width(1.0).min_height(1.0).max_width(w as f32).max_height(h as f32),
            ..Default::default()
        })
    }

    /// What widgets.conf says about this card.
    fn saved(&self, warn: bool) -> layout::Over {
        store::load(&card_names(), warn).get(self.card.name()).copied().unwrap_or_default()
    }

    /// Output-px placement: file values (reference px, scaled) over the default place, kept on screen.
    fn px_of(&self, o: layout::Over) -> Px {
        let (d, s) = (default_px(self.card), self.scale);
        layout::clamp(Px { side: o.side.unwrap_or(d.side), x: o.x.map_or(d.x, |v| (v * s).round() as i32), y: o.y.map_or(d.y, |v| (v * s).round() as i32) }, self.dims())
    }

    fn dims(&self) -> Dims {
        Dims { w: scu(W) as i32, h: self.card.height() as i32, out_w: self.out.0, out_h: self.out.1 }
    }

    /// Push the current anchor + margin to the compositor.
    fn apply_px(&self) -> Task<Message> {
        let Some(id) = self.surface else { return Task::none() };
        let m = margin_of(self.px);
        Task::batch([set_anchor(id, anchor_of(self.px.side)), set_margin(id, m.top, m.right, m.bottom, m.left)])
    }

    fn set_edit_mode(&mut self, on: bool) -> Task<Message> {
        if on == self.edit {
            return Task::none();
        }
        self.edit = on;
        self.drag = None;
        self.ptr = None;
        match self.surface {
            Some(id) => set_layer(id, if on { Layer::Top } else { Layer::Bottom }),
            None => Task::none(),
        }
    }

    /// One drag step from the pointer at `p` (see `layout::drag_step`).
    fn drag_to(&mut self, p: Point) -> Task<Message> {
        let Some(Some(p0)) = self.drag else { return Task::none() };
        let next = layout::drag_step(self.px, p.x - p0.x, p.y - p0.y, self.dims(), scu(layout::GRID as u32).max(1) as i32, scu(EDGE as u32) as i32, scu(TOP as u32) as i32);
        if next == self.px {
            return Task::none();
        }
        self.px = next;
        self.last_move = Instant::now();
        self.apply_px()
    }

    /// Save this card's place to widgets.conf (atomic, locked).
    fn save(&mut self) {
        let (x, y) = layout::to_ref(self.px, self.scale);
        let place = Place { side: self.px.side, x, y, visible: true };
        if let Err(e) = store::save_place(self.card.name(), &place) {
            eprintln!("glasskit-widgets: could not save widgets.conf: {e}");
        }
    }

    /// widgets.conf or the reload stamp changed (`--reset`, hand edit): follow it.
    fn reload(&mut self) -> Task<Message> {
        let o = self.saved(false);
        if o.visible == Some(false) {
            std::process::exit(0);
        }
        let px = self.px_of(o);
        if px == self.px || self.drag.is_some() {
            return Task::none();
        }
        self.px = px;
        self.apply_px()
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
        let app = App {
            core,
            card,
            sampler,
            snap,
            media: mediacard::MediaState::new(),
            tg: Toggles::default(),
            focus: State::new(),
            ticks: 0,
            made: false,
            first: None,
            scale: 1.0,
            made_on: None,
            conf: conf::load(),
            pet: None,
            edit: store::edit_on(),
            surface: None,
            out: (1920, 1080),
            px: Px { side: Side::Left, x: 0, y: 0 },
            ptr: None,
            drag: None,
            last_move: Instant::now(),
            reload: store::reload_token(),
        };
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
        let mut subs = vec![cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick), cosmic::iced::time::every(Duration::from_millis(250)).map(|_| Message::Poll), events];
        if debug_input() {
            subs.push(cosmic::iced::event::listen_raw(|e, status, id| {
                if let cosmic::iced::Event::Mouse(m) = &e {
                    eprintln!("[input] mouse {m:?} status={status:?} window={id:?}");
                }
                None
            }));
        }
        if self.edit {
            subs.push(cosmic::iced::event::listen_raw(|e, _, _| match e {
                cosmic::iced::Event::Mouse(mouse::Event::CursorMoved { position }) => Some(Message::Ptr(position)),
                cosmic::iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(Message::PtrUp),
                _ => None,
            }));
        }
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
        if debug_input() && !matches!(msg, Message::Tick | Message::Poll | Message::Anim | Message::Frame | Message::Media(..) | Message::Art(..) | Message::Toggles(_) | Message::Output(..)) {
            eprintln!("[input] {}: message {msg:?} (edit={})", self.card.name(), self.edit);
        }
        // edit mode: a click only drags, the card's own buttons are dead
        if self.edit && matches!(msg, Message::Flip(_) | Message::MediaCmd(_) | Message::Seek(_) | Message::FocusToggle | Message::FocusSkip | Message::FocusReset | Message::Poke | Message::PetSwitch(_)) {
            return Task::none();
        }
        match msg {
            Message::Output(name, o, s, update) => {
                // Resolution changed on our output: exit so autostart/the user relaunches at the new scale
                // (ponytail: no live set_size/set_margin resize; all layout is baked into view + surface).
                if self.made && update && self.made_on.is_some() && name == self.made_on && s.is_some_and(|s| (s.0 - self.scale).abs() > 1e-3) {
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
                    let (o, s) = self.first.take().map_or((IcedOutput::Active, (1.0, (1920, 1080))), |(o, s)| (IcedOutput::Output(o), s));
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
            Message::Poll => {
                let on = store::edit_on();
                let tok = store::reload_token();
                let mut tasks = vec![self.set_edit_mode(on)];
                if tok != self.reload {
                    self.reload = tok;
                    tasks.push(self.reload());
                }
                return Task::batch(tasks);
            }
            Message::ToggleEdit => {
                let on = !store::edit_on();
                if let Err(e) = store::set_edit(on) {
                    eprintln!("glasskit-widgets: could not toggle edit mode: {e}");
                }
                return self.set_edit_mode(on);
            }
            Message::DragStart => self.drag = Some(self.ptr),
            // ponytail: events within 12 ms of the last move are dropped; their coordinates still
            // belong to the surface's old position and would apply the same delta twice.
            Message::Ptr(p) => {
                self.ptr = Some(p);
                match self.drag {
                    Some(None) => self.drag = Some(Some(p)),
                    Some(Some(_)) if self.last_move.elapsed() >= Duration::from_millis(12) => return self.drag_to(p),
                    _ => {}
                }
            }
            Message::PtrUp => {
                if self.drag.is_some() {
                    let t = self.ptr.map_or_else(Task::none, |p| self.drag_to(p));
                    self.drag = None;
                    self.save();
                    return t;
                }
            }
            Message::Hide => {
                if let Err(e) = store::set_visible(self.card.name(), false) {
                    eprintln!("glasskit-widgets: could not save widgets.conf: {e}");
                }
                std::process::exit(0);
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
        self.rooted()
    }
    fn view_window(&self, _: Id) -> Element<'_, Message> {
        self.rooted()
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

/// Dashed accent outline + faint tint drawn over a card in edit mode.
struct Outline;

impl canvas::Program<Message, Theme, Renderer> for Outline {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, b: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, b.size());
        let w = sc(2.0).max(1.0);
        let path = Path::rounded_rectangle(Point::new(w / 2.0, w / 2.0), Size::new(b.width - w, b.height - w), sc(28.0).into());
        f.fill(&path, Color { a: 0.07, ..ACCENT });
        let dash = [sc(10.0).max(2.0), sc(7.0).max(2.0)];
        f.stroke(&path, Stroke { line_dash: LineDash { segments: &dash, offset: 0 }, ..Stroke::default().with_color(ACCENT).with_width(w) });
        vec![f.into_geometry()]
    }
}

/// Material "open_with" (four-way arrow) and "close", 24x24.
const MOVE_PATH: &str = "M10 9h4V6h3l-5-5-5 5h3v3zm-1 1H6V7l-5 5 5 5v-3h3v-4zm14 2l-5-5v3h-3v4h3v3l5-5zm-9 3h-4v3H7l5 5 5-5h-3v-3z";
const CLOSE_PATH: &str = "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z";

/// White glyph on an accent disc, `px` (scaled) wide.
fn disc_icon<'a>(path: &str, px: f32) -> Element<'a, Message> {
    let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#ffffff" d="{path}"/></svg>"##);
    container(widget::svg(widget::svg::Handle::from_memory(svg.into_bytes())).width(sc(px * 0.6)).height(sc(px * 0.6)))
        .center(Length::Fixed(sc(px)))
        .class(cosmic::theme::Container::custom(move |_| cosmic::iced::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(ACCENT)),
            border: cosmic::iced::Border { radius: (sc(px) / 2.0).into(), ..Default::default() },
            ..Default::default()
        }))
        .into()
}

impl App {
    /// The card, wrapped for rearranging: right-click toggles edit mode; in edit mode an outline,
    /// a grab layer (swallows every left press, starts the drag), the move glyph and the hide button.
    fn rooted(&self) -> Element<'_, Message> {
        let card = self.card_view();
        let body: Element<Message> = if self.edit {
            let fill = || widget::Space::new().width(Length::Fill).height(Length::Fill);
            let grab = widget::mouse_area(fill()).on_press(Message::DragStart).interaction(mouse::Interaction::Grab);
            // top centre: the left is where headings start, the right is the x button
            let handle = container(disc_icon(MOVE_PATH, 30.0)).padding(sc(10.0)).width(Length::Fill).height(Length::Fill).align_x(cosmic::iced::alignment::Horizontal::Center);
            let close = container(button::custom(disc_icon(CLOSE_PATH, 26.0)).padding(0).on_press(Message::Hide).class(cosmic::theme::Button::Text))
                .padding(sc(10.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(cosmic::iced::alignment::Horizontal::Right);
            stack([card, widget::canvas(Outline).width(Length::Fill).height(Length::Fill).into(), grab.into(), handle.into(), close.into()]).into()
        } else {
            card
        };
        widget::mouse_area(body).on_right_press(Message::ToggleEdit).into()
    }

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

const USAGE: &str = "usage: glasskit-widgets --card clock|calendar|system|media|toggles|storage|focus|pet
       glasskit-widgets --edit [on|off]   toggle (or set) edit mode: drag the cards, click x to hide one
       glasskit-widgets --lock            same as --edit off
       glasskit-widgets --show <card|all> bring hidden cards back
       glasskit-widgets --reset           back up widgets.conf to widgets.conf.bak, delete it, default layout";

/// The non-card commands. `None` = not one of them (run a card).
fn cli(args: &[String]) -> Option<i32> {
    let arg = |i: usize| args.get(i).map(String::as_str);
    let fail = |e: std::io::Error| {
        eprintln!("glasskit-widgets: {e}");
        1
    };
    let names = card_names();
    match arg(1)? {
        "--edit" | "--lock" => {
            let on = match (arg(1), arg(2)) {
                (Some("--lock"), _) | (_, Some("off")) => false,
                (_, Some("on")) => true,
                (_, None) => !store::edit_on(),
                _ => {
                    eprintln!("{USAGE}");
                    return Some(2);
                }
            };
            if let Err(e) = store::set_edit(on) {
                return Some(fail(e));
            }
            println!("edit mode {}", if on { "on" } else { "off" });
            if on {
                let hidden: Vec<_> = names.iter().filter(|n| store::load(&names, false).get(**n).is_some_and(|o| o.visible == Some(false))).collect();
                if !hidden.is_empty() {
                    println!("hidden cards: {}  (bring back with: glasskit-widgets --show <card|all>)", hidden.iter().map(|n| **n).collect::<Vec<_>>().join(", "));
                }
            }
            Some(0)
        }
        "--show" => {
            let want = arg(2).unwrap_or("");
            let list: Vec<&str> = if want == "all" { names.clone() } else { names.iter().copied().filter(|n| *n == want).collect() };
            if list.is_empty() {
                eprintln!("{USAGE}");
                return Some(2);
            }
            let hidden = store::load(&names, false);
            for c in list {
                if hidden.get(c).is_some_and(|o| o.visible == Some(false)) {
                    if let Err(e) = store::set_visible(c, true) {
                        return Some(fail(e));
                    }
                }
                if !store::running(c) {
                    if let Err(e) = store::launch(c) {
                        return Some(fail(e));
                    }
                    println!("started {c}");
                }
            }
            Some(0)
        }
        "--reset" => {
            match store::reset() {
                Ok(true) => println!("widgets.conf backed up to widgets.conf.bak and removed"),
                Ok(false) => println!("no widgets.conf, already the default layout"),
                Err(e) => return Some(fail(e)),
            }
            store::touch_reload(); // running cards move back
            for c in names.iter().filter(|c| !store::running(c)) {
                let _ = store::launch(c); // cards that were hidden come back too
            }
            Some(0)
        }
        "--card" => None,
        _ => {
            eprintln!("{USAGE}");
            Some(2)
        }
    }
}

fn main() -> cosmic::iced::Result {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = cli(&args) {
        std::process::exit(code);
    }
    let name = args.iter().position(|a| a == "--card").and_then(|i| args.get(i + 1));
    let Some(card) = name.and_then(|n| Card::ALL.iter().find(|(k, _)| k == n).map(|(_, c)| *c)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    if card == Card::Pet && !conf::load().pet_widget {
        eprintln!("glasskit-widgets: pet_widget = false in island.conf, not starting the pet card");
        return Ok(());
    }
    // hidden (x button / visible = false): stay out, unless edit mode is on
    if !store::edit_on() && store::load(&card_names(), false).get(card.name()).is_some_and(|o| o.visible == Some(false)) {
        return Ok(());
    }
    trim::start();
    let settings = Settings::default().no_main_window(true).transparent(true);
    cosmic::app::run::<App>(settings, card)
}

#[cfg(test)]
mod tests {
    use super::{Card, SCALE, default_px, scale_for};
    use crate::layout::{self, Dims, Side};
    use std::sync::atomic::Ordering;

    #[test]
    fn scale() {
        assert_eq!(scale_for(1920, 1080), 1.0);
        assert!((scale_for(1366, 768) - 0.7114).abs() < 1e-3);
        assert_eq!(scale_for(3840, 2160), 1.5);
        assert_eq!(scale_for(640, 360), 0.5);
    }

    /// One test, because SCALE is process-global. Default places equal the old slot-based margins
    /// (hard-coded here) and every card ends inside the screen.
    #[test]
    fn columns_fit_the_screen() {
        use Card::*;
        // (card, left column?, margin from its edge, margin from the top)
        let at_1080: [(Card, bool, i32, i32); 8] = [(Clock, true, 48, 48), (Calendar, true, 48, 256), (System, true, 48, 576), (Pet, true, 48, 832), (Media, false, 48, 48), (Toggles, false, 48, 224), (Storage, false, 48, 444), (Focus, false, 48, 684)];
        let at_768: [(Card, bool, i32, i32); 8] = [(Clock, true, 34, 34), (Calendar, true, 34, 182), (System, true, 34, 409), (Pet, true, 34, 591), (Media, false, 34, 34), (Toggles, false, 34, 159), (Storage, false, 34, 315), (Focus, false, 34, 485)];
        for (w, h, want) in [(1920, 1080, Some(at_1080)), (1366, 768, Some(at_768)), (2560, 1440, None), (1280, 720, None)] {
            let s = scale_for(w, h);
            SCALE.store(s.to_bits(), Ordering::Relaxed);
            let r = |x: u32| (x as f32 * s).round() as i32;
            for (_, c) in Card::ALL {
                let px = default_px(c);
                let d = Dims { w: r(super::W), h: c.height() as i32, out_w: w, out_h: h };
                assert_eq!(layout::clamp(px, d), px, "{c:?} is off the screen at {w}x{h}");
                assert!(px.y + d.h <= h, "{c:?} overflows {w}x{h}");
            }
            for (c, left, x, y) in want.into_iter().flatten() {
                let px = default_px(c);
                assert_eq!((px.side == Side::Left, px.x, px.y), (left, x, y), "{c:?} at {w}x{h}");
            }
        }
        SCALE.store(1.0f32.to_bits(), Ordering::Relaxed);
        assert_eq!(Card::ALL.len(), 8);
    }
}
