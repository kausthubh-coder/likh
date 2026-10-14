//! The pill: the small solid capsule at the bottom of the screen that shows
//! a dictation happening (docs/brand/BRAND.md, Pill states).
//!
//! `Look` is the whole animation as plain state stepped by time, and `draw`
//! paints it into a pixmap. Both are platform-free and tested here. The
//! window that shows the pixmap lives in `windows.rs`; it never takes focus,
//! lets clicks through, and draws nothing while the pill is hidden.
#![cfg_attr(
    not(target_os = "windows"),
    allow(dead_code, reason = "macOS (alpha) has no pill window yet; the drawing is tested on every platform")
)]

use crate::shared::spring::{self, Spring};
use crate::shared::theme::{self, palette};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use std::sync::mpsc::Sender;
use std::time::Duration;
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Pixmap, Point, SpreadMode, Stroke,
    Transform,
};

#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests;

/// The window the pill is drawn in, in logical pixels. It stays this size; the
/// pill changes shape inside it, so nothing resizes a window mid-animation.
pub const CANVAS: (f32, f32) = (400.0, 112.0);
/// The pill's bottom edge sits this far above the bottom of the work area (the taskbar).
const LIFT: f32 = 24.0;
const BARS: usize = 7;
/// Bars are 4 wide and 3 apart (46 in all), 15 from the pill's left edge.
const BAR_WIDTH: f32 = 4.0;
const BAR_GAP: f32 = 3.0;
const BAR_INSET: f32 = 15.0;
const SLOT: f32 = 24.0;
const SLOT_INSET: f32 = 6.0;
const ERROR_TEXT: f32 = 13.0;
const ERROR_LINE: f32 = 18.0;
const ERROR_MAX_WIDTH: f32 = 360.0;
/// How long an error stays before it fades, and how long done/cancelled linger as a sliver.
const ERROR_SECONDS: f32 = 6.0;
const SLIVER_SECONDS: f32 = 0.25;
const FADE_SECONDS: f32 = 0.15;
/// The quiet countdown shows its seconds instead of the stop square for this long at the end.
const QUIET_DIGITS_SECONDS: f32 = 3.0;

/// Windows' "Show animations" is off. The pill and onboarding then fade
/// instead of moving (BRAND.md, Reduced motion). It lives here because reading
/// it is a Win32 call, and `pill/windows.rs` is where those are allowed.
pub fn animations_off() -> bool {
    #[cfg(target_os = "windows")]
    {
        !windows::animations_on()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// What the pill shows. Hidden draws nothing at all.
#[derive(Debug, Clone, PartialEq)]
pub enum Show {
    Hidden,
    Listening,
    /// Recording with nothing held; a stop button on the right finishes it.
    HandsFree,
    /// Hands-free gone quiet: a ring around the stop button empties over `after`
    /// and it pastes when the ring runs out. `left` keeps counting down here
    /// between the dictation's ticks, so the ring moves smoothly.
    QuietStop {
        left: Duration,
        after: Duration,
    },
    Thinking,
    Warning {
        seconds_left: u64,
    },
    Error(String),
    Done,
    Cancelled,
}

impl Show {
    fn voiced(&self) -> bool {
        matches!(self, Show::Listening | Show::HandsFree | Show::QuietStop { .. } | Show::Warning { .. })
    }

    /// Both hands-free states carry the stop button.
    fn hands_free(&self) -> bool {
        matches!(self, Show::HandsFree | Show::QuietStop { .. })
    }
}

enum Command {
    Show(Show),
    Level(f32),
}

/// A handle to the pill. Cheap to clone; every clone talks to the same window.
#[derive(Clone)]
pub struct Pill {
    commands: Option<Sender<Command>>,
    #[cfg(target_os = "windows")]
    waker: Option<windows::Waker>,
}

impl Pill {
    /// Opens the pill's window on its own thread. Clicks on the hands-free stop
    /// button go to `dictation`. If the window fails, dictation still works; it
    /// just has nothing on screen.
    pub fn spawn(dictation: Sender<crate::dictation::Input>) -> Self {
        #[cfg(target_os = "windows")]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            match windows::spawn(rx, dictation) {
                Ok(waker) => Self { commands: Some(tx), waker: Some(waker) },
                Err(reason) => {
                    eprintln!("likh: no pill on screen: {reason}");
                    Self::none()
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            // macOS alpha: dictation works, the overlay comes later (docs/plan.md).
            let _ = dictation;
            Self::none()
        }
    }

    pub fn none() -> Self {
        Self {
            commands: None,
            #[cfg(target_os = "windows")]
            waker: None,
        }
    }

    pub fn show(&self, show: Show) {
        self.send(Command::Show(show));
    }

    /// A 0–1 mic level, about every 30ms while listening.
    pub fn level(&self, level: f32) {
        self.send(Command::Level(level));
    }

    fn send(&self, command: Command) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(command);
            #[cfg(target_os = "windows")]
            if let Some(waker) = &self.waker {
                waker.wake();
            }
        }
    }
}

