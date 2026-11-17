// SPDX-License-Identifier: GPL-3.0-or-later
//! Media card: art tile, title/artist, progress bar with elapsed/total, icon transport.
//! ember_core's `Player` only has title/artist/playing; position, length, art and seeking are read
//! here straight from the same MPRIS player (zbus, same bus name `media::read` picked).
use std::collections::HashMap;
use std::time::{Duration, Instant};

use cosmic::iced::advanced::text::Wrapping;
use cosmic::iced::widget::canvas::{self, Frame, Geometry, Path};
use cosmic::iced::{Alignment, Color, ContentFit, Length, Point, Rectangle, Size, mouse};
use cosmic::widget::{self, button, container, image, svg, text};
use cosmic::{Element, Renderer, Theme};
use ember_core::control::media::{self, Player};
use zbus::proxy::{Builder, CacheProperties};
use zbus::{ObjectPath, OwnedVariant, Variant};

use crate::style::*;
use crate::{Message, sc};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const IFACE: &str = "org.mpris.MediaPlayer2.Player";

// ---------------------------------------------------------------- MPRIS data

/// Extra MPRIS state `ember_core::control::media::Player` doesn't carry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extra {
    pub position_us: i64,
    pub length_us: Option<i64>,
    pub art_url: Option<String>,
    pub track_id: Option<String>,
    pub can_seek: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct Meta {
    pub length_us: Option<i64>,
    pub art_url: Option<String>,
    pub track_id: Option<String>,
}

/// Integer-ish D-Bus value (players disagree on i64/u64/i32) -> i64.
fn num(v: &Variant) -> Option<i64> {
    match v {
        Variant::I64(x) => Some(*x),
        Variant::U64(x) => i64::try_from(*x).ok(),
        Variant::I32(x) => Some(i64::from(*x)),
        Variant::U32(x) => Some(i64::from(*x)),
        Variant::F64(x) => Some(*x as i64),
        Variant::Variant(b) => num(b),
        _ => None,
    }
}

/// String or object path -> String.
fn string(v: &Variant) -> Option<String> {
    match v {
        Variant::Str(s) => Some(s.as_str().to_owned()),
        Variant::ObjectPath(p) => Some(p.as_str().to_owned()),
        Variant::Variant(b) => string(b),
        _ => None,
    }
}

/// `mpris:length`, `mpris:artUrl`, `mpris:trackid` out of a Metadata dict. A length of 0 (streams) is "unknown".
pub fn parse_meta(m: &HashMap<String, OwnedVariant>) -> Meta {
    let get = |k: &str| m.get(k).map(|v| &**v);
    Meta {
        length_us: get("mpris:length").and_then(num).filter(|l| *l > 0),
        art_url: get("mpris:artUrl").and_then(string).filter(|s| !s.is_empty()),
        track_id: get("mpris:trackid").and_then(string).filter(|s| s.starts_with('/') && !s.ends_with("/NoTrack")),
    }
}

/// Poll the shown player plus its position/length/art. Position comes from `Properties.Get` (uncached
/// proxy: it does not emit PropertiesChanged, so a cached read would be stale).
pub async fn read() -> (Option<Player>, Option<Extra>) {
    let Some(p) = media::read().await.ok().flatten() else { return (None, None) };
    let extra = read_extra(&p.bus).await;
    (Some(p), extra)
}

async fn proxy(conn: &zbus::Connection, bus: &str) -> Option<zbus::Proxy<'static>> {
    if !bus.starts_with(PREFIX) || bus.contains('/') {
        return None;
    }
    Builder::new(conn).destination(bus.to_owned()).path(PATH).interface(IFACE).cache_properties(CacheProperties::No).build().await.ok()
}

async fn read_extra(bus: &str) -> Option<Extra> {
    let conn = zbus::Connection::session().await.ok()?;
    let p = proxy(&conn, bus).await?;
    let meta: HashMap<String, OwnedVariant> = p.get_property("Metadata").await.unwrap_or_default();
    let m = parse_meta(&meta);
    Some(Extra {
        position_us: p.get_property::<i64>("Position").await.unwrap_or(0),
        length_us: m.length_us,
        art_url: m.art_url,
        track_id: m.track_id,
        can_seek: p.get_property::<bool>("CanSeek").await.unwrap_or(false),
    })
}

