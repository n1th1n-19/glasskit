// SPDX-License-Identifier: GPL-3.0-or-later
//! glasskit-launcher: floating glass app launcher. One wlr overlay layer surface (transparent,
//! full-output so a click outside the card closes it) with the card centred on screen.
//! Two tabs: All apps and Favourites (pinned, stored in ~/.local/state/glasskit-launcher/pinned).
//! Run again while open to close it (toggle), so a plain-Super shortcut opens and closes it.
mod apps;

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use apps::{App, Tab};
use cosmic::app::{Core, Settings, Task};
use cosmic::cctk::sctk::output::OutputInfo;
use cosmic::cctk::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use cosmic::iced::advanced::text::{Alignment, Ellipsize, EllipsizeHeightLimit};
use cosmic::iced::event::wayland::{Event as WlEvent, LayerEvent, OutputEvent};
use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{IcedOutput, SctkLayerSurfaceSettings};
use cosmic::iced::platform_specific::shell::commands::layer_surface::get_layer_surface;
use cosmic::iced::widget::scrollable::{self, AbsoluteOffset, Viewport};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment as VAlign, Background, Border, Color, Event, Length, Subscription};
use cosmic::widget::{self, button, container, icon, mouse_area, text, text_input};
use cosmic::Element;

const COLS: usize = 6;
const CARD_W: f32 = 1040.0;
const CARD_H: f32 = 700.0;
const PAD: f32 = 28.0;
const SEARCH_H: f32 = 44.0;
const GAP: f32 = 8.0;
const TILE_H: f32 = 132.0;
const STAR: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="#3b82f6" stroke="#3b82f6" stroke-width="1.5" stroke-linejoin="round" d="M12 2.6l2.9 6.1 6.7.8-4.9 4.6 1.3 6.6L12 17.4l-6 3.3 1.3-6.6L2.4 9.5l6.7-.8z"/></svg>"##;

const fn rgb(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}
const CARD_BG: Color = rgb(0x12, 0x14, 0x19, 0.72);
const LINE: Color = rgb(0x26, 0x2a, 0x33, 1.0);
const TRACK: Color = rgb(0x22, 0x26, 0x2e, 0.8);
const TEXT: Color = rgb(0xf4, 0xf5, 0xf7, 1.0);
const MUTED: Color = rgb(0x8b, 0x93, 0xa3, 1.0);
const ACCENT: Color = rgb(0x3b, 0x82, 0xf6, 1.0);
const FONT: cosmic::iced::Font = cosmic::iced::Font::with_name("Nunito");

/// Layout is designed for 1080p; `sc` scales every dimension by the output's factor (same as glasskit-widgets).
static SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0f32
fn sc(x: f32) -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed)) * x
}
/// s = clamp(min(h/1080, w/1920), 0.5, 1.5)
fn scale_for(w: i32, h: i32) -> f32 {
    (h as f32 / 1080.0).min(w as f32 / 1920.0).clamp(0.5, 1.5)
}
fn scale_of(i: &OutputInfo) -> Option<f32> {
    let (w, h) = i.logical_size.filter(|s| s.0 > 0 && s.1 > 0).or_else(|| i.modes.iter().find(|m| m.current).map(|m| m.dimensions))?;
    Some(scale_for(w, h))
}

#[derive(Debug, Clone)]
enum Message {
    Output(Option<f32>),
    Resized(cosmic::iced::Size),
    Tick,
    Query(String),
    Key(Named),
    Pin,
    SetTab(Tab),
    TogglePin(usize),
    Launch(usize),
    Scrolled(Viewport),
    Focus(bool),
    Close,
    Noop,
}

struct Launcher {
    core: Core,
    apps: Vec<App>,
    recent: Vec<String>,
    pins: Vec<String>,
    tab: Tab,
    query: String,
    shown: Vec<usize>,
    sel: usize,
    made: bool,
    focused: bool,
    ticks: u32,
    offset: f32,
    view_h: f32,
    input: widget::Id,
    list: widget::Id,
}