/// Everything about how the pill looks right now.
pub struct Look {
    show: Show,
    width: Spring,
    height: Spring,
    alpha: f32,
    /// Mic levels, newest first. The middle bar shows the newest, outer bars older ones.
    history: [f32; 4],
    bars: [f32; BARS],
    clock: f32,
    /// Seconds until a finished state starts fading out.
    linger: Option<f32>,
    /// Error text broken into the lines it will draw as.
    lines: Vec<String>,
    /// Animations are off: the pill takes each shape at once (its fade stays),
    /// and thinking pulses in place instead of travelling.
    still: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            show: Show::Hidden,
            width: Spring::new(spring::FAST, 40.0),
            height: Spring::new(spring::FAST, 8.0),
            alpha: 0.0,
            history: [0.0; 4],
            bars: [0.0; BARS],
            clock: 0.0,
            linger: None,
            lines: Vec::new(),
            still: false,
        }
    }
}

impl Look {
    pub fn is_hidden(&self) -> bool {
        self.show == Show::Hidden && self.alpha <= 0.0
    }

    pub fn hold_still(&mut self, still: bool) {
        self.still = still;
    }

    pub fn set(&mut self, show: Show) {
        if self.is_hidden() && show != Show::Hidden {
            // Every appearance grows out of the resting sliver.
            self.width = Spring::new(spring::FAST, 40.0);
            self.height = Spring::new(spring::FAST, 8.0);
            self.history = [0.0; 4];
        }
        let (width, height) = match &show {
            Show::Listening | Show::Thinking => (76.0, 36.0),
            Show::Warning { .. } | Show::HandsFree | Show::QuietStop { .. } => (100.0, 36.0),
            Show::Error(message) => {
                self.lines = wrap(message, ERROR_MAX_WIDTH - 60.0);
                let widest = self.lines.iter().map(|line| text_width(line, ERROR_TEXT)).fold(0.0, f32::max);
                ((widest + 60.0).clamp(200.0, ERROR_MAX_WIDTH), 52.0)
            }
            Show::Done | Show::Cancelled | Show::Hidden => (40.0, 8.0),
        };
        self.width.target = width;
        self.height.target = height;
        if self.still {
            for spring in [&mut self.width, &mut self.height] {
                spring.value = spring.target;
                spring.velocity = 0.0;
            }
        }
        self.linger = match show {
            Show::Done | Show::Cancelled => Some(SLIVER_SECONDS),
            Show::Error(_) => Some(ERROR_SECONDS),
            _ => None,
        };
        self.show = show;
    }

    /// Where the hands-free stop button is (centre x, centre y, radius, in
    /// canvas pixels), so the window can take clicks there and nowhere else.
    pub fn stop_button(&self) -> Option<(f32, f32, f32)> {
        if !self.show.hands_free() {
            return None;
        }
        let (w, h) = (self.width.value.max(8.0), self.height.value.max(4.0));
        let right = (CANVAS.0 + w) / 2.0;
        let cy = CANVAS.1 - LIFT - h / 2.0;
        // Only once the pill is big enough to hold it.
        (w >= 90.0 && h >= 30.0).then_some((right - SLOT_INSET - SLOT / 2.0, cy, SLOT / 2.0))
    }

    pub fn level(&mut self, level: f32) {
        self.history.rotate_right(1);
        self.history[0] = level.clamp(0.0, 1.0);
    }

