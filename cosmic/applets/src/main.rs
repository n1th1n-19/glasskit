// SPDX-License-Identifier: GPL-3.0-or-later
//! Glasskit panel applets: `glasskit-applets clock` and `glasskit-applets status`.
mod logic;

use std::marker::PhantomData;
use std::process::Command;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::iced::font::{Family, Weight};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Background, Border, Color, Font, Length, Rectangle, Subscription, Vector};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{self, column, row};
use cosmic::Element;
use ember_core::calendar::{self, Now, MONTHS};
use ember_core::control::bluetooth::{self, BtDevice};
use ember_core::control::network::{self, Wifi};
use ember_core::control::sound;
use logic::*;

const ACCENT: Color = Color::from_rgb(0.231, 0.510, 0.965); // #3b82f6
const TEXT: Color = Color::from_rgb(0.957, 0.961, 0.969); // #f4f5f7
const MUTED: Color = Color::from_rgb(0.545, 0.576, 0.639); // #8b93a3
const GREEN: Color = Color::from_rgb(0.204, 0.827, 0.600);
const ORANGE: Color = Color::from_rgb(0.984, 0.573, 0.235);
const RED: Color = Color::from_rgb(0.937, 0.267, 0.267);
const W: f32 = 340.0;
/// Popup never taller than this (a 768px screen minus panel and margins).
const POPUP_MAX_H: f32 = 540.0;

fn nunito(w: Weight) -> Font {
    Font { family: Family::Name("Nunito"), weight: w, ..Font::DEFAULT }
}

// ---- clock logic --------------------------------------------------------------

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// `Wed 18 Nov  1:10 AM`
pub fn clock_text(n: Now) -> String {
    let h = if n.hour % 12 == 0 { 12 } else { n.hour % 12 };
    format!("{} {} {}  {h}:{:02} {}", DAYS[n.weekday as usize % 7], n.day, &MONTHS[n.month as usize - 1][..3], n.minute, if n.hour < 12 { "AM" } else { "PM" })
}

/// Six Monday-first weeks; days outside the month are None.
pub fn grid(y: i32, m: u32) -> [[Option<u32>; 7]; 6] {
    let mut g = [[None; 7]; 6];
    let start = ((calendar::weekday(y, m, 1) + 6) % 7) as usize;
    for d in 1..=calendar::days_in(y, m) {
        let i = start + d as usize - 1;
        g[i / 7][i % 7] = Some(d);
    }
    g
}

// ---- state ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WifiSnap {
    enabled: bool,
    airplane: bool,
    device: Option<String>,
    nets: Vec<Wifi>,
}

#[derive(Debug, Clone)]
pub struct BtSnap {
    adapter: String,
    powered: bool,
    paired: Vec<BtDevice>,
    /// discovered but not paired: (mac, name)
    nearby: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct BattSnap {
    pct: Option<f32>,
    status: Option<String>,
    line: String,
    mins: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Snap {
    out: Option<(String, f32, bool)>, // name, volume, muted
    wifi: Option<WifiSnap>,
    bt: Option<BtSnap>,
    batt: Option<BattSnap>,
}

/// Wi-Fi password in flight; Debug is redacted so it can never reach a log.
#[derive(Clone, Default)]
pub struct Secret(String);
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sec {
    Sound,
    Wifi,
    Bt,
    Batt,
}

struct PwEntry {
    ssid: String,
    pw: Secret,
    wpa3: bool,
    /// a saved profile exists and is stale: delete it before joining again
    forget: bool,
}

/// Popup-only UI state (survives snapshot refreshes).
struct Ui {
    open: Option<Sec>,
    pw: Option<PwEntry>,
    note: Option<String>,
    /// ssid / mac / bluez path currently being connected
    busy: Option<String>,
    scanning: bool,
    rescanning: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Snap(Snap),
    Press(Vector, Rectangle),
    Open(Sec),
    Vol(f32),
    Mute(bool),
    Wifi(bool),
    Bt(bool),
    Rescan,
    WifiConnect(String),
    WifiDisconnect,
    WifiPw(Secret),
    WifiSubmit,
    WifiCancel,
    /// ssid, may retry with a password, result
    NetDone(String, bool, Result<(), String>),
    BtConnect(String, bool),
    BtPair(String),
    BtDone(Result<(), String>),
    BtScan,
    PopupClosed(Id),
    Surface(cosmic::surface::Action<Message>),
}

pub trait Kind: 'static {
    const ID: &'static str;
    const STATUS: bool;
}
pub struct Clock;
pub struct Status;
impl Kind for Clock {
    const ID: &'static str = "io.github.n1th1n_19.Glasskit.Clock";
    const STATUS: bool = false;
}
impl Kind for Status {
    const ID: &'static str = "io.github.n1th1n_19.Glasskit.Status";
    const STATUS: bool = true;
}

struct Applet<K: Kind> {
    core: Core,
    popup: Option<Id>,
    snap: Snap,
    ui: Ui,
    now: Now,
    _k: PhantomData<K>,
}

fn read_batt() -> Option<BattSnap> {
    for e in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path().join("uevent")) else { continue };
        if let Some(b) = ember_core::battery::parse_uevent(&text) {
            return Some(BattSnap { pct: b.percent, line: b.status_line(), mins: time_left_mins(&text), status: b.status });
        }
    }
    None
}

