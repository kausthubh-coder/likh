//! The pill: the small solid capsule at the bottom of the screen that shows
//! a dictation happening (docs/brand/BRAND.md, Pill states).
//!
//! `Look` is the whole animation as plain state stepped by time, and `draw`
//! paints it into a pixmap. Both are platform-free and tested here. The
//! window that shows the pixmap lives in `windows.rs` and `macos.rs`; it never
//! takes focus, lets clicks through, and draws nothing while the pill is hidden.
#![cfg_attr(
    not(target_os = "windows"),
    allow(
        dead_code,
        reason = "only the Windows pill takes clicks; Linux has no pill; the drawing is tested everywhere"
    )
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

#[cfg(target_os = "macos")]
mod macos;
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
/// An action's recording: the spark's slot on the left pushes the bars right by this much.
const SPARK_SHIFT: f32 = SLOT;
/// Text after the spark's slot starts here, from the pill's left edge.
const AFTER_SLOT: f32 = SLOT_INSET + SLOT + 8.0;
/// An action's result stays this long.
const FINISHED_SECONDS: f32 = 2.0;
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
    #[cfg(target_os = "macos")]
    {
        !macos::animations_on()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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
    /// A failure with one fix: the error look plus a button that opens it.
    Problem {
        text: String,
        fix: Fix,
    },
    /// Nothing broke, nothing happened ("No voice heard"): grey flat bars, a short stay, no sound.
    Note(String),
    /// Still recording, but the mic has sent only digital silence. It stays
    /// until the dictation ends or sound arrives.
    MicSilent {
        device: String,
    },
    Done,
    Cancelled,
    /// This state, for the action key: the spark sits on the left (`actions/`).
    Spark(Box<Show>),
    /// An action is running: the spark, its name, and a small travelling wave.
    Running(String),
    /// An action finished and says so (its `notify` output): a check, and its words for a moment.
    Finished(String),
    /// An action waits to be confirmed: the spark with a ring that empties over
    /// `after`, the question, and the keys that answer it.
    Confirm {
        question: String,
        /// The action key, as `Ctrl+Shift+Space`.
        keys: String,
        left: Duration,
        after: Duration,
    },
}

/// What a problem's button opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    /// likh's own Microphone row, with its list and Test meter.
    PickMic,
    /// Windows' microphone privacy page.
    MicAccess,
    /// likh's Actions tab, to approve an action's scripts.
    Review,
    /// likh's Actions tab, where the failed action shows what went wrong.
    Details,
    /// Send the failed recording again (its audio is kept).
    Retry,
    /// Another app has the shortcut: likh's Dictation tab, already listening for new keys.
    Shortcut,
}

impl Fix {
    pub fn label(self) -> &'static str {
        match self {
            Fix::PickMic => "Pick mic",
            Fix::MicAccess => "Open settings",
            Fix::Review => "Review",
            Fix::Details => "Details",
            Fix::Retry => "Retry",
            Fix::Shortcut => "Change keys",
        }
    }

    /// A Windows settings page, or None when the fix is in likh's own window.
    pub fn url(self) -> Option<&'static str> {
        match self {
            Fix::PickMic | Fix::Review | Fix::Details | Fix::Retry | Fix::Shortcut => None,
            Fix::MicAccess => Some(crate::shared::MIC_SETTINGS),
        }
    }
}

/// What a click on the pill does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Stop,
    Fix(Fix),
}

impl Show {
    fn voiced(&self) -> bool {
        matches!(self, Show::Listening | Show::HandsFree | Show::QuietStop { .. } | Show::Warning { .. })
    }

    /// Both hands-free states carry the stop button.
    fn hands_free(&self) -> bool {
        matches!(self, Show::HandsFree | Show::QuietStop { .. })
    }

    /// The words of a message state, whether it is a failure (red icon) or
    /// just news (grey flat bars), and its button if it has one.
    fn message(&self) -> Option<(String, bool, Option<Fix>)> {
        match self {
            Show::Error(text) => Some((text.clone(), true, None)),
            Show::Problem { text, fix } => Some((text.clone(), true, Some(*fix))),
            Show::Note(text) => Some((text.clone(), false, None)),
            Show::MicSilent { device } => Some((format!("{device} is silent. Is it muted?"), false, None)),
            _ => None,
        }
    }
}