impl Launcher {
    fn make(&mut self, scale: f32) -> Task<Message> {
        if std::mem::replace(&mut self.made, true) {
            return Task::none();
        }
        SCALE.store(scale.to_bits(), Ordering::Relaxed);
        self.view_h = sc(CARD_H - 2.0 * PAD - SEARCH_H - 16.0);
        Task::batch([
            get_layer_surface(SctkLayerSurfaceSettings {
                id: Id::unique(),
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                anchor: Anchor::all(),
                // ponytail: Active = the output with the pointer/focus; the scale comes from the first output
                // seen, so on mixed-DPI multi-monitor it may differ slightly (clamped 0.5-1.5).
                output: IcedOutput::Active,
                namespace: "glasskit-launcher".into(),
                size: Some((None, None)),
                exclusive_zone: -1,
                ..Default::default()
            }),
            text_input::focus(self.input.clone()),
        ])
    }

    fn row_h() -> f32 {
        sc(TILE_H + GAP)
    }

    /// Keep the selected row inside the scroll viewport.
    fn reveal(&mut self) -> Task<Message> {
        let top = (self.sel / COLS) as f32 * Self::row_h();
        let bottom = top + sc(TILE_H);
        let y = if top < self.offset {
            top
        } else if bottom > self.offset + self.view_h {
            bottom - self.view_h
        } else {
            return Task::none();
        };
        self.offset = y;
        scrollable::scroll_to(self.list.clone(), AbsoluteOffset { x: None, y: Some(y) })
    }

    fn refilter(&mut self) {
        self.shown = apps::rank_tab(&self.apps, &self.query, &self.recent, &self.pins, self.tab);
    }

    fn set_tab(&mut self, tab: Tab) -> Task<Message> {
        self.tab = tab;
        self.refilter();
        self.sel = 0;
        self.offset = 0.0;
        scrollable::scroll_to(self.list.clone(), AbsoluteOffset { x: None, y: Some(0.0) })
    }

    fn toggle_pin(&mut self, pos: usize) {
        let Some(&a) = self.shown.get(pos) else { return };
        apps::toggle_pin(&mut self.pins, &self.apps[a].id);
        apps::pins_save(&self.pins);
        self.refilter();
        self.sel = self.sel.min(self.shown.len().saturating_sub(1));
    }

    fn go(&mut self, i: usize) {
        if let Some(&a) = self.shown.get(i) {
            apps::recent_push(&self.apps[a].id);
            apps::launch(&self.apps[a]);
        }
        close();
    }
}

fn pid_file() -> std::path::PathBuf {
    std::env::var("XDG_RUNTIME_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| "/tmp".into()).join("glasskit-launcher.pid")
}

fn close() -> ! {
    let _ = std::fs::remove_file(pid_file());
    std::process::exit(0)
}

/// Toggle: if another instance is alive, SIGTERM it and return true (we should exit).
fn toggle_running() -> bool {
    let f = pid_file();
    if let Some(pid) = std::fs::read_to_string(&f).ok().and_then(|s| s.trim().parse::<u32>().ok()) {
        let alive = std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.starts_with("glasskit-launch"));
        if alive && pid != std::process::id() {
            let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
            let _ = std::fs::remove_file(&f);
            return true;
        }
    }
    let _ = std::fs::write(&f, std::process::id().to_string());
    false
}

fn label(s: impl Into<String>, size: f32, c: Color) -> widget::Text<'static, cosmic::Theme> {
    text(s.into()).font(FONT).size(sc(size)).class(cosmic::theme::Text::Color(c))
}

fn tile_style(selected: bool, hover: bool) -> button::Style {
    let mut s = button::Style::new();
    s.background = match (selected, hover) {
        (true, _) => Some(Background::Color(Color { a: 0.22, ..ACCENT })),
        (false, true) => Some(Background::Color(Color { a: 0.07, ..TEXT })),
        _ => None,
    };
    s.border_radius = sc(16.0).into();
    s.border_width = if selected { sc(1.0).max(1.0) } else { 0.0 };
    s.border_color = Color { a: 0.55, ..ACCENT };
    s.text_color = Some(TEXT);
    s
}