fn run(prog: &str, args: &[&str]) -> Result<String, String> {
    let o = Command::new(prog).args(args).output().map_err(|e| format!("{prog} not available: {e}"))?;
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    if o.status.success() { Ok(text) } else { Err(String::from_utf8_lossy(&o.stderr).trim().trim_start_matches("Error: ").to_string()) }
}

/// Pair (if needed), trust and connect a discovered device. Blocking.
fn pair_connect(mac: &str) -> Result<(), String> {
    if !valid_mac(mac) {
        return Err("bad device address".into());
    }
    let out = run("bluetoothctl", &["--timeout", "20", "pair", mac])?;
    if !out.contains("Pairing successful") && !out.contains("AlreadyExists") {
        return Err("Pairing failed (put the device in pairing mode)".into());
    }
    let _ = run("bluetoothctl", &["trust", mac]);
    let out = run("bluetoothctl", &["--timeout", "15", "connect", mac])?;
    if out.contains("Connection successful") { Ok(()) } else { Err("Paired, but could not connect".into()) }
}

async fn snapshot(rescan: bool) -> Snap {
    let b = tokio_blocking(move || {
        if rescan {
            let _ = run("nmcli", &["dev", "wifi", "rescan"]); // errors if scanned a moment ago: fine
            std::thread::sleep(Duration::from_millis(2500));
        }
        let out = sound::list(sound::Kind::Output).ok().and_then(|l| l.into_iter().find(|d| d.default)).map(|d| (d.name, d.volume, d.muted));
        let wifi = network::read().ok().map(|n| WifiSnap { enabled: n.wifi_enabled, airplane: n.airplane, device: n.wifi_device, nets: n.networks });
        let known = run("bluetoothctl", &["devices"]).map(|o| parse_bt_devices(&o)).unwrap_or_default();
        (out, wifi, read_batt(), known)
    })
    .await;
    let bt = bluetooth::read().await.ok().map(|s| BtSnap {
        adapter: s.adapter.unwrap_or_default(),
        powered: s.powered,
        nearby: b.3.into_iter().filter(|(mac, _)| !s.devices.iter().any(|d| path_is_mac(&d.path, mac))).collect(),
        paired: s.devices,
    });
    Snap { out: b.0, wifi: b.1, bt, batt: b.2 }
}

async fn tokio_blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T
where
    T: Default,
{
    let (tx, rx) = cosmic::iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await.unwrap_or_default()
}

async fn blocking_res(f: impl FnOnce() -> Result<(), String> + Send + 'static) -> Result<(), String> {
    tokio_blocking(move || Some(f())).await.unwrap_or_else(|| Err("cancelled".into()))
}

fn refresh(rescan: bool) -> Task<Message> {
    cosmic::task::future(async move { Message::Snap(snapshot(rescan).await) })
}

fn pw_id() -> widget::Id {
    widget::Id::new("glasskit-wifi-pw")
}

