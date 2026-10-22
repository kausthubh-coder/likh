//! The shortcut at the top of Dictation: its key caps, and a small keyboard
//! with those keys lit. Picking a new one lifts each key you press off the
//! keyboard and lands it in its slot as a cap, and the saved shortcut pulses
//! once (BRAND.md, Settings window). Nothing moves otherwise, so at rest it
//! draws once and asks for no frames.

use crate::shared::spring;
use crate::shared::theme::{self, Palette, radius, size, space};
use ab_glyph::{Font as _, FontRef};
use iced::widget::canvas::{self, Frame, Path, Stroke, Text, stroke::LineDash};
use iced::{Color, Pixels, Point, Rectangle, Renderer, Size, Theme, alignment, border, mouse};

/// One key unit on the little keyboard, and the keys' own size and spacing.
const UNIT: f32 = 11.0;
const KEY_GAP: f32 = 2.0;
const KEY_HEIGHT: f32 = 10.0;
const ROW_STEP: f32 = 12.0;
/// The keyboard is 15 units wide and 5 rows tall.
pub const KEYBOARD: Size = Size::new(15.0 * UNIT - KEY_GAP, 4.0 * ROW_STEP + KEY_HEIGHT);
pub const CAP_HEIGHT: f32 = 36.0;
/// How long a key takes to fly into place, and the saved pulse: its length and the gap between caps.
const LANDING: f32 = 0.6;
const PRESSED: f32 = 0.16;
const PULSE: f32 = 0.36;
const PULSE_STAGGER: f32 = 0.045;

/// The keyboard, row by row: each key's name as likh writes it, and its width
/// in units. Keys a shortcut can't use still get drawn, by any name.
const ROWS: [&[(&str, f32)]; 5] = [
    &[
        ("`", 1.0),
        ("1", 1.0),
        ("2", 1.0),
        ("3", 1.0),
        ("4", 1.0),
        ("5", 1.0),
        ("6", 1.0),
        ("7", 1.0),
        ("8", 1.0),
        ("9", 1.0),
        ("0", 1.0),
        ("-", 1.0),
        ("=", 1.0),
        ("Backspace", 2.0),
    ],
    &[
        ("Tab", 1.5),
        ("Q", 1.0),
        ("W", 1.0),
        ("E", 1.0),
        ("R", 1.0),
        ("T", 1.0),
        ("Y", 1.0),
        ("U", 1.0),
        ("I", 1.0),
        ("O", 1.0),
        ("P", 1.0),
        ("[", 1.0),
        ("]", 1.0),
        ("\\", 1.5),
    ],
    &[
        ("Caps", 1.75),
        ("A", 1.0),
        ("S", 1.0),
        ("D", 1.0),
        ("F", 1.0),
        ("G", 1.0),
        ("H", 1.0),
        ("J", 1.0),
        ("K", 1.0),
        ("L", 1.0),
        (";", 1.0),
        ("'", 1.0),
        ("Enter", 2.25),
    ],
    &[
        ("Shift", 2.25),
        ("Z", 1.0),
        ("X", 1.0),
        ("C", 1.0),
        ("V", 1.0),
        ("B", 1.0),
        ("N", 1.0),
        ("M", 1.0),
        (",", 1.0),
        (".", 1.0),
        ("/", 1.0),
        ("Right Shift", 2.75),
    ],
    &[
        ("Ctrl", 1.25),
        (crate::hotkey::WIN, 1.25),
        (crate::hotkey::ALT, 1.25),
        ("Space", 6.25),
        ("Right Alt", 1.25),
        ("Right Win", 1.25),
        ("Menu", 1.25),
        ("Right Ctrl", 1.25),
    ],
];

/// Where a key sits on the little keyboard, from its top left. None for keys
/// it doesn't have (F1 to F12, Home and the rest): those caps fade in where they land.
fn key(name: &str) -> Option<Rectangle> {
    ROWS.iter().enumerate().find_map(|(row, keys)| {
        let mut x = 0.0;
        keys.iter().find_map(|(label, width)| {
            let here = Rectangle::new(
                Point::new(x * UNIT, row as f32 * ROW_STEP),
                Size::new(width * UNIT - KEY_GAP, KEY_HEIGHT),
            );
            x += width;
            (*label == name).then_some(here)
        })
    })
}

#[derive(Debug, Clone)]
struct Cap {
    label: &'static str,
    /// Peach while picking a new shortcut and once it's saved.
    lit: bool,
    /// When it left the keyboard, on `clock`. None for a cap that was simply there.
    landed_at: Option<f32>,
}