fn icon_for(a: &App, size: f32) -> Element<'static, Message> {
    let px = sc(size).round() as u16;
    if a.icon.starts_with('/') {
        icon::icon(icon::from_path(a.icon.clone().into())).size(px).into()
    } else {
        icon::from_name(a.icon.clone()).size(px).prefer_svg(true).icon().into()
    }
}

impl Launcher {
    fn tile(&self, pos: usize, a: &App) -> Element<'_, Message> {
        let sel = pos == self.sel;
        let body = widget::Column::new()
            .align_x(VAlign::Center)
            .spacing(sc(6.0))
            .push(icon_for(a, 56.0))
            .push(label(a.name.clone(), 14.0, TEXT).width(Length::Fill).align_x(Alignment::Center).ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(2))));
        let mut content = cosmic::iced::widget::Stack::new().push(container(body).center_x(Length::Fill).padding([sc(14.0), sc(8.0)]));
        if self.tab == Tab::All && self.pins.contains(&a.id) {
            let star = cosmic::iced::widget::svg(cosmic::iced::widget::svg::Handle::from_memory(STAR)).width(sc(16.0)).height(sc(16.0));
            content = content.push(container(star).width(Length::Fill).align_x(cosmic::iced::alignment::Horizontal::Right).padding([sc(8.0), sc(10.0)]));
        }
        let tile = button::custom(content)
            .width(Length::FillPortion(1))
            .height(sc(TILE_H))
            .padding(0)
            .on_press(Message::Launch(pos))
            .class(cosmic::theme::Button::Custom {
                active: Box::new(move |_, _| tile_style(sel, false)),
                hovered: Box::new(move |_, _| tile_style(sel, true)),
                pressed: Box::new(move |_, _| tile_style(true, true)),
                disabled: Box::new(move |_| tile_style(false, false)),
            });
        mouse_area(tile).on_right_press(Message::TogglePin(pos)).into()
    }

    fn tabs(&self) -> Element<'_, Message> {
        let pill = |t: Tab, name: &'static str| {
            let on = self.tab == t;
            let st = move || {
                let mut s = button::Style::new();
                s.background = on.then_some(Background::Color(ACCENT));
                s.border_radius = sc(18.0).into();
                s.text_color = Some(if on { Color::WHITE } else { MUTED });
                s
            };
            button::custom(container(label(name, 15.0, if on { Color::WHITE } else { MUTED })).center(Length::Fill))
                .width(sc(112.0))
                .height(sc(36.0))
                .padding(0)
                .on_press(Message::SetTab(t))
                .class(cosmic::theme::Button::Custom { active: Box::new(move |_, _| st()), hovered: Box::new(move |_, _| st()), pressed: Box::new(move |_, _| st()), disabled: Box::new(move |_| st()) })
        };
        container(widget::Row::new().spacing(sc(4.0)).push(pill(Tab::All, "All")).push(pill(Tab::Favs, "Favourites")))
            .padding(sc(4.0))
            .class(cosmic::theme::Container::custom(|_| container::Style {
                background: Some(Background::Color(TRACK)),
                border: Border { color: LINE, width: sc(1.0).max(1.0), radius: sc(SEARCH_H / 2.0).into() },
                ..Default::default()
            }))
            .into()
    }

    fn card(&self) -> Element<'_, Message> {
        let focus_ring = |c: Color, w: f32| move |_: &cosmic::Theme| text_input::Appearance {
            background: Background::Color(TRACK),
            border_radius: sc(SEARCH_H / 2.0).into(),
            border_offset: None,
            border_width: w,
            border_color: c,
            label_color: MUTED,
            placeholder_color: MUTED,
            selected_text_color: TEXT,
            icon_color: None,
            text_color: Some(TEXT),
            selected_fill: Color { a: 0.45, ..ACCENT },
        };
        let search = text_input("Search apps", &self.query)
            .id(self.input.clone())
            .font(FONT)
            .size(sc(16.0))
            .padding([sc(10.0), sc(20.0)])
            .line_height(cosmic::iced::widget::text::LineHeight::Absolute(sc(24.0).into()))
            .width(Length::Fill)
            .on_input(Message::Query)
            .style(cosmic::theme::TextInput::Custom {
                active: Box::new(focus_ring(LINE, sc(1.0).max(1.0))),
                error: Box::new(focus_ring(LINE, sc(1.0).max(1.0))),
                hovered: Box::new(focus_ring(LINE, sc(1.0).max(1.0))),
                focused: Box::new(focus_ring(ACCENT, sc(2.0).max(1.0))),
                disabled: Box::new(focus_ring(LINE, sc(1.0).max(1.0))),
            });
        let search = container(search).height(sc(SEARCH_H)).align_y(VAlign::Center).width(Length::Fill);
        let top = widget::Row::new().spacing(sc(12.0)).align_y(VAlign::Center).push(search).push(self.tabs());

        let mut grid = widget::Column::new().spacing(sc(GAP));
        for (r, chunk) in self.shown.chunks(COLS).enumerate() {
            let mut row = widget::Row::new().spacing(sc(GAP));
            for (c, &i) in chunk.iter().enumerate() {
                row = row.push(self.tile(r * COLS + c, &self.apps[i]));
            }
            for _ in chunk.len()..COLS {
                row = row.push(widget::Space::new().width(Length::FillPortion(1)));
            }
            grid = grid.push(row);
        }
        let body: Element<Message> = if self.shown.is_empty() {
            let msg = if self.tab == Tab::Favs && !self.pins.iter().any(|p| self.apps.iter().any(|a| &a.id == p)) {
                "No favourites yet. Right-click an app or press Ctrl+P to pin."
            } else {
                "No matching apps"
            };
            container(label(msg, 16.0, MUTED)).center(Length::Fill).into()
        } else {
            widget::scrollable(grid).id(self.list.clone()).on_scroll(Message::Scrolled).height(Length::Fill).into()
        };

        let card = container(widget::Column::new().spacing(sc(16.0)).push(top).push(body))
            .padding(sc(PAD))
            .width(sc(CARD_W))
            .height(sc(CARD_H))
            .class(cosmic::theme::Container::custom(|_| container::Style {
                text_color: Some(TEXT),
                background: Some(Background::Color(CARD_BG)),
                border: Border { color: LINE, width: sc(1.0).max(1.0), radius: sc(28.0).into() },
                ..Default::default()
            }));
        // inner area swallows presses so only clicks outside the card reach the backdrop
        let card = mouse_area(card).on_press(Message::Noop);
        mouse_area(container(card).width(Length::Fill).height(Length::Fill).align_x(VAlign::Center).align_y(cosmic::iced::alignment::Vertical::Center))
            .on_press(Message::Close)
            .into()
    }
}