impl<K: Kind> cosmic::Application for Applet<K> {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = K::ID;

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let first = if K::STATUS { refresh(false) } else { Task::none() };
        let ui = Ui { open: Some(Sec::Sound), pw: None, note: None, busy: None, scanning: false, rescanning: false };
        (Self { core, popup: None, snap: Snap::default(), ui, now: calendar::now(), _k: PhantomData }, first)
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        cosmic::iced::time::every(Duration::from_secs(if K::STATUS { 3 } else { 1 })).map(|_| Message::Tick)
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        // ponytail: control calls are blocking-but-fast (pactl/nmcli); run off-thread, re-poll after.
        let act = |f: Box<dyn FnOnce() + Send>| {
            cosmic::task::future(async move {
                tokio_blocking(move || f()).await;
                Message::Tick
            })
        };
        match msg {
            Message::Tick => return if K::STATUS { refresh(false) } else { self.now = calendar::now(); Task::none() },
            Message::Snap(s) => {
                self.snap = s;
                self.ui.rescanning = false;
            }
            Message::Press(offset, bounds) => match self.popup {
                Some(id) => return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id))),
                None => {
                    self.ui.note = None;
                    self.ui.pw = None;
                    self.ui.rescanning = K::STATUS;
                    let open = cosmic::task::message(cosmic::Action::Surface(popup_action::<K>(offset, bounds)));
                    return if K::STATUS { Task::batch([open, refresh(true)]) } else { open };
                }
            },
            Message::Open(sec) => {
                self.ui.open = if self.ui.open == Some(sec) { None } else { Some(sec) };
                self.ui.pw = None;
                self.ui.note = None;
            }
            Message::Vol(v) => {
                if let Some((name, vol, _)) = &mut self.snap.out {
                    *vol = v;
                    let n = name.clone();
                    return act(Box::new(move || drop(sound::set_volume(sound::Kind::Output, &n, v))));
                }
            }
            Message::Mute(m) => {
                if let Some((name, _, muted)) = &mut self.snap.out {
                    *muted = m;
                    let n = name.clone();
                    return act(Box::new(move || drop(sound::set_mute(sound::Kind::Output, &n, m))));
                }
            }
            Message::Wifi(on) => {
                if let Some(w) = &mut self.snap.wifi {
                    w.enabled = on;
                }
                return act(Box::new(move || drop(network::set_wifi(on))));
            }
            Message::Bt(on) => {
                if let Some(b) = &mut self.snap.bt {
                    b.powered = on;
                    let a = b.adapter.clone();
                    return cosmic::task::future(async move {
                        let _ = bluetooth::set_powered(&a, on).await;
                        Message::Tick
                    });
                }
            }
            Message::Rescan => {
                self.ui.rescanning = true;
                self.ui.note = None;
                return refresh(true);
            }
            Message::WifiConnect(ssid) => {
                let Some(w) = &self.snap.wifi else { return Task::none() };
                let Some(n) = w.nets.iter().find(|n| n.ssid == ssid).cloned() else { return Task::none() };
                let dev = w.device.clone().unwrap_or_default();
                self.ui.note = None;
                if n.known {
                    self.ui.busy = Some(ssid.clone());
                    return cosmic::task::future(async move {
                        let s = ssid.clone();
                        let r = blocking_res(move || network::connect_known(&s)).await;
                        Message::NetDone(ssid, true, r)
                    });
                } else if n.secure {
                    self.ui.pw = Some(PwEntry { ssid, pw: Secret::default(), wpa3: n.wpa3, forget: false });
                    return widget::text_input::focus(pw_id());
                }
                self.ui.busy = Some(ssid.clone());
                return cosmic::task::future(async move {
                    let r = network::connect_new(&dev, &ssid, None, false).await;
                    Message::NetDone(ssid, false, r)
                });
            }
            Message::WifiDisconnect => {
                if let Some(dev) = self.snap.wifi.as_ref().and_then(|w| w.device.clone()) {
                    return act(Box::new(move || drop(run("nmcli", &["dev", "disconnect", &dev]))));
                }
            }
            Message::WifiPw(s) => {
                if let Some(p) = &mut self.ui.pw {
                    p.pw = s;
                }
            }
            Message::WifiCancel => self.ui.pw = None,
            Message::WifiSubmit => {
                let Some(p) = self.ui.pw.take() else { return Task::none() };
                if p.pw.0.len() < 8 {
                    self.ui.note = Some("Wi-Fi passwords are 8 to 63 characters.".into());
                    self.ui.pw = Some(p);
                    return Task::none();
                }
                let dev = self.snap.wifi.as_ref().and_then(|w| w.device.clone()).unwrap_or_default();
                self.ui.busy = Some(p.ssid.clone());
                self.ui.note = None;
                return cosmic::task::future(async move {
                    if p.forget {
                        let s = p.ssid.clone();
                        let _ = blocking_res(move || network::forget(&s)).await;
                    }
                    let r = network::connect_new(&dev, &p.ssid, Some(&p.pw.0), p.wpa3).await;
                    Message::NetDone(p.ssid, false, r)
                });
            }
            Message::NetDone(ssid, retry_with_password, r) => {
                self.ui.busy = None;
                if let Err(e) = r {
                    let wpa3 = self.snap.wifi.as_ref().and_then(|w| w.nets.iter().find(|n| n.ssid == ssid)).is_some_and(|n| n.wpa3);
                    let low = e.to_lowercase();
                    if retry_with_password && (low.contains("secret") || low.contains("password")) {
                        self.ui.note = Some("Saved password was rejected. Enter it again.".into());
                        self.ui.pw = Some(PwEntry { ssid, pw: Secret::default(), wpa3, forget: true });
                        return Task::batch([refresh(false), widget::text_input::focus(pw_id())]);
                    }
                    self.ui.note = Some(e);
                }
                return refresh(false);
            }
            Message::BtConnect(path, on) => {
                self.ui.busy = Some(path.clone());
                self.ui.note = None;
                return cosmic::task::future(async move { Message::BtDone(bluetooth::set_connected(&path, on).await) });
            }
            Message::BtPair(mac) => {
                self.ui.busy = Some(mac.clone());
                self.ui.note = None;
                return cosmic::task::future(async move { Message::BtDone(blocking_res(move || pair_connect(&mac)).await) });
            }
            Message::BtDone(r) => {
                self.ui.busy = None;
                self.ui.scanning = false;
                if let Err(e) = r {
                    self.ui.note = Some(e);
                }
                return refresh(false);
            }
            Message::BtScan => {
                if self.ui.scanning {
                    return Task::none();
                }
                self.ui.scanning = true;
                return cosmic::task::future(async {
                    // ponytail: fixed 8 s scan; new devices show up on the 3 s poll meanwhile
                    let _ = blocking_res(|| run("bluetoothctl", &["--timeout", "8", "scan", "on"]).map(drop)).await;
                    Message::BtDone(Ok(()))
                });
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.ui.pw = None;
                }
            }
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let applet = &self.core.applet;
        let size = f32::from(applet.suggested_size(true).0).max(14.0);
        let (px, py) = applet.suggested_padding(true);
        if !K::STATUS {
            let btn = widget::button::custom(clock_pill(self.now, size)).class(cosmic::theme::Button::AppletIcon).padding([py, px]).on_press_with_rectangle(Message::Press);
            return applet.autosize_window(btn).into();
        }
        // no fill or border: bare glyphs, only a faint hover tint on the button
        let btn = widget::button::custom(status_row(&self.snap, (size * 0.85).round().clamp(20.0, 24.0)))
            .class(btn_class(Color::TRANSPARENT, 10.0))
            .padding([py, px])
            .on_press_with_rectangle(Message::Press);
        let tip = tooltip_text(&self.snap);
        applet.autosize_window(applet.applet_tooltip(btn, tip, self.popup.is_some(), Message::Surface, self.core.main_window_id())).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        "".into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