/// SetPosition(trackid, us). Safe: the player ignores it when the track changed meanwhile.
pub async fn seek(bus: String, track_id: String, to_us: i64) {
    let Ok(conn) = zbus::Connection::session().await else { return };
    let Some(p) = proxy(&conn, &bus).await else { return };
    let Ok(tid) = ObjectPath::try_from(track_id) else { return };
    let _ = p.call_method("SetPosition", &(tid, to_us)).await;
}

// ---------------------------------------------------------------- pure helpers

/// Position now: last polled position plus the time since, only while playing, clamped to the length.
pub fn elapsed_us(pos_us: i64, len_us: Option<i64>, playing: bool, since: Duration) -> i64 {
    let p = pos_us.max(0) + if playing { since.as_micros() as i64 } else { 0 };
    len_us.map_or(p, |l| p.min(l))
}

/// Bar fill 0..=1; `None` (indeterminate: empty bar, no times) when the length is unknown.
pub fn progress(pos_us: i64, len_us: Option<i64>, playing: bool, since: Duration) -> Option<f32> {
    let l = len_us.filter(|l| *l > 0)?;
    Some((elapsed_us(pos_us, Some(l), playing, since) as f64 / l as f64).clamp(0.0, 1.0) as f32)
}

/// m:ss, or h:mm:ss from one hour.
pub fn fmt_time(us: i64) -> String {
    let s = us.max(0) / 1_000_000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// Click at fraction `f` of the bar -> target position in microseconds.
pub fn seek_target(f: f32, len_us: i64) -> i64 {
    (f.clamp(0.0, 1.0) as f64 * len_us as f64) as i64
}

// ---------------------------------------------------------------- icons

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Prev,
    Play,
    Pause,
    Next,
}

/// The toggle shows the action a press performs: pause while playing, play otherwise.
pub fn toggle_glyph(playing: bool) -> Glyph {
    if playing { Glyph::Pause } else { Glyph::Play }
}

impl Glyph {
    /// Material Icons Round (filled, rounded), 24x24.
    fn path(self) -> &'static str {
        match self {
            Glyph::Prev => "M7 6c.55 0 1 .45 1 1v10c0 .55-.45 1-1 1s-1-.45-1-1V7c0-.55.45-1 1-1zm3.66 6.82l5.77 4.07c.66.47 1.58-.01 1.58-.82V7.93c0-.81-.92-1.29-1.58-.82l-5.77 4.07c-.57.4-.57 1.24 0 1.64z",
            Glyph::Next => "M7.58 16.89l5.77-4.07c.56-.4.56-1.24 0-1.63L7.58 7.11C6.91 6.65 6 7.12 6 7.93v8.14c0 .81.91 1.28 1.58.82zM16 7v10c0 .55.45 1 1 1s1-.45 1-1V7c0-.55-.45-1-1-1s-1 .45-1 1z",
            Glyph::Play => "M8 6.82v10.36c0 .79.87 1.27 1.54.84l8.14-5.18c.62-.39.62-1.29 0-1.69L9.54 5.98C8.87 5.55 8 6.03 8 6.82z",
            Glyph::Pause => "M8 19c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2s-2 .9-2 2v10c0 1.1.9 2 2 2zm6-12v10c0 1.1.9 2 2 2s2-.9 2-2V7c0-1.1-.9-2-2-2s-2 .9-2 2z",
        }
    }
    /// Inline SVG with an explicit fill (no theme recolouring).
    pub fn svg(self, fill: &str, alpha: f32) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="{fill}" fill-opacity="{alpha}" d="{}"/></svg>"#, self.path())
    }
}

fn glyph<'a>(g: Glyph, fill: &str, alpha: f32, px: f32) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(g.svg(fill, alpha).into_bytes())).width(px).height(px).into()
}