impl cosmic::Application for Launcher {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "io.glasskit.Launcher";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let mut core = core;
        core.set_auto_blur(Default::default()); // no compositor blur behind the launcher
        let apps = apps::load();
        let recent = apps::recent_load();
        let pins = apps::pins_init(&apps, &recent);
        let tab = if pins.iter().any(|p| apps.iter().any(|a| &a.id == p)) { Tab::Favs } else { Tab::All };
        let shown = apps::rank_tab(&apps, "", &recent, &pins, tab);
        let l = Launcher { core, apps, recent, pins, tab, query: String::new(), shown, sel: 0, made: false, focused: false, ticks: 0, offset: 0.0, view_h: 400.0, input: widget::Id::unique(), list: widget::Id::unique() };
        (l, Task::none())
    }

    fn subscription(&self) -> Subscription<Message> {
        let events = cosmic::iced::event::listen_with(|e, _, _| match e {
            Event::PlatformSpecific(cosmic::iced::event::PlatformSpecific::Wayland(WlEvent::Output(OutputEvent::Created(Some(i)), _))) => Some(Message::Output(scale_of(&i))),
            Event::PlatformSpecific(cosmic::iced::event::PlatformSpecific::Wayland(WlEvent::Layer(LayerEvent::Focused, ..))) => Some(Message::Focus(true)),
            Event::PlatformSpecific(cosmic::iced::event::PlatformSpecific::Wayland(WlEvent::Layer(LayerEvent::Unfocused, ..))) => Some(Message::Focus(false)),
            // the surface is full-output, so its real size gives the scale of the output we actually opened on
            Event::Window(cosmic::iced::window::Event::Resized(sz)) if sz.width > 0.0 && sz.height > 0.0 => Some(Message::Resized(sz)),
            Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(k), .. }) => Some(Message::Key(k)),
            Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Character(c), modifiers, .. }) if modifiers.control() && c.eq_ignore_ascii_case("p") => Some(Message::Pin),
            _ => None,
        });
        Subscription::batch([cosmic::iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick), events])
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Resized(sz) => {
                if self.made {
                    SCALE.store(scale_for(sz.width as i32, sz.height as i32).to_bits(), Ordering::Relaxed);
                    self.view_h = sc(CARD_H - 2.0 * PAD - SEARCH_H - 16.0);
                }
            }
            Message::Output(s) => return self.make(s.unwrap_or(1.0)),
            // no output event within ~0.5s: go with scale 1.0
            Message::Tick => {
                self.ticks += 1;
                if self.ticks >= 2 {
                    return self.make(1.0);
                }
            }
            Message::Focus(f) => {
                // close when keyboard focus is lost (only after we have actually had it)
                if f {
                    self.focused = true;
                    return text_input::focus(self.input.clone());
                } else if self.focused {
                    close();
                }
            }
            Message::Query(q) => {
                self.query = q;
                self.refilter();
                self.sel = 0;
                self.offset = 0.0;
                return scrollable::scroll_to(self.list.clone(), AbsoluteOffset { x: None, y: Some(0.0) });
            }
            Message::Key(k) => {
                let n = self.shown.len();
                let sel = self.sel;
                match k {
                    Named::Escape => close(),
                    Named::Enter => self.go(sel),
                    Named::ArrowRight => self.sel = (sel + 1).min(n.saturating_sub(1)),
                    Named::ArrowLeft => self.sel = sel.saturating_sub(1),
                    Named::ArrowDown => self.sel = if sel + COLS < n { sel + COLS } else { sel },
                    Named::ArrowUp => self.sel = sel.saturating_sub(COLS),
                    // Tab / Shift+Tab / Ctrl+Tab all flip between the two tabs
                    Named::Tab => return self.set_tab(if self.tab == Tab::All { Tab::Favs } else { Tab::All }),
                    _ => return Task::none(),
                }
                return self.reveal();
            }
            Message::Pin => self.toggle_pin(self.sel),
            Message::TogglePin(i) => self.toggle_pin(i),
            Message::SetTab(t) => return self.set_tab(t),
            Message::Launch(i) => self.go(i),
            Message::Scrolled(v) => {
                self.offset = v.absolute_offset().y;
                self.view_h = v.bounds().height;
            }
            Message::Close => close(),
            Message::Noop => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        self.card()
    }
    fn view_window(&self, _: Id) -> Element<'_, Message> {
        self.card()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::iced::theme::Style { background_color: Color::TRANSPARENT, text_color: TEXT, icon_color: TEXT })
    }
}

fn main() -> cosmic::iced::Result {
    if toggle_running() {
        return Ok(());
    }
    let settings = Settings::default().no_main_window(true).transparent(true);
    let r = cosmic::app::run::<Launcher>(settings, ());
    let _ = std::fs::remove_file(pid_file());
    r
}

#[cfg(test)]
mod tests {
    use super::scale_for;
    #[test]
    fn scale() {
        assert_eq!(scale_for(1920, 1080), 1.0);
        assert!((scale_for(1366, 768) - 0.7114).abs() < 1e-3);
        assert_eq!(scale_for(3840, 2160), 1.5);
    }
}