// ---- views ------------------------------------------------------------------

/// The popup card: `#121419` at 58%, 1 px `#262a33` line, radius 18, one single layer.
fn glass(radius: f32, alpha: f32, rgb: [f32; 3]) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |_| widget::container::Style {
        text_color: Some(TEXT),
        background: Some(Background::Color(Color::from_rgba(rgb[0], rgb[1], rgb[2], alpha))),
        border: Border { color: Color::from_rgb(0.149, 0.165, 0.2), width: 1.0, radius: radius.into() },
        ..Default::default()
    })
}

fn t(s: impl Into<String>, size: f32, w: Weight, c: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into()).size(size).font(nunito(w)).class(cosmic::theme::Text::Color(c))
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

fn hex(c: Color) -> String {
    let b = |v: f32| (v * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c.r), b(c.g), b(c.b))
}

fn lift(c: Color, k: f32) -> Color {
    Color { r: c.r + (1.0 - c.r) * k, g: c.g + (1.0 - c.g) * k, b: c.b + (1.0 - c.b) * k, a: (c.a + 0.08).min(1.0) }
}

/// Flat button: `base` fill (transparent = none), lightens on hover/press.
fn btn_class(base: Color, radius: f32) -> cosmic::theme::Button {
    let st = move |c: Color| {
        let mut s = widget::button::Style::new();
        s.background = Some(Background::Color(c));
        s.border_radius = radius.into();
        s.text_color = Some(TEXT);
        s.icon_color = Some(TEXT);
        s
    };
    cosmic::theme::Button::Custom {
        active: Box::new(move |_, _| st(base)),
        disabled: Box::new(move |_| st(base)),
        hovered: Box::new(move |_, _| st(lift(base, 0.12))),
        pressed: Box::new(move |_, _| st(lift(base, 0.2))),
    }
}

fn svg(doc: String, size: f32) -> Element<'static, Message> {
    widget::icon::from_svg_bytes(doc.into_bytes()).icon().size(size as u16).into()
}

