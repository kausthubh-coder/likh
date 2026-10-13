//! Brand tokens from docs/brand/BRAND.md. Views use these names, never raw colors.

use iced::Color;
use iced::font::{Family, Weight};
use iced::{Font, Pixels};

pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub elevated: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_subtle: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    /// The hairline around a surface that sits on another (inputs, key caps, cards).
    pub edge: Color,
    /// A hairline that has to read on its own: an empty key cap waiting for a key.
    pub edge_strong: Color,
    /// Under the pointer, and while pressed. White veils, so they work on `bg` and `surface` alike.
    pub hover: Color,
    pub pressed: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
}

pub fn palette() -> Palette {
    Palette {
        bg: oklch(0.155, 0.012, 275.0),
        surface: oklch(0.19, 0.014, 275.0),
        elevated: oklch(0.225, 0.016, 275.0),
        text: oklch(0.945, 0.006, 275.0),
        text_muted: oklch(0.72, 0.012, 275.0),
        text_subtle: oklch(0.56, 0.014, 275.0),
        accent: oklch(0.78, 0.11, 45.0),
        on_accent: oklch(0.22, 0.03, 45.0),
        success: oklch(0.78, 0.13, 155.0),
        warning: oklch(0.84, 0.14, 85.0),
        danger: oklch(0.68, 0.17, 25.0),
        edge: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
        edge_strong: Color::from_rgba(1.0, 1.0, 1.0, 0.14),
        hover: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
        pressed: Color::from_rgba(1.0, 1.0, 1.0, 0.035),
        accent_hover: oklch(0.81, 0.11, 45.0),
        accent_pressed: oklch(0.75, 0.11, 45.0),
    }
}

/// The tight shadow under a lifted control (BRAND.md, Surface: `0 1 2 black / 0.2`).
pub fn shadow_raised() -> iced::Shadow {
    iced::Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.2), offset: iced::Vector::new(0.0, 1.0), blur_radius: 2.0 }
}

/// Type scale: five sizes and nothing else.
pub mod size {
    pub const CAPTION: f32 = 12.0;
    pub const SMALL: f32 = 13.0;
    pub const BODY: f32 = 15.0;
    pub const TITLE: f32 = 20.0;
    pub const DISPLAY: f32 = 34.0;
}

/// Spacing scale.
#[allow(dead_code, reason = "the whole scale from BRAND.md; steps get used as screens land")]
pub mod space {
    pub const S1: f32 = 4.0;
    pub const S2: f32 = 8.0;
    pub const S3: f32 = 12.0;
    pub const S4: f32 = 16.0;
    pub const S5: f32 = 24.0;
    pub const S6: f32 = 32.0;
    pub const S7: f32 = 48.0;
}

#[allow(dead_code, reason = "the whole set from BRAND.md; WINDOW is for the frameless window in phase 3")]
pub mod radius {
    pub const WINDOW: f32 = 16.0;
    pub const CARD: f32 = 12.0;
    pub const CONTROL: f32 = 10.0;
}

/// The pill draws its own text with this one weight.
pub const GEIST_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Geist-Medium.ttf");

/// Key caps drawn on a canvas measure their labels in this one.
pub const GEIST_MONO: &[u8] = include_bytes!("../../assets/fonts/GeistMono-Medium.ttf");

pub const FONT_FILES: [&[u8]; 4] = [
    include_bytes!("../../assets/fonts/Geist-Regular.ttf"),
    GEIST_MEDIUM,
    include_bytes!("../../assets/fonts/Geist-SemiBold.ttf"),
    GEIST_MONO,
];

pub const UI: Font = Font::with_name("Geist");
pub const UI_MEDIUM: Font = Font { weight: Weight::Medium, ..UI };
pub const UI_SEMIBOLD: Font = Font { weight: Weight::Semibold, ..UI };
pub const MONO: Font = Font { family: Family::Name("Geist Mono"), weight: Weight::Medium, ..Font::DEFAULT };

pub fn line_height(size: f32) -> iced::widget::text::LineHeight {
    let px = match size {
        s if s <= size::CAPTION => 16.0,
        s if s <= size::SMALL => 18.0,
        s if s <= size::BODY => 22.0,
        s if s <= size::TITLE => 26.0,
        _ => 38.0,
    };
    iced::widget::text::LineHeight::Absolute(Pixels(px))
}

/// OKLCH (lightness 0–1, chroma, hue in degrees) to sRGB, clamped to gamut.
pub fn oklch(l: f32, c: f32, h: f32) -> Color {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    Color::from_rgb(encode(r), encode(g), encode(b))
}

fn encode(linear: f32) -> f32 {
    let x = linear.clamp(0.0, 1.0);
    if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Color, b: [f32; 3]) -> bool {
        // About 1.5 of 255: Chromium rounds per channel.
        (a.r - b[0]).abs() < 0.006 && (a.g - b[1]).abs() < 0.006 && (a.b - b[2]).abs() < 0.006
    }

    #[test]
    fn oklch_endpoints_are_black_and_white() {
        assert!(close(oklch(0.0, 0.0, 0.0), [0.0, 0.0, 0.0]));
        assert!(close(oklch(1.0, 0.0, 0.0), [1.0, 1.0, 1.0]));
    }

    #[test]
    fn accent_is_the_warm_peach_from_the_brand_board() {
        // Chromium paints oklch(0.78 0.11 45) as rgb(242, 160, 124).
        assert!(close(palette().accent, [242.0 / 255.0, 160.0 / 255.0, 124.0 / 255.0]));
    }
}
