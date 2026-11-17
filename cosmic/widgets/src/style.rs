//! Design tokens: glass card, Nunito with the system sans as fallback.
use cosmic::iced::widget::container;
use cosmic::iced::{Background, Border, Color, Font, font::Weight};
use cosmic::widget::text;
use cosmic::Element;
use crate::sc;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
}

pub const CARD_BG: Color = Color { r: 20.0 / 255.0, g: 23.0 / 255.0, b: 28.0 / 255.0, a: 0.86 };
pub const LINE: Color = rgb(0x26, 0x2a, 0x33);
pub const TRACK: Color = rgb(0x22, 0x26, 0x2e);
pub const TEXT: Color = rgb(0xf4, 0xf5, 0xf7);
pub const MUTED: Color = rgb(0x8b, 0x93, 0xa3);
pub const ACCENT: Color = rgb(0x3b, 0x82, 0xf6);
pub const GREEN: Color = rgb(0x22, 0xc5, 0x5e);
pub const ORANGE: Color = rgb(0xf9, 0x73, 0x16);
pub const PURPLE: Color = rgb(0xa8, 0x55, 0xf7);

// Nunito isn't bundled; iced falls back to the default sans when the family is missing.
pub const FONT: Font = Font::with_name("Nunito");
pub const FONT_BOLD: Font = Font { weight: Weight::Bold, ..Font::with_name("Nunito") };

pub fn card_style() -> container::Style {
    container::Style {
        text_color: Some(TEXT),
        background: Some(Background::Color(CARD_BG)),
        border: Border { color: LINE, width: sc(1.0).max(1.0), radius: sc(28.0).into() },
        ..Default::default()
    }
}

pub fn tile_style() -> container::Style {
    container::Style { background: Some(Background::Color(TRACK)), border: Border { color: LINE, width: sc(1.0).max(1.0), radius: sc(18.0).into() }, ..Default::default() }
}

pub fn today_style(hot: bool) -> container::Style {
    container::Style { background: hot.then_some(Background::Color(ACCENT)), border: Border { radius: sc(10.0).into(), ..Default::default() }, ..Default::default() }
}

pub fn pill_style(on: bool, color: Color) -> cosmic::widget::button::Style {
    let mut s = cosmic::widget::button::Style::new();
    s.background = Some(Background::Color(if on { color } else { TRACK }));
    s.text_color = Some(TEXT);
    s.border_radius = sc(16.0).into();
    s
}

pub fn muted<'a, M: 'static>(s: impl Into<String>) -> Element<'a, M> {
    text(s.into()).font(FONT).size(sc(13.0)).class(cosmic::theme::Text::Color(MUTED)).into()
}

pub fn body<'a, M: 'static>(s: impl Into<String>) -> Element<'a, M> {
    text(s.into()).font(FONT).size(sc(15.0)).class(cosmic::theme::Text::Color(TEXT)).into()
}

pub fn heading<'a, M: 'static>(s: impl Into<String>) -> Element<'a, M> {
    text(s.into()).font(FONT_BOLD).size(sc(20.0)).class(cosmic::theme::Text::Color(TEXT)).into()
}

