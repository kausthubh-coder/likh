//! The few controls onboarding and the settings window share: buttons, key
//! caps and a switch. Brand tokens only (docs/brand/BRAND.md).

use super::theme::{self, palette, radius, size, space};
use iced::widget::{Space, button, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

/// The one peach action on a screen.
pub fn primary<'a, M: Clone + 'a>(label: &'a str, on_press: M, alpha: f32) -> Element<'a, M> {
    let p = palette();
    let (fill, ink) = (fade(p.accent, alpha), fade(p.on_accent, alpha));
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center().width(Length::Fill))
        .on_press(on_press)
        .padding([11, 16])
        .width(200)
        .style(move |_, status| {
            // Hover is instant and subtle (it happens constantly); press darkens a touch more.
            let fill = match status {
                button::Status::Hovered => lighten(fill, 0.04),
                button::Status::Pressed => lighten(fill, -0.04),
                _ => fill,
            };
            button::Style {
                background: Some(Background::Color(fill)),
                text_color: ink,
                border: Border { radius: radius::CONTROL.into(), ..Border::default() },
                ..button::Style::default()
            }
        })
        .into()
}

/// Secondary actions are just words until you point at them.
pub fn ghost<'a, M: Clone + 'a>(label: &'a str, on_press: M, alpha: f32) -> Element<'a, M> {
    let p = palette();
    let (hover, rest, ink) = (p.elevated, p.text_muted, p.text);
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center())
        .on_press(on_press)
        .padding([11, 16])
        .style(move |_, status| {
            let (background, color) = match status {
                button::Status::Hovered => (Some(Background::Color(hover)), ink),
                button::Status::Pressed => (Some(Background::Color(lighten(hover, -0.03))), ink),
                _ => (None, rest),
            };
            button::Style {
                background,
                text_color: fade(color, alpha),
                border: Border { radius: radius::CONTROL.into(), ..Border::default() },
                ..button::Style::default()
            }
        })
        .into()
}

/// One key cap. `down` lights it peach; `waiting` draws an empty, dimmed cap.
pub fn keycap<'a, M: 'a>(label: &'a str, down: bool, waiting: bool) -> Element<'a, M> {
    let p = palette();
    let (fill, ink) = match (down, waiting) {
        (true, _) => (p.accent, p.on_accent),
        (false, true) => (p.bg, p.text_subtle),
        (false, false) => (p.surface, p.text),
    };
    let edge =
        if down { Color::TRANSPARENT } else { Color::from_rgba(1.0, 1.0, 1.0, if waiting { 0.14 } else { 0.06 }) };
    container(text(label).size(size::BODY).font(theme::MONO).color(ink))
        .center_y(56)
        .padding([0.0, space::S5])
        .style(move |_| container::Style {
            background: Some(Background::Color(fill)),
            border: Border { radius: radius::CONTROL.into(), width: 1.0, color: edge },
            ..container::Style::default()
        })
        .into()
}

/// Key caps for a whole shortcut, `+` between them. All lit when `down`.
pub fn keycaps<'a, M: 'a>(parts: &[&'static str], down: bool) -> Element<'a, M> {
    let p = palette();
    let mut caps = row![].spacing(space::S3).align_y(Alignment::Center);
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            caps = caps.push(text("+").size(size::BODY).color(p.text_subtle));
        }
        caps = caps.push(keycap(part, down, false));
    }
    caps.into()
}

/// An on/off switch: a peach track when on.
pub fn switch<'a, M: Clone + 'a>(on: bool, flip: M) -> Element<'a, M> {
    let p = palette();
    let (track, knob) = (if on { p.accent } else { p.elevated }, if on { p.on_accent } else { p.text_muted });
    let dot = container(Space::new()).width(14).height(14).style(move |_| pill_fill(knob));
    let inside =
        if on { row![Space::new().width(Length::Fill), dot] } else { row![dot, Space::new().width(Length::Fill)] };
    button(container(inside).padding(3).width(34).height(20).style(move |_| pill_fill(track)))
        .on_press(flip)
        .padding(0)
        .style(|_, _| button::Style::default())
        .into()
}

/// Pick one of a few short options, side by side. The picked one sits on a
/// lifted segment; the rest are words until you point at them.
pub fn segmented<'a, M: Clone + 'a, T: Copy + PartialEq + 'a>(
    options: &[(T, &'a str)],
    picked: T,
    on_pick: impl Fn(T) -> M,
) -> Element<'a, M> {
    let p = palette();
    let (track, lifted, ink, rest) = (p.surface, p.elevated, p.text, p.text_muted);
    let inset = 3.0;
    let segments = row(options.iter().map(|&(value, label)| {
        let on = value == picked;
        button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center())
            .on_press(on_pick(value))
            .padding([5.0, space::S3])
            .style(move |_, status| button::Style {
                background: on.then_some(Background::Color(lifted)),
                text_color: if on || matches!(status, button::Status::Hovered) { ink } else { rest },
                // Nested inside the track: its radius minus the inset.
                border: Border { radius: (radius::CONTROL - inset).into(), ..Border::default() },
                shadow: if on {
                    iced::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                        offset: iced::Vector::new(0.0, 1.0),
                        blur_radius: 2.0,
                    }
                } else {
                    iced::Shadow::default()
                },
                ..button::Style::default()
            })
            .into()
    }))
    .spacing(2);
    container(segments)
        .padding(inset)
        .style(move |_| container::Style {
            background: Some(Background::Color(track)),
            border: Border { radius: radius::CONTROL.into(), ..Border::default() },
            ..container::Style::default()
        })
        .into()
}

/// A text field on `surface` that lights its edge peach while focused.
pub fn field_style(status: iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    let p = palette();
    iced::widget::text_input::Style {
        background: Background::Color(p.surface),
        border: Border {
            radius: radius::CONTROL.into(),
            width: 1.0,
            color: if matches!(status, iced::widget::text_input::Status::Focused { .. }) {
                fade(p.accent, 0.5)
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.06)
            },
        },
        icon: p.text_subtle,
        placeholder: p.text_subtle,
        value: p.text,
        selection: fade(p.accent, 0.35),
    }
}

/// A phrase you say, as a small capsule in quotes: the pill's shape, at rest.
pub fn said<'a, M: 'a>(phrase: &'a str) -> Element<'a, M> {
    let p = palette();
    let fill = p.surface;
    container(text(format!("\u{201c}{phrase}\u{201d}")).size(size::CAPTION).font(theme::UI_MEDIUM).color(p.text))
        .padding([3.0, space::S2 + 2.0])
        .style(move |_| container::Style {
            background: Some(Background::Color(fill)),
            border: Border { radius: 999.0.into(), width: 1.0, color: Color::from_rgba(1.0, 1.0, 1.0, 0.06) },
            ..container::Style::default()
        })
        .into()
}

pub fn pill_fill(fill: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(fill)),
        border: Border { radius: 999.0.into(), ..Border::default() },
        ..container::Style::default()
    }
}

pub fn fade(color: Color, alpha: f32) -> Color {
    Color { a: color.a * alpha, ..color }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: a.a + (b.a - a.a) * t }
}

pub fn lighten(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r + amount).clamp(0.0, 1.0),
        g: (color.g + amount).clamp(0.0, 1.0),
        b: (color.b + amount).clamp(0.0, 1.0),
        a: color.a,
    }
}
