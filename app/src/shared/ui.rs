//! The few controls onboarding and the settings window share: buttons, key
//! caps and a switch. Brand tokens only (docs/brand/BRAND.md).

use super::theme::{self, palette, radius, size, space};
use iced::widget::{Space, button, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length};

/// The one peach action on a screen.
pub fn primary<'a, M: Clone + 'a>(label: &'a str, on_press: M, alpha: f32) -> Element<'a, M> {
    let p = palette();
    let ink = fade(p.on_accent, alpha);
    let (rest, hover, pressed) = (p.accent, p.accent_hover, p.accent_pressed);
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center().width(Length::Fill))
        .on_press(on_press)
        .padding([space::S3, space::S4])
        .width(200)
        .style(move |_, status| {
            // Hover is instant and subtle (it happens constantly); press darkens a touch more.
            let fill = match status {
                button::Status::Hovered => hover,
                button::Status::Pressed => pressed,
                _ => rest,
            };
            button::Style {
                background: Some(Background::Color(fade(fill, alpha))),
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
    let (hover, pressed, rest, ink) = (p.hover, p.pressed, p.text_muted, p.text);
    button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center())
        .on_press(on_press)
        .padding(space::S3)
        .style(move |_, status| {
            let (background, color) = match status {
                button::Status::Hovered => (Some(Background::Color(hover)), ink),
                button::Status::Pressed => (Some(Background::Color(pressed)), ink),
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
    let edge = match (down, waiting) {
        (true, _) => Color::TRANSPARENT,
        (false, true) => p.edge_strong,
        (false, false) => p.edge,
    };
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

/// An on/off switch: a peach track when on. Only a picture: the row it sits
/// in is the button, so the name and the words under it flip it too.
pub fn switch<'a, M: 'a>(on: bool) -> Element<'a, M> {
    let p = palette();
    let (track, knob) = (if on { p.accent } else { p.elevated }, if on { p.on_accent } else { p.text_muted });
    let dot = container(Space::new()).width(14).height(14).style(move |_| pill_fill(knob));
    let inside =
        if on { row![Space::new().width(Length::Fill), dot] } else { row![dot, Space::new().width(Length::Fill)] };
    // 3 = (20 - 14) / 2: the knob sits centred in the track.
    container(inside).padding(3).width(34).height(20).style(move |_| pill_fill(track)).into()
}

/// Pick one of a few short options, side by side. The picked one sits on a
/// lifted segment; the rest are words until you point at them.
pub fn segmented<'a, M: Clone + 'a, T: Copy + PartialEq + 'a>(
    options: &[(T, &'a str)],
    picked: T,
    on_pick: impl Fn(T) -> M,
) -> Element<'a, M> {
    let p = palette();
    let (track, lifted, ink, rest, hover) = (p.surface, p.elevated, p.text, p.text_muted, p.hover);
    // BRAND.md, Settings window: inset 3, so the segments' radius is 10 - 3.
    let inset = 3.0;
    let segments = row(options.iter().map(|&(value, label)| {
        let on = value == picked;
        button(text(label).size(size::SMALL).font(theme::UI_MEDIUM).center())
            .on_press(on_pick(value))
            // The first label lands on the text edge: the inset plus this makes `space::S3`.
            .padding([space::S1, space::S3 - inset])
            .style(move |_, status| {
                let pointed = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: match (on, pointed) {
                        (true, _) => Some(Background::Color(lifted)),
                        (false, true) => Some(Background::Color(hover)),
                        (false, false) => None,
                    },
                    text_color: if on || pointed { ink } else { rest },
                    border: Border { radius: (radius::CONTROL - inset).into(), ..Border::default() },
                    shadow: if on { theme::shadow_raised() } else { iced::Shadow::default() },
                    ..button::Style::default()
                }
            })
            .into()
    }));
    container(segments)
        .padding(inset)
        .style(move |_| container::Style {
            background: Some(Background::Color(track)),
            border: Border { radius: radius::CONTROL.into(), ..Border::default() },
            ..container::Style::default()
        })
        .into()
}

/// Pick one of many (a mic, a language): a field like a text field that opens
/// a list on `elevated`, one step above the panel it sits on.
/// `on_open` lets the caller refresh what's in it (mics come and go).
pub fn dropdown<'a, M: Clone + 'a, T: Clone + PartialEq + std::fmt::Display + 'a>(
    options: Vec<T>,
    picked: Option<T>,
    on_pick: impl Fn(T) -> M + 'a,
    on_open: M,
) -> Element<'a, M> {
    let p = palette();
    iced::widget::pick_list(options, picked, on_pick)
        .on_open(on_open)
        .text_size(size::SMALL)
        .font(theme::UI_MEDIUM)
        .padding([space::S2, space::S3])
        .width(240)
        .style(move |_, status| iced::widget::pick_list::Style {
            text_color: p.text,
            placeholder_color: p.text_subtle,
            handle_color: p.text_muted,
            background: Background::Color(p.surface),
            border: Border {
                radius: radius::CONTROL.into(),
                width: 1.0,
                color: match status {
                    iced::widget::pick_list::Status::Opened { .. } => fade(p.accent, 0.5),
                    iced::widget::pick_list::Status::Hovered => p.edge_strong,
                    iced::widget::pick_list::Status::Active => p.edge,
                },
            },
        })
        .menu_style(move |_| iced::widget::overlay::menu::Style {
            background: Background::Color(p.elevated),
            border: Border { radius: radius::CONTROL.into(), width: 1.0, color: p.edge },
            text_color: p.text_muted,
            selected_text_color: p.text,
            selected_background: Background::Color(p.hover),
            shadow: theme::shadow_raised(),
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
                p.edge
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
    let (fill, edge) = (p.surface, p.edge);
    container(text(format!("\u{201c}{phrase}\u{201d}")).size(size::CAPTION).font(theme::UI_MEDIUM).color(p.text))
        .padding([space::S1, space::S3])
        .style(move |_| container::Style {
            background: Some(Background::Color(fill)),
            border: Border { radius: 999.0.into(), width: 1.0, color: edge },
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