fn pill(label: &str, accent: bool, msg: Message) -> Element<'static, Message> {
    let (fg, bg) = if accent { (Color::WHITE, ACCENT) } else { (TEXT, Color::from_rgba(1.0, 1.0, 1.0, 0.10)) };
    widget::button::custom(t(label.to_string(), 12.0, Weight::Bold, fg)).class(btn_class(bg, 14.0)).padding([4, 12]).on_press(msg).into()
}

fn clock_pill(n: Now, size: f32) -> Element<'static, Message> {
    t(clock_text(n), size * 0.8, Weight::Bold, TEXT).into()
}

fn batt_color(s: BattState) -> Color {
    match s {
        BattState::Charging | BattState::Full => GREEN,
        BattState::Low => ORANGE,
        BattState::Critical => RED,
        BattState::Unknown => MUTED,
        BattState::Plugged | BattState::Normal => TEXT,
    }
}

fn vol_svg(o: Option<&(String, f32, bool)>) -> String {
    volume(o.map_or(Vol::Muted, |o| vol_level(o.1, o.2)), &hex(TEXT))
}

fn wifi_svg(w: Option<&WifiSnap>) -> String {
    let c = hex(TEXT);
    match w {
        Some(w) if w.enabled && !w.airplane => wifi(w.nets.iter().find(|n| n.in_use).map_or(0, |n| wifi_bars(n.signal)), false, &c),
        _ => wifi(0, true, &c),
    }
}

fn bt_state_of(b: Option<&BtSnap>) -> BtState {
    b.map_or(BtState::Off, |b| bt_state(b.powered, b.paired.iter().any(|d| d.connected)))
}

fn bt_svg(b: Option<&BtSnap>) -> String {
    let s = bt_state_of(b);
    bluetooth(s, &hex(if s == BtState::Connected { ACCENT } else { TEXT }))
}

fn batt_state_of(b: &BattSnap) -> BattState {
    batt_state(b.pct, b.status.as_deref())
}

fn batt_svg(b: &BattSnap) -> String {
    let s = batt_state_of(b);
    battery(s, b.pct.unwrap_or(0.0), &hex(batt_color(s)))
}

fn tooltip_text(s: &Snap) -> String {
    let w = s.wifi.as_ref();
    let ssid = w.and_then(|w| w.nets.iter().find(|n| n.in_use)).map(|n| n.ssid.as_str());
    let conn: Vec<&str> = s.bt.iter().flat_map(|b| b.paired.iter().filter(|d| d.connected).map(|d| d.name.as_str())).collect();
    let mut lines = vec![vol_tip(s.out.as_ref().map(|o| (o.1, o.2))), wifi_tip(w.map(|w| w.enabled), w.is_some_and(|w| w.airplane), ssid), bt_tip(s.bt.as_ref().map(|b| b.powered), &conn)];
    if let Some(b) = &s.batt {
        lines.push(batt_tip(b.pct, b.status.as_deref(), b.mins));
    }
    lines.join("\n")
}

/// Panel content: bare 20-24px glyphs, 12px spacing, battery % tinted by state.
fn status_row(s: &Snap, g: f32) -> Element<'static, Message> {
    let mut r = row::with_capacity(5).spacing(12).align_y(Alignment::Center);
    r = r.push(svg(vol_svg(s.out.as_ref()), g)).push(svg(wifi_svg(s.wifi.as_ref()), g)).push(svg(bt_svg(s.bt.as_ref()), g));
    if let Some(b) = &s.batt {
        r = r.push(svg(batt_svg(b), g));
        if let Some(p) = b.pct {
            r = r.push(t(format!("{p:.0}%"), g * 0.78, Weight::Bold, batt_color(batt_state_of(b))));
        }
    }
    r.into()
}