/// Round/flat icon button: `hit` px square (scaled), `icon` px glyph, optional filled `bg` circle.
fn icon_button<'a>(g: Glyph, hit: f32, icon: f32, fill: &str, bg: Option<Color>, on: Option<Message>) -> Element<'a, Message> {
    let alpha = if on.is_some() { 1.0 } else { 0.45 };
    let style = move |hover: bool, enabled: bool| {
        let mut s = widget::button::Style::new();
        s.background = match (bg, hover) {
            (Some(c), _) => Some(cosmic::iced::Background::Color(Color { a: if enabled { c.a } else { 0.3 }, ..c })),
            (None, true) => Some(cosmic::iced::Background::Color(TRACK)),
            _ => None,
        };
        s.border_radius = (sc(hit) / 2.0).into();
        s
    };
    button::custom(container(glyph(g, fill, alpha, sc(icon))).center(Length::Fill))
        .width(sc(hit))
        .height(sc(hit))
        .padding(0)
        .on_press_maybe(on)
        .class(cosmic::theme::Button::Custom {
            active: Box::new(move |_, _| style(false, true)),
            hovered: Box::new(move |_, _| style(true, true)),
            pressed: Box::new(move |_, _| style(true, true)),
            disabled: Box::new(move |_| style(false, false)),
        })
        .into()
}

// ---------------------------------------------------------------- progress bar

struct Bar {
    frac: Option<f32>,
    seekable: bool,
}

impl canvas::Program<Message, Theme, Renderer> for Bar {
    type State = ();
    fn update(&self, _: &mut (), e: &canvas::Event, b: Rectangle, c: mouse::Cursor) -> Option<canvas::Action<Message>> {
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = e {
            if self.seekable && self.frac.is_some() {
                let p = c.position_in(b)?;
                return Some(canvas::Action::publish(Message::Seek(p.x / b.width)).and_capture());
            }
        }
        None
    }
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, b: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, b.size());
        let h = sc(6.0).max(2.0);
        let y = (b.height - h) / 2.0;
        f.fill(&Path::rounded_rectangle(Point::new(0.0, y), Size::new(b.width, h), (h / 2.0).into()), TRACK);
        if let Some(fr) = self.frac.filter(|f| *f > 0.0) {
            let w = (b.width * fr).max(h);
            f.fill(&Path::rounded_rectangle(Point::new(0.0, y), Size::new(w, h), (h / 2.0).into()), ACCENT);
        }
        vec![f.into_geometry()]
    }
    fn mouse_interaction(&self, _: &(), b: Rectangle, c: mouse::Cursor) -> mouse::Interaction {
        if self.seekable && self.frac.is_some() && c.is_over(b) { mouse::Interaction::Pointer } else { mouse::Interaction::default() }
    }
}

// ---------------------------------------------------------------- state + view

pub struct MediaState {
    pub player: Option<Player>,
    pub extra: Option<Extra>,
    pub polled: Instant,
    pub art_url: Option<String>,
    art: Option<image::Handle>,
}

impl MediaState {
    pub fn new() -> Self {
        Self { player: None, extra: None, polled: Instant::now(), art_url: None, art: None }
    }

    /// Store a poll result. Returns the art URL to load when it changed.
    pub fn apply(&mut self, p: Option<Player>, e: Option<Extra>) -> Option<String> {
        self.player = p;
        self.extra = e;
        self.polled = Instant::now();
        let url = self.extra.as_ref().and_then(|e| e.art_url.clone());
        if url == self.art_url {
            return None;
        }
        self.art_url = url.clone();
        self.art = None;
        url
    }

    pub fn set_art(&mut self, url: &str, h: Option<image::Handle>) {
        if self.art_url.as_deref() == Some(url) {
            self.art = h;
        }
    }

    /// Whether the bar is moving (drives the ~4 fps redraw subscription).
    pub fn moving(&self) -> bool {
        self.player.as_ref().is_some_and(|p| p.playing) && self.extra.as_ref().is_some_and(|e| e.length_us.is_some())
    }