    /// Advances by `dt` seconds. Returns false once nothing moves, so the window can stop drawing.
    pub fn step(&mut self, dt: f32) -> bool {
        self.clock += dt;
        if let Show::QuietStop { left, .. } = &mut self.show {
            *left = left.saturating_sub(Duration::from_secs_f32(dt));
        }
        let sized = self.width.step(dt) & self.height.step(dt);

        if let Some(left) = self.linger.as_mut() {
            *left -= dt;
            if *left <= 0.0 {
                self.linger = None;
                self.show = Show::Hidden;
            }
        }
        let target_alpha = match self.show {
            Show::Hidden => 0.0,
            Show::Done | Show::Cancelled => 0.7,
            _ => 1.0,
        };
        let fade = dt / FADE_SECONDS;
        self.alpha = if self.alpha < target_alpha {
            (self.alpha + fade).min(target_alpha)
        } else {
            (self.alpha - fade).max(target_alpha)
        };

        for (i, bar) in self.bars.iter_mut().enumerate() {
            let age = (i as i32 - (BARS as i32 - 1) / 2).unsigned_abs() as usize;
            let target = match self.show {
                // Outer bars carry older levels and stand a little shorter, so a loud
                // voice still reads as a shape, not a block.
                Show::Listening | Show::HandsFree | Show::QuietStop { .. } | Show::Warning { .. } => {
                    self.history[age] * (1.0 - 0.12 * age as f32)
                }
                // Still: the bars stand level and `draw` pulses them instead.
                Show::Thinking if self.still => 0.3,
                // A slow travelling wave, left to right (1.2s loop).
                Show::Thinking => 0.15 + 0.35 * (1.0 + (self.clock * 5.2 - i as f32 * 0.7).sin()) / 2.0,
                _ => 0.0,
            };
            // Attack 40ms, release 140ms (BRAND.md, Level bars).
            let rate = if target > *bar { 0.04 } else { 0.14 };
            *bar += (target - *bar) * (dt / rate).min(1.0);
            if (target - *bar).abs() < 0.001 {
                *bar = target;
            }
        }

        let animated = self.show.voiced() || self.show == Show::Thinking;
        !sized || self.alpha != target_alpha || animated || self.linger.is_some() || self.bars.iter().any(|b| *b > 0.0)
    }