/// A message's text starts here, after its icon.
const MESSAGE_TEXT_X: f32 = 44.0;
/// A message's button: as tall as the pill less its 8px inset, its label padded 12 each side.
const FIX_HEIGHT: f32 = 36.0;
const FIX_INSET: f32 = 8.0;
const FIX_GAP: f32 = 12.0;
/// How long news stays; errors and problems stay `ERROR_SECONDS`.
const NOTE_SECONDS: f32 = 3.0;

fn fix_width(fix: Fix) -> f32 {
    text_width(fix.label(), ERROR_TEXT) + 24.0
}

/// The space right of a message's text: the button and its gaps, or plain padding.
fn message_right(fix: Option<Fix>) -> f32 {
    fix.map_or(16.0, |fix| FIX_GAP + fix_width(fix) + FIX_INSET)
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
    pub fn spawn(
        dictation: Sender<crate::dictation::Input>,
        ui: iced::futures::channel::mpsc::UnboundedSender<crate::tray::Pick>,
    ) -> Self {
        #[cfg(target_os = "windows")]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            match windows::spawn(rx, dictation, ui) {
                Ok(waker) => Self { commands: Some(tx), waker: Some(waker) },
                Err(reason) => {
                    crate::diagnostics::error("pill", format!("no pill on screen: {reason}"));
                    Self::none()
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            // The macOS pill shows every state but takes no clicks (pill/macos.rs).
            let _ = (dictation, ui);
            let (tx, rx) = std::sync::mpsc::channel();
            match macos::spawn(rx) {
                Ok(()) => Self { commands: Some(tx) },
                Err(reason) => {
                    crate::diagnostics::error("pill", format!("no pill on screen: {reason}"));
                    Self::none()
                }
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            // Linux is for agents and tests: no pill window.
            let _ = (dictation, ui);
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
    /// The pointer is over the pill: a message waits while it's being read.
    hovered: bool,
    /// The action key's spark is on the left.
    spark: bool,
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
            hovered: false,
            spark: false,
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
        let (spark, show) = match show {
            Show::Spark(inner) => (true, *inner),
            show => (matches!(show, Show::Running(_) | Show::Finished(_) | Show::Confirm { .. }), show),
        };
        self.spark = spark;
        if self.is_hidden() && show != Show::Hidden {
            // Every appearance grows out of the resting sliver.
            self.width = Spring::new(spring::FAST, 40.0);
            self.height = Spring::new(spring::FAST, 8.0);
            self.history = [0.0; 4];
        }
        let (width, height) = match (&show, show.message()) {
            (_, Some((text, alarm, fix))) => {
                let around = MESSAGE_TEXT_X + message_right(fix);
                self.lines = wrap(&text, ERROR_MAX_WIDTH - around);
                let widest = self.lines.iter().map(|line| text_width(line, ERROR_TEXT)).fold(0.0, f32::max);
                // Failures keep the error pill's size; news hugs its words.
                if alarm {
                    ((widest + around).clamp(200.0, ERROR_MAX_WIDTH), 52.0)
                } else {
                    ((widest + around).min(ERROR_MAX_WIDTH), if self.lines.len() > 1 { 52.0 } else { 36.0 })
                }
            }
            (Show::Listening | Show::Thinking, _) => (76.0 + self.shift(), 36.0),
            (Show::Warning { .. } | Show::HandsFree | Show::QuietStop { .. }, _) => (100.0 + self.shift(), 36.0),
            (Show::Running(name), _) => (AFTER_SLOT + text_width(name, ERROR_TEXT) + 8.0 + RUNNING_BARS + 16.0, 36.0),
            (Show::Finished(words), _) => {
                self.lines = vec![one_line(words, ERROR_MAX_WIDTH - AFTER_SLOT - 16.0)];
                (AFTER_SLOT + text_width(&self.lines[0], ERROR_TEXT) + 16.0, 36.0)
            }
            (Show::Confirm { question, keys, .. }, _) => (confirm_layout(question, keys).1, 36.0),
            _ => (40.0, 8.0),
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
            Show::Error(_) | Show::Problem { .. } => Some(ERROR_SECONDS),
            Show::Note(_) => Some(NOTE_SECONDS),
            Show::Finished(_) => Some(FINISHED_SECONDS),
            _ => None,
        };
        self.show = show;
    }

    /// How far the spark pushes the bars right.
    fn shift(&self) -> f32 {
        if self.spark { SPARK_SHIFT } else { 0.0 }
    }

    /// The pointer moved onto the pill (or off it). A message stays while it's hovered.
    pub fn hover(&mut self, on: bool) {
        self.hovered = on;
    }

    /// The pill's body covers this canvas point.
    pub fn covers(&self, x: f32, y: f32) -> bool {
        let (w, h) = (self.width.value, self.height.value);
        let (left, top) = ((CANVAS.0 - w) / 2.0, CANVAS.1 - LIFT - h);
        self.alpha > 0.0 && x >= left && x <= left + w && y >= top && y <= top + h
    }

    /// Where a problem's button is (x, y, width, height, in canvas pixels), once the pill has grown to hold it.
    pub fn fix_button(&self) -> Option<(f32, f32, f32, f32, Fix)> {
        let Show::Problem { fix, .. } = &self.show else { return None };
        let (w, h) = (self.width.value, self.height.value);
        let bw = fix_width(*fix);
        let right = (CANVAS.0 + w) / 2.0 - FIX_INSET;
        let cy = CANVAS.1 - LIFT - h / 2.0;
        (h >= FIX_HEIGHT + 2.0 * FIX_INSET - 1.0 && w >= self.width.target - 1.0).then_some((
            right - bw,
            cy - FIX_HEIGHT / 2.0,
            bw,
            FIX_HEIGHT,
            *fix,
        ))
    }

    /// The one place on the pill that takes a click, as a box in canvas pixels.
    pub fn target(&self) -> Option<((f32, f32, f32, f32), Click)> {
        if let Some((cx, cy, r)) = self.stop_button() {
            return Some(((cx - r, cy - r, 2.0 * r, 2.0 * r), Click::Stop));
        }
        self.fix_button().map(|(x, y, w, h, fix)| ((x, y, w, h), Click::Fix(fix)))
    }

    /// A message is up and waiting to be read; the window watches the pointer only then.
    pub fn reading(&self) -> bool {
        self.show.message().is_some() && self.linger.is_some()
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
        if let Show::QuietStop { left, .. } | Show::Confirm { left, .. } = &mut self.show {
            *left = left.saturating_sub(Duration::from_secs_f32(dt));
        }
        let sized = self.width.step(dt) & self.height.step(dt);

        if !self.hovered
            && let Some(left) = self.linger.as_mut()
        {
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
                // Three small bars after the action's name, on the thinking wave's rhythm.
                Show::Running(_) if i < 3 && self.still => 0.3,
                Show::Running(_) if i < 3 => 0.15 + 0.35 * (1.0 + (self.clock * 5.2 - i as f32 * 0.7).sin()) / 2.0,
                _ => 0.0,
            };
            // Attack 40ms, release 140ms (BRAND.md, Level bars).
            let rate = if target > *bar { 0.04 } else { 0.14 };
            *bar += (target - *bar) * (dt / rate).min(1.0);
            if (target - *bar).abs() < 0.001 {
                *bar = target;
            }
        }

        let animated =
            self.show.voiced() || matches!(self.show, Show::Thinking | Show::Running(_) | Show::Confirm { .. });
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
        match (&self.show, self.show.message()) {
            (_, Some((_, alarm, _))) => self.draw_message(&mut pixmap, &clip, x, cy, alarm, scale),
            (Show::Running(_) | Show::Finished(_) | Show::Confirm { .. }, None) => {
                self.draw_action(&mut pixmap, &clip, x, cy, h, scale);
            }
            (_, None) => {
                if self.spark && h > 16.0 {
                    let sx = x + SLOT_INSET + SLOT / 2.0;
                    fill(
                        &mut pixmap,
                        PathBuilder::from_circle(sx, cy, SLOT / 2.0),
                        to_skia(p.elevated, 1.0),
                        at,
                        Some(&clip),
                    );
                    draw_spark(&mut pixmap, &clip, sx, cy, at);
                }
                let color = if self.show == Show::Thinking { p.text_muted } else { p.accent };
                // Still and thinking: a gentle opacity pulse on the same 1.2s loop, nothing travels.
                let pulse = if self.still && self.show == Show::Thinking {
                    0.7 + 0.3 * (self.clock * std::f32::consts::TAU / 1.2).cos()
                } else {
                    1.0
                };
                for (i, level) in self.bars.iter().enumerate() {
                    let bar_h = (4.0 + level * 16.0).min(h - 8.0).max(0.0);
                    let bar_x = x + self.shift() + BAR_INSET + i as f32 * (BAR_WIDTH + BAR_GAP);
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

    /// An action's own states: the spark (or a check once it finished) in its
    /// slot on the left, then its words. A running action adds three small
    /// waving bars, a waiting one its ring and the keys that answer it.
    fn draw_action(&self, pixmap: &mut Pixmap, clip: &Mask, x: f32, cy: f32, h: f32, scale: f32) {
        let p = palette();
        let at = Transform::from_scale(scale, scale);
        if h < 30.0 {
            return;
        }
        let (sx, r) = (x + SLOT_INSET + SLOT / 2.0, SLOT / 2.0);
        fill(pixmap, PathBuilder::from_circle(sx, cy, r), to_skia(p.elevated, 1.0), at, Some(clip));
        let baseline = cy + 4.5;
        let text_x = x + AFTER_SLOT;
        match &self.show {
            Show::Running(name) => {
                draw_spark(pixmap, clip, sx, cy, at);
                draw_text(pixmap, name, ERROR_TEXT, text_x, baseline, to_skia(p.text, 1.0), scale);
                let bars_x = text_x + text_width(name, ERROR_TEXT) + 8.0;
                for (i, level) in self.bars.iter().take(3).enumerate() {
                    let bar_h = 4.0 + level * 8.0;
                    let bar = capsule(bars_x + i as f32 * (BAR_WIDTH + 3.0), cy - bar_h / 2.0, BAR_WIDTH, bar_h);
                    fill(pixmap, bar, to_skia(p.text_muted, 1.0), at, Some(clip));
                }
            }
            Show::Finished(_) => {
                let mut check = PathBuilder::new();
                check.move_to(sx - 3.4, cy + 0.2);
                check.line_to(sx - 1.0, cy + 2.6);
                check.line_to(sx + 3.6, cy - 2.6);
                stroke(pixmap, check.finish(), to_skia(p.success, 1.0), 1.8, at);
                if let Some(line) = self.lines.first() {
                    draw_text(pixmap, line, ERROR_TEXT, text_x, baseline, to_skia(p.text, 1.0), scale);
                }
            }
            Show::Confirm { question, keys, left, after } => {
                draw_spark(pixmap, clip, sx, cy, at);
                // Peach, like stopping after quiet: running it is the normal answer.
                let share = (left.as_secs_f32() / after.as_secs_f32().max(0.001)).clamp(0.0, 1.0);
                stroke(pixmap, arc(sx, cy, r - 1.5, share), to_skia(p.accent, 1.0), 2.0, at);
                for part in confirm_layout(question, keys).0 {
                    let px = x + part.x;
                    match part.kind {
                        Part::Words | Part::Quiet => {
                            let ink = if part.kind == Part::Words { p.text } else { p.text_muted };
                            draw_text(pixmap, &part.text, ERROR_TEXT, px, baseline, to_skia(ink, 1.0), scale);
                        }
                        Part::Cap => {
                            fill(
                                pixmap,
                                rounded_rect(px, cy - 10.0, part.width, 20.0, 5.0),
                                to_skia(p.elevated, 1.0),
                                at,
                                Some(clip),
                            );
                            draw_text(
                                pixmap,
                                &part.text,
                                CAP_TEXT,
                                px + CAP_PAD,
                                cy + 4.0,
                                to_skia(p.text, 1.0),
                                scale,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// A failure gets the red alert icon; news gets three grey bars lying flat,
    /// the pill's own bars with nothing to show.
    fn draw_message(&self, pixmap: &mut Pixmap, clip: &Mask, x: f32, cy: f32, alarm: bool, scale: f32) {
        let p = palette();
        let at = Transform::from_scale(scale, scale);
        let icon_x = x + 16.0 + 8.0;
        if alarm {
            fill(pixmap, PathBuilder::from_circle(icon_x, cy, 8.0), to_skia(p.danger, 1.0), at, Some(clip));
            fill(pixmap, rounded_rect(icon_x - 1.0, cy - 4.5, 2.0, 5.5, 1.0), to_skia(p.surface, 1.0), at, Some(clip));
            fill(pixmap, PathBuilder::from_circle(icon_x, cy + 3.5, 1.1), to_skia(p.surface, 1.0), at, Some(clip));
        } else {
            for i in -1..=1 {
                let bar_x = icon_x - BAR_WIDTH / 2.0 + i as f32 * (BAR_WIDTH + BAR_GAP);
                let bar = capsule(bar_x, cy - BAR_WIDTH / 2.0, BAR_WIDTH, BAR_WIDTH);
                fill(pixmap, bar, to_skia(p.text_muted, 1.0), at, Some(clip));
            }
        }
        let top = cy - self.lines.len() as f32 * ERROR_LINE / 2.0;
        for (i, line) in self.lines.iter().enumerate() {
            let baseline = top + i as f32 * ERROR_LINE + 13.5;
            draw_text(pixmap, line, ERROR_TEXT, x + MESSAGE_TEXT_X, baseline, to_skia(p.text, 1.0), scale);
        }
        if let Some((bx, by, bw, bh, fix)) = self.fix_button() {
            fill(pixmap, capsule(bx, by, bw, bh), to_skia(p.elevated, 1.0), at, Some(clip));
            let label_x = bx + (bw - text_width(fix.label(), ERROR_TEXT)) / 2.0;
            draw_text(pixmap, fix.label(), ERROR_TEXT, label_x, by + bh / 2.0 + 4.5, to_skia(p.text, 1.0), scale);
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

/// Room for the running wave's three bars after an action's name.
const RUNNING_BARS: f32 = 3.0 * BAR_WIDTH + 2.0 * 3.0;
const CAP_TEXT: f32 = 12.0;
const CAP_PAD: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Words,
    Quiet,
    Cap,
}

/// One piece of the confirm pill's line, placed from the pill's left edge.
struct Placed {
    kind: Part,
    text: String,
    x: f32,
    width: f32,
}

/// The confirm pill's line: "Run Deploy preview?", the action key's cap, "or", Esc's cap.
/// Returns the pieces and the pill's whole width.
fn confirm_layout(question: &str, keys: &str) -> (Vec<Placed>, f32) {
    let mut parts = Vec::new();
    let mut x = AFTER_SLOT;
    for (kind, text) in [(Part::Words, question), (Part::Cap, keys), (Part::Quiet, "or"), (Part::Cap, "Esc")] {
        let width = match kind {
            Part::Cap => text_width(text, CAP_TEXT) + 2.0 * CAP_PAD,
            Part::Words | Part::Quiet => text_width(text, ERROR_TEXT),
        };
        parts.push(Placed { kind, text: text.to_owned(), x, width });
        x += width + if kind == Part::Words { 8.0 } else { 6.0 };
    }
    (parts, x - 6.0 + 8.0)
}

/// `text` on one line of at most `max` wide, cut with an ellipsis if it must be.
fn one_line(text: &str, max: f32) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text_width(&flat, ERROR_TEXT) <= max {
        return flat;
    }
    let mut cut = flat;
    while text_width(&format!("{cut}\u{2026}"), ERROR_TEXT) > max && cut.pop().is_some() {}
    format!("{}\u{2026}", cut.trim_end())
}

/// The action key's mark: a four-point spark, 9.4px across, centred on (cx, cy).
fn draw_spark(pixmap: &mut Pixmap, clip: &Mask, cx: f32, cy: f32, at: Transform) {
    let (r, k) = (4.7, 1.0);
    let mut b = PathBuilder::new();
    b.move_to(cx, cy - r);
    b.quad_to(cx + k, cy - k, cx + r, cy);
    b.quad_to(cx + k, cy + k, cx, cy + r);
    b.quad_to(cx - k, cy + k, cx - r, cy);
    b.quad_to(cx - k, cy - k, cx, cy - r);
    b.close();
    fill(pixmap, b.finish(), to_skia(palette().text, 1.0), at, Some(clip));
}