    /// (bus, track id, target us) for a click at `f` if the player can seek; also moves the local position.
    pub fn seek(&mut self, f: f32) -> Option<(String, String, i64)> {
        let (p, e) = (self.player.as_ref()?, self.extra.as_mut()?);
        let (len, tid) = (e.length_us?, e.track_id.clone()?);
        if !e.can_seek {
            return None;
        }
        let to = seek_target(f, len);
        e.position_us = to;
        self.polled = Instant::now();
        Some((p.bus.clone(), tid, to))
    }

    pub fn view(&self) -> Element<'_, Message> {
        let on = self.player.is_some();
        let playing = self.player.as_ref().is_some_and(|p| p.playing);
        let (title, artist) = self.player.as_ref().map_or((String::new(), String::new()), |p| (if p.title.is_empty() { p.app.clone() } else { p.title.clone() }, p.artist.clone()));
        let since = self.polled.elapsed();
        let (pos, len) = self.extra.as_ref().map_or((0, None), |e| (e.position_us, e.length_us));
        let frac = progress(pos, len, playing, since);
        let seekable = self.extra.as_ref().is_some_and(|e| e.can_seek && e.track_id.is_some());

        let art_px = sc(96.0);
        let art: Element<Message> = match &self.art {
            Some(h) => image(h.clone()).width(art_px).height(art_px).content_fit(ContentFit::Cover).border_radius([sc(18.0); 4]).into(),
            None => container(widget::Space::new()).width(art_px).height(art_px).class(cosmic::theme::Container::custom(|_| tile_style())).into(),
        };

        let line = |s: String, bold: bool, size: f32, color: Color| {
            container(text(s).font(if bold { FONT_BOLD } else { FONT }).size(sc(size)).wrapping(Wrapping::None).class(cosmic::theme::Text::Color(color))).width(Length::Fill).clip(true)
        };
        // times flank the bar; a fixed width per track length class keeps the bar from jittering
        let tw = sc(if len.is_some_and(|l| l >= 3_600_000_000) { 44.0 } else { 30.0 });
        let time = |s: String, right: bool| -> Element<Message> {
            let t = text(s).font(FONT).size(sc(12.0)).class(cosmic::theme::Text::Color(MUTED));
            container(t).width(tw).align_x(if right { Alignment::End } else { Alignment::Start }).into()
        };
        let mut bar = widget::Row::new().spacing(sc(8.0)).align_y(Alignment::Center);
        if let Some(l) = len {
            bar = bar.push(time(fmt_time(elapsed_us(pos, Some(l), playing, since)), false));
        }
        bar = bar.push(widget::canvas(Bar { frac, seekable }).width(Length::Fill).height(sc(16.0)));
        if let Some(l) = len {
            bar = bar.push(time(fmt_time(l), true));
        }

        let flat_hex = if on { "#f4f5f7" } else { "#8b93a3" };
        let accent = if on { ACCENT } else { Color { a: 0.35, ..ACCENT } };
        let ctl = |c| on.then_some(Message::MediaCmd(c));
        let buttons = widget::Row::new()
            .spacing(sc(12.0))
            .align_y(Alignment::Center)
            .push(icon_button(Glyph::Prev, 36.0, 24.0, flat_hex, None, ctl(media::Cmd::Previous)))
            .push(icon_button(toggle_glyph(playing), 44.0, 28.0, "#ffffff", Some(accent), ctl(media::Cmd::PlayPause)))
            .push(icon_button(Glyph::Next, 36.0, 24.0, flat_hex, None, ctl(media::Cmd::Next)));

        let info = widget::Column::new()
            .spacing(sc(2.0))
            .width(Length::Fill)
            .push(line(title, true, 16.0, TEXT))
            .push(line(artist, false, 13.0, MUTED))
            .push(widget::Space::new().height(Length::Fill))
            .push(bar)
            .push(container(buttons).width(Length::Fill).center_x(Length::Fill));
        widget::Row::new().spacing(sc(16.0)).height(Length::Fill).align_y(Alignment::Center).push(art).push(info).into()
    }
}

// ---------------------------------------------------------------- album art