    /// Paints the pill at `scale` (the monitor's DPI over 96) into a canvas-sized pixmap.
    pub fn draw(&self, scale: f32) -> Option<Pixmap> {
        let mut pixmap = Pixmap::new((CANVAS.0 * scale).ceil() as u32, (CANVAS.1 * scale).ceil() as u32)?;
        if self.alpha <= 0.0 {
            return Some(pixmap);
        }
        let p = palette();
        let (w, h) = (self.width.value.max(8.0), self.height.value.max(4.0));
        let (x, y) = ((CANVAS.0 - w) / 2.0, CANVAS.1 - LIFT - h);
        let at = Transform::from_scale(scale, scale);

        // Two shadows: a tight one and a soft one (BRAND.md, Surface), the soft one as stacked rings.
        for ring in 1..=6 {
            let grow = ring as f32 * 1.6;
            let shadow = capsule(x - grow, y - grow + 6.0, w + 2.0 * grow, h + 2.0 * grow);
            fill(&mut pixmap, shadow, rgba(0.0, 0.0, 0.0, 0.035), at, None);
        }
        fill(&mut pixmap, capsule(x, y + 1.0, w, h), rgba(0.0, 0.0, 0.0, 0.2), at, None);

        let body = capsule(x, y, w, h)?;
        fill(&mut pixmap, Some(body.clone()), to_skia(p.surface, 1.0), at, None);
        // Hairline outside, highlight inside along the top.
        stroke(&mut pixmap, capsule(x - 0.25, y - 0.25, w + 0.5, h + 0.5), rgba(0.0, 0.0, 0.0, 0.5), 0.5, at);
        if let Some(shader) = LinearGradient::new(
            Point::from_xy(0.0, y),
            Point::from_xy(0.0, y + h / 2.0),
            vec![GradientStop::new(0.0, rgba(1.0, 1.0, 1.0, 0.10)), GradientStop::new(1.0, rgba(1.0, 1.0, 1.0, 0.0))],
            SpreadMode::Pad,
            Transform::identity(),
        ) {
            let paint = Paint { shader, anti_alias: true, ..Paint::default() };
            if let Some(outline) = capsule(x + 0.5, y + 0.5, w - 1.0, h - 1.0) {
                pixmap.stroke_path(&outline, &paint, &Stroke { width: 1.0, ..Stroke::default() }, at, None);
            }
        }

        // Everything inside is clipped to the capsule, so it can't spill out while the pill grows.
        let mut clip = Mask::new(pixmap.width(), pixmap.height())?;
        clip.fill_path(&body, FillRule::Winding, true, at);
        let cy = y + h / 2.0;
        match &self.show {
            Show::Error(_) => self.draw_error(&mut pixmap, &clip, x, cy, scale),
            _ => {
                let color = if self.show == Show::Thinking { p.text_muted } else { p.accent };
                // Still and thinking: a gentle opacity pulse on the same 1.2s loop, nothing travels.
                let pulse = if self.still && self.show == Show::Thinking {
                    0.7 + 0.3 * (self.clock * std::f32::consts::TAU / 1.2).cos()
                } else {
                    1.0
                };
                for (i, level) in self.bars.iter().enumerate() {
                    let bar_h = (4.0 + level * 16.0).min(h - 8.0).max(0.0);
                    let bar_x = x + BAR_INSET + i as f32 * (BAR_WIDTH + BAR_GAP);
                    let bar = capsule(bar_x, cy - bar_h / 2.0, BAR_WIDTH, bar_h.max(BAR_WIDTH));
                    if bar_h > 0.0 && h > 16.0 {
                        fill(&mut pixmap, bar, to_skia(color, pulse), at, Some(&clip));
                    }
                }
            }
        }
        if let Some((cx, cy, r)) = self.stop_button() {
            fill(&mut pixmap, PathBuilder::from_circle(cx, cy, r), to_skia(p.elevated, 1.0), at, Some(&clip));
            let left = match self.show {
                Show::QuietStop { left, after } => {
                    // Peach, not the warning yellow: stopping after quiet is the normal end, not a problem.
                    let share = (left.as_secs_f32() / after.as_secs_f32().max(0.001)).clamp(0.0, 1.0);
                    stroke(&mut pixmap, arc(cx, cy, r - 1.5, share), to_skia(p.accent, 1.0), 2.0, at);
                    Some(left.as_secs_f32())
                }
                _ => None,
            };
            match left {
                Some(left) if left > 0.0 && left <= QUIET_DIGITS_SECONDS => {
                    let label = (left.ceil() as u64).to_string();
                    let tw = text_width(&label, 12.0);
                    draw_text(&mut pixmap, &label, 12.0, cx - tw / 2.0, cy + 4.0, to_skia(p.text, 1.0), scale);
                }
                _ => {
                    let square = rounded_rect(cx - 4.0, cy - 4.0, 8.0, 8.0, 2.0);
                    fill(&mut pixmap, square, to_skia(p.text, 1.0), at, Some(&clip));
                }
            }
        }
        if let Show::Warning { seconds_left } = self.show {
            let (cx, r) = (x + w - SLOT_INSET - SLOT / 2.0, SLOT / 2.0);
            fill(&mut pixmap, PathBuilder::from_circle(cx, cy, r), to_skia(p.elevated, 1.0), at, Some(&clip));
            // The ring empties over the last minute, clockwise from the top.
            let share = (seconds_left as f32 / 60.0).clamp(0.0, 1.0);
            stroke(&mut pixmap, arc(cx, cy, r - 1.5, share), to_skia(p.warning, 1.0), 2.0, at);
            let label = seconds_left.to_string();
            let tw = text_width(&label, 12.0);
            draw_text(&mut pixmap, &label, 12.0, cx - tw / 2.0, cy + 4.0, to_skia(p.text, 1.0), scale);
        }

        if self.alpha < 1.0 {
            for pixel in pixmap.pixels_mut() {
                let c = pixel.demultiply();
                let faded = tiny_skia::ColorU8::from_rgba(
                    c.red(),
                    c.green(),
                    c.blue(),
                    (f32::from(c.alpha()) * self.alpha) as u8,
                );
                *pixel = faded.premultiply();
            }
        }
        Some(pixmap)
    }

    fn draw_error(&self, pixmap: &mut Pixmap, clip: &Mask, x: f32, cy: f32, scale: f32) {
        let p = palette();
        let at = Transform::from_scale(scale, scale);
        let icon_x = x + 16.0 + 8.0;
        fill(pixmap, PathBuilder::from_circle(icon_x, cy, 8.0), to_skia(p.danger, 1.0), at, Some(clip));
        fill(pixmap, rounded_rect(icon_x - 1.0, cy - 4.5, 2.0, 5.5, 1.0), to_skia(p.surface, 1.0), at, Some(clip));
        fill(pixmap, PathBuilder::from_circle(icon_x, cy + 3.5, 1.1), to_skia(p.surface, 1.0), at, Some(clip));
        let top = cy - self.lines.len() as f32 * ERROR_LINE / 2.0;
        for (i, line) in self.lines.iter().enumerate() {
            let baseline = top + i as f32 * ERROR_LINE + 13.5;
            draw_text(pixmap, line, ERROR_TEXT, x + 44.0, baseline, to_skia(p.text, 1.0), scale);
        }
    }
}

fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::from_rgba(r, g, b, a).unwrap_or(Color::TRANSPARENT)
}

fn to_skia(color: iced::Color, alpha: f32) -> Color {
    rgba(color.r, color.g, color.b, color.a * alpha)
}