fn popup_action<K: Kind>(offset: Vector, bounds: Rectangle) -> cosmic::surface::Action<Message> {
    app_popup::<Applet<K>>(
        |_| Default::default(),
        move |state: &mut Applet<K>| {
            let id = Id::unique();
            state.popup = Some(id);
            let mut s = state.core.applet.get_popup_settings(state.core.main_window_id().unwrap(), id, None, None, None);
            s.positioner.anchor_rect = Rectangle { x: (bounds.x - offset.x) as i32, y: (bounds.y - offset.y) as i32, width: bounds.width as i32, height: bounds.height as i32 };
            s.positioner.size_limits = cosmic::iced::Limits::NONE.min_width(W).max_width(W).min_height(1.0).max_height(POPUP_MAX_H + 36.0);
            s
        },
        Some(Box::new(|state: &Applet<K>| {
            let body = if K::STATUS { status_card(&state.snap, &state.ui) } else { clock_card(state.now) };
            // the status card scrolls inside a capped box so it can never outgrow the screen
            let body: Element<Message> = if K::STATUS { widget::container(widget::scrollable(body)).max_height(POPUP_MAX_H).into() } else { body };
            let framed = widget::container(body).padding(18).width(Length::Fixed(W)).class(glass(18.0, 0.58, [0.071, 0.078, 0.098]));
            let el: Element<Message> = cosmic::widget::autosize::autosize(framed, cosmic::widget::Id::new("cosmic-applet-autosize"))
                .limits(cosmic::iced::Limits::NONE.min_width(W).max_width(W).min_height(1.0).max_height(POPUP_MAX_H + 36.0))
                .into();
            el.map(cosmic::Action::App)
        })),
    )
}

fn clock_card(n: Now) -> Element<'static, Message> {
    let h = if n.hour % 12 == 0 { 12 } else { n.hour % 12 };
    let mut col = column::with_capacity(9).spacing(6).align_x(Alignment::Center);
    col = col.push(t(format!("{h}:{:02} {}", n.minute, if n.hour < 12 { "AM" } else { "PM" }), 40.0, Weight::Black, TEXT));
    col = col.push(t(format!("{} {} {}", DAYS[n.weekday as usize % 7], n.day, MONTHS[n.month as usize - 1]), 14.0, Weight::Bold, MUTED));
    col = col.push(widget::Space::new().height(8));
    let mut head = row::with_capacity(7);
    for d in ["M", "T", "W", "T", "F", "S", "S"] {
        head = head.push(widget::container(t(d, 12.0, Weight::Bold, MUTED)).width(Length::Fixed(32.0)).align_x(Alignment::Center));
    }
    col = col.push(head);
    for week in grid(n.year, n.month) {
        let mut r = row::with_capacity(7);
        for d in week {
            let cell: Element<Message> = match d {
                Some(d) if d == n.day => widget::container(t(d.to_string(), 13.0, Weight::Bold, Color::WHITE))
                    .width(Length::Fixed(28.0))
                    .height(Length::Fixed(28.0))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .class(cosmic::theme::Container::custom(|_| widget::container::Style { background: Some(Background::Color(ACCENT)), border: Border { radius: 14.0.into(), ..Default::default() }, ..Default::default() }))
                    .into(),
                Some(d) => widget::container(t(d.to_string(), 13.0, Weight::Normal, TEXT)).width(Length::Fixed(28.0)).height(Length::Fixed(28.0)).align_x(Alignment::Center).align_y(Alignment::Center).into(),
                None => widget::Space::new().width(Length::Fixed(28.0)).height(Length::Fixed(28.0)).into(),
            };
            r = r.push(widget::container(cell).width(Length::Fixed(32.0)).align_x(Alignment::Center));
        }
        col = col.push(r);
    }
    col.into()
}