/// `file:///a%20b.png` -> `/a b.png`. http(s) (or anything else) -> None: no network fetch here.
pub fn art_path(url: &str) -> Option<std::path::PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if let Some(v) = rest.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).ok().map(Into::into)
}

/// Centre-crop to a square and box-average down to at most `max` px. RGBA in, RGBA out.
pub fn square_thumb(w: usize, h: usize, rgba: &[u8], max: usize) -> Option<(u32, Vec<u8>)> {
    let s = w.min(h);
    if s == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let n = s.min(max);
    let (x0, y0) = ((w - s) / 2, (h - s) / 2);
    let mut out = Vec::with_capacity(n * n * 4);
    for j in 0..n {
        let (ya, yb) = (j * s / n, ((j + 1) * s / n).max(j * s / n + 1));
        for i in 0..n {
            let (xa, xb) = (i * s / n, ((i + 1) * s / n).max(i * s / n + 1));
            let mut acc = [0u32; 4];
            for y in ya..yb {
                for x in xa..xb {
                    let o = ((y0 + y) * w + x0 + x) * 4;
                    for c in 0..4 {
                        acc[c] += u32::from(rgba[o + c]);
                    }
                }
            }
            let k = ((yb - ya) * (xb - xa)) as u32;
            out.extend(acc.map(|a| (a / k) as u8));
        }
    }
    Some((n as u32, out))
}

fn decode_png(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut d = png::Decoder::new(std::io::Cursor::new(data));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()];
    let f = r.next_frame(&mut buf).ok()?;
    let px = &buf[..f.buffer_size()];
    let rgba = match f.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((f.width as usize, f.height as usize, rgba))
}

fn decode_jpeg(data: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut d = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(data));
    let px = d.decode().ok()?; // RGB
    let i = d.info()?;
    let (w, h) = (i.width as usize, i.height as usize);
    (px.len() >= w * h * 3).then(|| (w, h, px.chunks_exact(3).take(w * h).flat_map(|p| [p[0], p[1], p[2], 255]).collect()))
}