fn fill(pixmap: &mut Pixmap, path: Option<Path>, color: Color, at: Transform, clip: Option<&Mask>) {
    let Some(path) = path else { return };
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    pixmap.fill_path(&path, &paint, FillRule::Winding, at, clip);
}

fn stroke(pixmap: &mut Pixmap, path: Option<Path>, color: Color, width: f32, at: Transform) {
    let Some(path) = path else { return };
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    let stroke = Stroke { width, line_cap: tiny_skia::LineCap::Round, ..Stroke::default() };
    pixmap.stroke_path(&path, &paint, &stroke, at, None);
}

/// A rectangle whose ends are half circles. None only for shapes too small to draw.
fn capsule(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    rounded_rect(x, y, w, h, (w.min(h) / 2.0).max(0.0))
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    // Corners as cubic quarter circles (k = 0.5523).
    let k = 0.552_284_8 * r;
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}

/// The part of a circle from twelve o'clock clockwise, `share` of the way round.
fn arc(cx: f32, cy: f32, r: f32, share: f32) -> Option<Path> {
    if share <= 0.0 {
        return None;
    }
    let steps = (64.0 * share).ceil().max(2.0) as usize;
    let mut b = PathBuilder::new();
    for i in 0..=steps {
        let angle = std::f32::consts::TAU * share * i as f32 / steps as f32 - std::f32::consts::FRAC_PI_2;
        let (px, py) = (cx + r * angle.cos(), cy + r * angle.sin());
        if i == 0 { b.move_to(px, py) } else { b.line_to(px, py) }
    }
    b.finish()
}

fn font() -> Option<FontRef<'static>> {
    FontRef::try_from_slice(theme::GEIST_MEDIUM).ok()
}

pub fn text_width(text: &str, size: f32) -> f32 {
    width_at(text, PxScale::from(size))
}

/// The width of `text` at an iced text size. iced sizes text by the em;
/// ab_glyph's `PxScale` (what the pill draws with) is the taller ascent-to-descent height.
pub fn ui_text_width(text: &str, size: f32) -> f32 {
    let Some(font) = font() else { return 0.0 };
    let per_em = font.units_per_em().map_or(1.0, |units| font.height_unscaled() / units);
    width_at(text, PxScale::from(size * per_em))
}

fn width_at(text: &str, px: PxScale) -> f32 {
    let Some(font) = font() else { return 0.0 };
    let scaled = font.as_scaled(px);
    let mut width = 0.0;
    let mut previous = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = previous {
            width += scaled.kern(prev, id);
        }
        width += scaled.h_advance(id);
        previous = Some(id);
    }
    width
}

/// Greedy word wrap into at most two lines; a third would make the pill a toast.
pub fn wrap(text: &str, max_width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if text_width(&candidate, ERROR_TEXT) <= max_width || current.is_empty() {
            current = candidate;
        } else {
            lines.push(std::mem::replace(&mut current, word.to_string()));
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.len() > 2 {
        let mut second = lines[1..].join(" ");
        while text_width(&format!("{second}…"), ERROR_TEXT) > max_width && second.pop().is_some() {}
        lines.truncate(1);
        lines.push(format!("{}…", second.trim_end()));
    }
    lines
}

/// Draws one line of Geist Medium with its baseline at `baseline` (logical pixels).
fn draw_text(pixmap: &mut Pixmap, text: &str, size: f32, x: f32, baseline: f32, color: Color, scale: f32) {
    let Some(font) = font() else { return };
    let px = PxScale::from(size * scale);
    let scaled = font.as_scaled(px);
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let ink = color.premultiply().to_color_u8();
    let mut caret = x * scale;
    let mut previous = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = previous {
            caret += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(px, point(caret, baseline * scale));
        caret += scaled.h_advance(id);
        previous = Some(id);
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let bounds = outline.px_bounds();
        let pixels = pixmap.pixels_mut();
        outline.draw(|gx, gy, coverage| {
            let (px, py) = (bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32);
            if px < 0 || py < 0 || px >= width || py >= height {
                return;
            }
            let dst = &mut pixels[(py * width + px) as usize];
            let a = coverage.clamp(0.0, 1.0);
            let blend = |s: u8, d: u8| {
                (f32::from(s) * a + f32::from(d) * (1.0 - f32::from(ink.alpha()) / 255.0 * a)).round() as u8
            };
            let out = tiny_skia::PremultipliedColorU8::from_rgba(
                blend(ink.red(), dst.red()),
                blend(ink.green(), dst.green()),
                blend(ink.blue(), dst.blue()),
                blend(ink.alpha(), dst.alpha()),
            );
            if let Some(out) = out {
                *dst = out;
            }
        });
    }
}