/// Collapsible section header: clickable icon + title + summary + chevron, optional toggle on the right.
fn header(sec: Sec, ui: &Ui, icon: String, title: &str, sub: String, toggle: Option<Element<'static, Message>>) -> Element<'static, Message> {
    let label = row::with_capacity(4)
        .spacing(10)
        .align_y(Alignment::Center)
        .push(svg(icon, 18.0))
        .push(column::with_capacity(2).push(t(title.to_string(), 14.0, Weight::Bold, TEXT)).push(t(trunc(&sub, 34), 11.0, Weight::Normal, MUTED)).width(Length::Fill))
        .push(svg(chevron(ui.open == Some(sec), &hex(MUTED)), 16.0));
    let b = widget::button::custom(label).class(btn_class(Color::TRANSPARENT, 10.0)).padding([6, 8]).width(Length::Fill).on_press(Message::Open(sec));
    let mut r = row::with_capacity(2).spacing(8).align_y(Alignment::Center).push(b);
    if let Some(tg) = toggle {
        r = r.push(tg);
    }
    r.into()
}

fn body(c: impl Into<Element<'static, Message>>) -> Element<'static, Message> {
    widget::container(c).padding([2, 8, 8, 8]).width(Length::Fill).into()
}

fn muted(s: &str) -> Element<'static, Message> {
    t(s.to_string(), 12.0, Weight::Normal, MUTED).into()
}

/// icon + name/subtitle + optional lock + action: one list row.
fn list_row(icon: String, name: &str, sub: &str, sub_color: Color, lock_icon: bool, action: Element<'static, Message>) -> Element<'static, Message> {
    let mut r = row::with_capacity(4).spacing(10).align_y(Alignment::Center).push(svg(icon, 16.0));
    r = r.push(column::with_capacity(2).push(t(trunc(name, 22), 13.0, Weight::Bold, TEXT)).push(t(sub.to_string(), 11.0, Weight::Normal, sub_color)).width(Length::Fill));
    if lock_icon {
        r = r.push(svg(lock(&hex(MUTED)), 13.0));
    }
    r.push(action).into()
}

fn wifi_body(w: Option<&WifiSnap>, ui: &Ui) -> Element<'static, Message> {
    let Some(w) = w else { return body(muted("Wi-Fi unavailable")) };
    if !w.enabled || w.airplane {
        return body(muted("Wi-Fi is off"));
    }
    let scan = if ui.rescanning { muted("Scanning…") } else { pill("Rescan", false, Message::Rescan) };
    let mut col = column::with_capacity(w.nets.len() + 2).spacing(8);
    col = col.push(row::with_capacity(2).align_y(Alignment::Center).push(t(format!("{} networks", w.nets.len()), 12.0, Weight::Normal, MUTED).width(Length::Fill)).push(scan));
    for n in &w.nets {
        let (sub, sc) = if n.in_use { ("Connected", GREEN) } else if n.known { ("Saved", MUTED) } else if n.secure { ("Secured", MUTED) } else { ("Open", MUTED) };
        let action = if ui.busy.as_deref() == Some(&n.ssid) {
            muted("Connecting…")
        } else if n.in_use {
            pill("Disconnect", false, Message::WifiDisconnect)
        } else {
            pill("Connect", true, Message::WifiConnect(n.ssid.clone()))
        };
        col = col.push(list_row(wifi(wifi_bars(n.signal), false, &hex(if n.in_use { ACCENT } else { TEXT })), &n.ssid, sub, sc, n.secure, action));
        if let Some(p) = ui.pw.as_ref().filter(|p| p.ssid == n.ssid) {
            let input = widget::text_input("Password", p.pw.0.clone()).password().id(pw_id()).on_input(|s| Message::WifiPw(Secret(s))).on_submit(|_| Message::WifiSubmit).width(Length::Fill);
            col = col.push(row::with_capacity(3).spacing(6).align_y(Alignment::Center).push(input).push(pill("Join", true, Message::WifiSubmit)).push(pill("Cancel", false, Message::WifiCancel)));
        }
    }
    body(col)
}

fn bt_body(b: Option<&BtSnap>, ui: &Ui) -> Element<'static, Message> {
    let Some(b) = b else { return body(muted("Bluetooth unavailable")) };
    if !b.powered {
        return body(muted("Bluetooth is off"));
    }
    let scan = if ui.scanning { muted("Scanning…") } else { pill("Scan", false, Message::BtScan) };
    let mut col = column::with_capacity(b.paired.len() + b.nearby.len() + 3).spacing(8);
    col = col.push(row::with_capacity(2).align_y(Alignment::Center).push(t("Paired", 12.0, Weight::Normal, MUTED).width(Length::Fill)).push(scan));
    if b.paired.is_empty() {
        col = col.push(muted("No paired devices"));
    }
    for d in &b.paired {
        let sub = match (d.connected, d.battery) {
            (true, Some(p)) => format!("Connected · {p}%"),
            (true, None) => "Connected".into(),
            (false, Some(p)) => format!("Paired · {p}%"),
            (false, None) => "Paired".into(),
        };
        let action = if ui.busy.as_deref() == Some(&d.path) {
            muted("Working…")
        } else if d.connected {
            pill("Disconnect", false, Message::BtConnect(d.path.clone(), false))
        } else {
            pill("Connect", true, Message::BtConnect(d.path.clone(), true))
        };
        let st = if d.connected { BtState::Connected } else { BtState::On };
        col = col.push(list_row(bluetooth(st, &hex(if d.connected { ACCENT } else { TEXT })), &d.name, &sub, if d.connected { GREEN } else { MUTED }, false, action));
    }
    if !b.nearby.is_empty() {
        col = col.push(t("Nearby", 12.0, Weight::Normal, MUTED));
    }
    for (mac, name) in &b.nearby {
        let action = if ui.busy.as_deref() == Some(mac) { muted("Pairing…") } else { pill("Pair", true, Message::BtPair(mac.clone())) };
        col = col.push(list_row(bluetooth(BtState::On, &hex(MUTED)), name, "Not paired", MUTED, false, action));
    }
    body(col)
}