/// Decode a local PNG/JPEG into a 256px square thumbnail. Remote (http/https) art is not fetched.
pub async fn load_art(url: String) -> Option<image::Handle> {
    let path = art_path(&url)?;
    tokio::task::spawn_blocking(move || {
        if std::fs::metadata(&path).ok()?.len() > 32 << 20 {
            return None;
        }
        let data = std::fs::read(path).ok()?;
        let (w, h, rgba) = if data.starts_with(&[0x89, b'P', b'N', b'G']) {
            decode_png(&data)?
        } else if data.starts_with(&[0xff, 0xd8]) {
            decode_jpeg(&data)?
        } else {
            return None;
        };
        let (n, px) = square_thumb(w, h, &rgba, 256)?;
        Some(image::Handle::from_rgba(n, n, px))
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    fn ov(v: Variant<'static>) -> OwnedVariant {
        OwnedVariant::try_from(v).unwrap()
    }

    #[test]
    fn progress_playing_interpolates_and_clamps() {
        let len = Some(100 * 1_000_000);
        assert_eq!(progress(0, len, true, Duration::ZERO), Some(0.0));
        assert_eq!(progress(25_000_000, len, true, 25 * S), Some(0.5));
        assert_eq!(progress(90_000_000, len, true, 60 * S), Some(1.0)); // past the end
        assert_eq!(progress(-5, len, false, Duration::ZERO), Some(0.0));
        assert_eq!(elapsed_us(90_000_000, len, true, 60 * S), 100_000_000);
    }

    #[test]
    fn progress_paused_does_not_advance() {
        assert_eq!(progress(25_000_000, Some(100_000_000), false, 30 * S), Some(0.25));
        assert_eq!(elapsed_us(25_000_000, Some(100_000_000), false, 30 * S), 25_000_000);
    }

    #[test]
    fn progress_unknown_length_is_indeterminate() {
        assert_eq!(progress(5_000_000, None, true, S), None);
        assert_eq!(progress(5_000_000, Some(0), true, S), None);
    }

    #[test]
    fn time_format() {
        assert_eq!(fmt_time(0), "0:00");
        assert_eq!(fmt_time(65_900_000), "1:05");
        assert_eq!(fmt_time(3_599_000_000), "59:59");
        assert_eq!(fmt_time(3_600_000_000), "1:00:00");
        assert_eq!(fmt_time(3_725_000_000), "1:02:05");
        assert_eq!(fmt_time(-5), "0:00");
    }

    #[test]
    fn seek_maps_fraction_to_position() {
        assert_eq!(seek_target(0.5, 200_000_000), 100_000_000);
        assert_eq!(seek_target(2.0, 200_000_000), 200_000_000);
        assert_eq!(seek_target(-1.0, 200_000_000), 0);
    }

    #[test]
    fn icon_choice() {
        assert_eq!(toggle_glyph(true), Glyph::Pause);
        assert_eq!(toggle_glyph(false), Glyph::Play);
        let s = Glyph::Play.svg("#ffffff", 0.5);
        assert!(s.starts_with("<svg") && s.contains(r##"fill="#ffffff""##) && s.contains(r#"fill-opacity="0.5""#));
        assert_ne!(Glyph::Prev.path(), Glyph::Next.path());
    }

    #[test]
    fn meta_parsing() {
        let mut m = HashMap::new();
        m.insert("mpris:length".to_string(), ov(Variant::I64(215_000_000)));
        m.insert("mpris:artUrl".to_string(), ov(Variant::Str("file:///tmp/a%20b.png".into())));
        m.insert("mpris:trackid".to_string(), ov(Variant::ObjectPath(ObjectPath::try_from("/org/x/track/7").unwrap())));
        let p = parse_meta(&m);
        assert_eq!(p, Meta { length_us: Some(215_000_000), art_url: Some("file:///tmp/a%20b.png".into()), track_id: Some("/org/x/track/7".into()) });
        // u64 length (some players), string trackid, NoTrack and zero length are handled
        let mut m = HashMap::new();
        m.insert("mpris:length".to_string(), ov(Variant::U64(9_000_000)));
        m.insert("mpris:trackid".to_string(), ov(Variant::Str("/org/mpris/MediaPlayer2/TrackList/NoTrack".into())));
        assert_eq!(parse_meta(&m), Meta { length_us: Some(9_000_000), art_url: None, track_id: None });
        m.insert("mpris:length".to_string(), ov(Variant::I64(0)));
        assert_eq!(parse_meta(&m).length_us, None);
        assert_eq!(parse_meta(&HashMap::new()), Meta::default());
    }

    #[test]
    fn art_urls() {
        assert_eq!(art_path("file:///tmp/a%20b.png").unwrap().to_str(), Some("/tmp/a b.png"));
        assert_eq!(art_path("file://localhost/tmp/c.jpg").unwrap().to_str(), Some("/tmp/c.jpg"));
        assert_eq!(art_path("file:///tmp/100%.png").unwrap().to_str(), Some("/tmp/100%.png"));
        assert!(art_path("https://i.scdn.co/image/abc").is_none());
        assert!(art_path("http://x/y.png").is_none());
        assert!(art_path("file://host/x").is_none());
    }

    #[test]
    fn thumb_crops_and_averages() {
        // 4x2 image: left half black, right half white -> 2x2 centre crop = black|white, then 1px thumb = grey
        let mut img = vec![];
        for _ in 0..2 {
            for x in 0..4 {
                let v = if x < 2 { 0 } else { 255 };
                img.extend([v, v, v, 255]);
            }
        }
        let (n, t) = square_thumb(4, 2, &img, 8).unwrap();
        assert_eq!((n, t.len()), (2, 16)); // crop x=1..3: black col, white col
        assert_eq!(&t[..4], &[0, 0, 0, 255]);
        assert_eq!(&t[4..8], &[255, 255, 255, 255]);
        let (n, t) = square_thumb(4, 2, &img, 1).unwrap();
        assert_eq!((n, &t[..]), (1, &[127u8, 127, 127, 255][..]));
        assert!(square_thumb(0, 0, &[], 8).is_none());
    }
}