/// The shortcut's caps and the keys lit under them, and where the motion is.
#[derive(Debug, Clone)]
pub struct Keys {
    caps: Vec<Cap>,
    /// A dashed slot after the caps: waiting for the next key.
    slot: bool,
    clock: f32,
    pulse_at: Option<f32>,
}

impl Keys {
    /// The shortcut as it is, at rest.
    pub fn at_rest(parts: &[&'static str]) -> Self {
        let caps = parts.iter().map(|label| Cap { label, lit: false, landed_at: None }).collect();
        Self { caps, slot: false, clock: 0.0, pulse_at: None }
    }

    /// Picking a new one: no caps yet, one empty slot.
    pub fn listen(&mut self) {
        self.caps.clear();
        self.slot = true;
        self.pulse_at = None;
    }

    /// The modifiers down right now. New ones fly in; let-go ones leave.
    pub fn hold(&mut self, held: &[&'static str]) {
        self.caps = self.arrive(held);
        self.slot = true;
    }

    /// The new shortcut: its last key flies in, then all of them pulse.
    pub fn chosen(&mut self, parts: &[&'static str]) {
        self.caps = self.arrive(parts);
        self.slot = false;
        self.pulse_at = Some(self.clock + LANDING * 0.6);
    }

    fn arrive(&self, labels: &[&'static str]) -> Vec<Cap> {
        labels
            .iter()
            .map(|label| {
                self.caps.iter().find(|cap| cap.label == *label).cloned().unwrap_or(Cap {
                    label,
                    lit: true,
                    landed_at: Some(self.clock),
                })
            })
            .collect()
    }

    pub fn step(&mut self, dt: f32) {
        self.clock += dt;
    }

    /// True while a key is in flight or the pulse is running.
    pub fn moving(&self) -> bool {
        let flying = self.caps.iter().any(|cap| cap.landed_at.is_some_and(|at| self.clock - at < LANDING));
        let pulsing = self.pulse_at.is_some_and(|at| self.clock - at < PULSE + PULSE_STAGGER * self.caps.len() as f32);
        flying || pulsing
    }

    pub fn art(&self, p: &Palette) -> Art {
        Art {
            keys: self.clone(),
            colors: Colors {
                backdrop: p.surface,
                key: p.elevated,
                lit: p.accent,
                on_lit: p.on_accent,
                cap: p.bg,
                edge: p.edge,
                ink: p.text,
                plus: p.text_subtle,
            },
        }
    }
}

pub struct Colors {
    backdrop: Color,
    key: Color,
    lit: Color,
    on_lit: Color,
    cap: Color,
    edge: Color,
    ink: Color,
    plus: Color,
}

/// The caps on the left, the keyboard on the right, in one canvas so a key can fly between them.
pub struct Art {
    keys: Keys,
    colors: Colors,
}

impl<M> canvas::Program<M> for Art {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let (keys, c) = (&self.keys, &self.colors);
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), c.backdrop);
        let board = Point::new(bounds.width - KEYBOARD.width, 0.0);
        let on_board = |r: Rectangle| Rectangle::new(Point::new(board.x + r.x, board.y + r.y), r.size());

        // The keyboard, with the shortcut's keys lit and a soft glow under them.
        let lit: Vec<&str> = keys.caps.iter().map(|cap| cap.label).collect();
        for (row, list) in ROWS.iter().enumerate() {
            let mut x = 0.0;
            for (label, width) in list.iter() {
                let mut here = on_board(Rectangle::new(
                    Point::new(x * UNIT, row as f32 * ROW_STEP),
                    Size::new(width * UNIT - KEY_GAP, KEY_HEIGHT),
                ));
                x += width;
                let on = lit.contains(label);
                // The key you just pressed dips for a moment as it lets go of its cap.
                let pressed = keys
                    .caps
                    .iter()
                    .any(|cap| cap.label == *label && cap.landed_at.is_some_and(|at| keys.clock - at < PRESSED));
                if pressed {
                    here = scaled(here, 0.82);
                }
                if on {
                    let glow = grown(here, 3.0);
                    frame.fill(&rounded(glow, 5.0), Color { a: 0.18, ..c.lit });
                }
                frame.fill(&rounded(here, 3.0), if on { c.lit } else { c.key });
            }
        }

        // The caps, left to right, with a + between them and the slot at the end.
        let advance = mono_advance(size::SMALL);
        let mut x = 0.0;
        for (i, cap) in keys.caps.iter().enumerate() {
            let width = 2.0 * space::S3 + advance * cap.label.chars().count() as f32;
            let home = Rectangle::new(Point::new(x, 0.0), Size::new(width, CAP_HEIGHT));
            let flight = cap.landed_at.map_or(1.0, |at| spring::PLAYFUL.progress(keys.clock - at));
            // It travels straight home; the spring's overshoot becomes a swell as it lands, not a
            // slide past home, which would carry the first cap off the canvas's left edge.
            let travel = flight.clamp(0.0, 1.0);
            let mut rect = match (cap.landed_at, key(cap.label)) {
                (Some(_), Some(from)) => lerp(on_board(from), home, travel),
                _ => home,
            };
            if flight > 1.0 {
                rect = scaled(rect, flight);
            }
            let corner = 3.0 + (radius::CONTROL - 1.0 - 3.0) * travel;
            if let Some(at) = keys.pulse_at {
                let t = (keys.clock - at - PULSE_STAGGER * i as f32) / PULSE;
                if (0.0..1.0).contains(&t) {
                    rect = scaled(rect, 1.0 + 0.08 * (t * std::f32::consts::PI).sin());
                }
            }
            // A swelling first cap stays inside the canvas.
            rect.x = rect.x.max(0.5);
            let shown = (0.4 + 0.6 * flight).clamp(0.0, 1.0);
            let (fill, ink) = if cap.lit { (c.lit, c.on_lit) } else { (c.cap, c.ink) };
            frame.fill(&rounded(rect, corner), Color { a: fill.a * shown, ..fill });
            if !cap.lit {
                frame.stroke(&rounded(rect, corner), Stroke::default().with_width(1.0).with_color(c.edge));
            }
            // The label appears once the cap is nearly home.
            let words = ((flight - 0.6) / 0.4).clamp(0.0, 1.0);
            frame.fill_text(Text {
                content: cap.label.to_string(),
                position: rect.center(),
                color: Color { a: ink.a * words, ..ink },
                size: Pixels(size::SMALL),
                font: theme::MONO,
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Center,
                ..Text::default()
            });
            x += width;
            if i + 1 < keys.caps.len() || keys.slot {
                frame.fill_text(Text {
                    content: "+".into(),
                    position: Point::new(x + space::S3, CAP_HEIGHT / 2.0),
                    color: c.plus,
                    size: Pixels(size::SMALL),
                    font: theme::UI,
                    align_x: alignment::Horizontal::Center.into(),
                    align_y: alignment::Vertical::Center,
                    ..Text::default()
                });
                x += 2.0 * space::S3;
            }
        }
        if keys.slot {
            let slot = Rectangle::new(Point::new(x, 0.0), Size::new(CAP_HEIGHT, CAP_HEIGHT));
            frame.stroke(
                &rounded(grown(slot, -0.75), radius::CONTROL - 1.0),
                Stroke {
                    line_dash: LineDash { segments: &[4.0, 3.0], offset: 0 },
                    ..Stroke::default().with_width(1.5).with_color(Color { a: 0.55, ..c.lit })
                },
            );
        }
        vec![frame.into_geometry()]
    }
}

fn rounded(r: Rectangle, corner: f32) -> Path {
    Path::rounded_rectangle(r.position(), r.size(), border::radius(corner))
}

fn grown(r: Rectangle, by: f32) -> Rectangle {
    Rectangle::new(Point::new(r.x - by, r.y - by), Size::new(r.width + 2.0 * by, r.height + 2.0 * by))
}

fn scaled(r: Rectangle, by: f32) -> Rectangle {
    let (w, h) = (r.width * by, r.height * by);
    Rectangle::new(Point::new(r.center_x() - w / 2.0, r.center_y() - h / 2.0), Size::new(w, h))
}

/// Part of the way from `a` to `b`; past 1 it overshoots, which is the bounce.
fn lerp(a: Rectangle, b: Rectangle, t: f32) -> Rectangle {
    let mix = |from: f32, to: f32| from + (to - from) * t;
    Rectangle::new(Point::new(mix(a.x, b.x), mix(a.y, b.y)), Size::new(mix(a.width, b.width), mix(a.height, b.height)))
}

/// How wide one character of Geist Mono is at `px`.
fn mono_advance(px: f32) -> f32 {
    let Ok(font) = FontRef::try_from_slice(theme::GEIST_MONO) else { return px * 0.6 };
    let units = font.units_per_em().unwrap_or(1000.0);
    font.h_advance_unscaled(font.glyph_id('0')) / units * px
}