fn status_card(s: &Snap, ui: &Ui) -> Element<'static, Message> {
    let mut col = column::with_capacity(9).spacing(4);

    // Sound
    let (vol, muted_) = s.out.as_ref().map_or((0.0, true), |o| (o.1, o.2));
    let sub = if muted_ { "Muted".to_string() } else { format!("{:.0}%", vol * 100.0) };
    col = col.push(header(Sec::Sound, ui, vol_svg(s.out.as_ref()), "Sound", sub, Some(widget::toggler(!muted_).on_toggle(|on| Message::Mute(!on)).into())));
    if ui.open == Some(Sec::Sound) {
        col = col.push(body(row::with_capacity(2).spacing(10).align_y(Alignment::Center).push(widget::slider(0.0..=1.0, vol.min(1.0), Message::Vol).step(0.01).width(Length::Fill)).push(t(format!("{:.0}%", vol * 100.0), 12.0, Weight::Bold, MUTED))));
    }

    // Wi-Fi
    let w = s.wifi.as_ref();
    let sub = match w {
        None => "Unavailable".to_string(),
        Some(w) if !w.enabled || w.airplane => "Off".into(),
        Some(w) => w.nets.iter().find(|n| n.in_use).map_or("Not connected".to_string(), |n| n.ssid.clone()),
    };
    let tg = w.map(|w| widget::toggler(w.enabled && !w.airplane).on_toggle(Message::Wifi).into());
    col = col.push(header(Sec::Wifi, ui, wifi_svg(w), "Wi-Fi", sub, tg));
    if ui.open == Some(Sec::Wifi) {
        col = col.push(wifi_body(w, ui));
    }

    // Bluetooth
    let b = s.bt.as_ref();
    let sub = match b {
        None => "Unavailable".to_string(),
        Some(b) if !b.powered => "Off".into(),
        Some(b) => {
            let c: Vec<&str> = b.paired.iter().filter(|d| d.connected).map(|d| d.name.as_str()).collect();
            if c.is_empty() { "On".into() } else { c.join(", ") }
        }
    };
    let tg = b.map(|b| widget::toggler(b.powered).on_toggle(Message::Bt).into());
    col = col.push(header(Sec::Bt, ui, bt_svg(b), "Bluetooth", sub, tg));
    if ui.open == Some(Sec::Bt) {
        col = col.push(bt_body(b, ui));
    }

    // Battery (read-only)
    if let Some(b) = &s.batt {
        let sub = b.pct.map_or("Unknown".to_string(), |p| format!("{p:.0}%"));
        col = col.push(header(Sec::Batt, ui, batt_svg(b), "Battery", sub, None));
        if ui.open == Some(Sec::Batt) {
            let mut c = column::with_capacity(3).spacing(4);
            c = c.push(t(batt_tip(b.pct, b.status.as_deref(), b.mins), 13.0, Weight::Bold, batt_color(batt_state_of(b))));
            c = c.push(muted(&b.line));
            col = col.push(body(c));
        }
    }

    if let Some(n) = &ui.note {
        col = col.push(widget::container(t(n.clone(), 12.0, Weight::Bold, ORANGE)).padding([4, 8]));
    }
    col.into()
}

fn main() -> cosmic::iced::Result {
    match std::env::args().nth(1).as_deref() {
        Some("clock") => cosmic::applet::run::<Applet<Clock>>(()),
        Some("status") => cosmic::applet::run::<Applet<Status>>(()),
        _ => {
            eprintln!("usage: glasskit-applets <clock|status>");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_format() {
        let n = Now { year: 2026, month: 11, day: 18, hour: 1, minute: 10, weekday: 3 };
        assert_eq!(clock_text(n), "Wed 18 Nov  1:10 AM");
        assert_eq!(clock_text(Now { hour: 12, ..n }), "Wed 18 Nov  12:10 PM");
    }

    #[test]
    fn monday_first_grid() {
        let g = grid(2026, 11); // 1 Nov 2026 is a Sunday
        assert_eq!(g[0][6], Some(1));
        assert_eq!(g[1][0], Some(2));
    }

    #[test]
    fn colours_and_helpers() {
        assert_eq!(hex(GREEN), "#34d399");
        assert_eq!(trunc("short", 10), "short");
        assert_eq!(trunc("a very long network name", 10), "a very lo…");
        assert_eq!(format!("{:?}", Secret("hunter22".into())), "Secret(..)");
        assert_eq!(batt_color(BattState::Critical), RED);
    }
}
