//! The murmuration: a flock of dots sweeps in and settles into a shape, the
//! logo when onboarding opens and the word "murmur" when it finishes.
//!
//! Every position is a pure function of the seconds since the flock started,
//! so the window only asks for frames while `t < duration()` and a test can
//! check any moment of it.

use super::mix;
use crate::shared::spring::{self, SpringSpec};
use crate::shared::theme::Palette;
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The logo: five bars tracing an "m" inside a capsule (BRAND.md, Logo).
    Mark,
    /// "murmur" spelled in dots.
    Word,
}

/// Bar heights of the mark, as a share of the 22-unit bar box.
const MARK_BARS: [f32; 5] = [0.45, 0.85, 0.55, 0.85, 0.45];

/// "mur" on a 5-row dot grid; the word repeats it.
const MUR: [&str; 5] =
    ["XX.X..X...X.X.XX", "X.X.X.X...X.XX..", "X.X.X.X...X.X...", "X.X.X.X...X.X...", "X.X.X..XXXX.X..."];

/// Seconds the flock flies before birds start peeling off to land.
const FLY: f32 = 1.0;
const LAND_FROM: f32 = 0.55;
/// Landing sweeps left to right across the shape over this long.
const SWEEP: f32 = 0.35;

impl Shape {
    fn spring(self) -> SpringSpec {
        match self {
            Shape::Mark => spring::DEFAULT,
            // Finishing onboarding is the one place the brand spends bounce.
            Shape::Word => spring::PLAYFUL,
        }
    }

    fn bird_count(self) -> usize {
        match self {
            Shape::Mark => 96,
            Shape::Word => 140,
        }
    }

    /// Seconds until every bird has landed and the shape stands still.
    pub fn duration(self) -> f32 {
        LAND_FROM + SWEEP + 0.1 + self.spring().duration * 1.6
    }
}

/// Where each landing spot is, relative to the centre of the art, and how big
/// a landed dot is.
pub(super) struct Layout {
    pub(super) slots: Vec<Point>,
    radius: f32,
    /// One mark unit in pixels (the mark is 64 x 40 units).
    unit: f32,
}

pub(super) fn layout(shape: Shape, size: Size) -> Layout {
    match shape {
        Shape::Mark => {
            let unit = (size.width / 64.0).min(size.height / 40.0).min(4.0) * 0.6;
            let radius = 2.0 * unit;
            let mut slots = Vec::new();
            for (i, share) in MARK_BARS.iter().enumerate() {
                let x = (17.0 + 8.0 * i as f32 - 32.0) * unit;
                let half = 22.0 * share * unit / 2.0 - radius;
                let count = ((2.0 * half) / (1.2 * radius)).ceil() as usize + 1;
                for k in 0..count {
                    let y = -half + 2.0 * half * k as f32 / (count - 1).max(1) as f32;
                    slots.push(Point::new(x, y));
                }
            }
            Layout { slots, radius, unit }
        }
        Shape::Word => {
            let columns = MUR[0].len() * 2 + 1;
            let pitch = (size.width * 0.8 / columns as f32).min(14.0);
            let mut slots = Vec::new();
            for copy in 0..2 {
                for (row, line) in MUR.iter().enumerate() {
                    for (col, cell) in line.chars().enumerate() {
                        if cell == 'X' {
                            let column = (copy * (MUR[0].len() + 1) + col) as f32;
                            slots.push(Point::new(
                                (column - (columns - 1) as f32 / 2.0) * pitch,
                                (row as f32 - 2.0) * pitch,
                            ));
                        }
                    }
                }
            }
            Layout { slots, radius: pitch * 0.36, unit: pitch }
        }
    }
}

/// A fixed pseudo-random number in 0..1, so the flock is the same every run.
fn noise(i: usize, salt: u64) -> f32 {
    let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// The flight path of the flock's centre, from off the left edge, up and
/// round, into the middle. Relative to the centre of the art.
fn path(u: f32, size: Size) -> Point {
    let (w, h) = (size.width, size.height);
    let p = [(-0.75 * w, 0.35 * h), (-0.1 * w, -0.65 * h), (0.5 * w, -0.25 * h), (0.0, 0.0)];
    let v = 1.0 - u;
    let b = [v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u];
    Point::new(b.iter().zip(&p).map(|(b, p)| b * p.0).sum(), b.iter().zip(&p).map(|(b, p)| b * p.1).sum())
}

pub struct Bird {
    pub position: Point,
    /// 0 while flying, 1 once landed (above 1 while a playful spring overshoots).
    pub landed: f32,
    pub radius: f32,
}

/// Every bird at `t` seconds, for art of `size`, relative to its centre.
pub fn birds(shape: Shape, size: Size, t: f32) -> Vec<Bird> {
    let layout = layout(shape, size);
    let left = layout.slots.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    let span = (layout.slots.iter().map(|p| p.x).fold(f32::MIN, f32::max) - left).max(1.0);
    (0..shape.bird_count())
        .map(|i| {
            let slot = layout.slots[i % layout.slots.len()];
            // Each bird trails the centre a little, so the flock stretches out like starlings do.
            let lag = 0.25 * noise(i, 1);
            let u = ((t - lag) / FLY).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - u).powi(3);
            let centre = path(eased, size);
            let angle = std::f32::consts::TAU * noise(i, 2);
            let reach = noise(i, 3).sqrt() * (1.0 - 0.5 * eased);
            let phase = std::f32::consts::TAU * noise(i, 4);
            let flying = Point::new(
                centre.x + angle.cos() * reach * size.width * 0.2 + (t * 6.0 + phase).sin() * 3.0,
                centre.y + angle.sin() * reach * size.height * 0.14 + (t * 5.0 + phase * 1.3).cos() * 3.0,
            );
            let land_at = LAND_FROM + SWEEP * (slot.x - left) / span + 0.1 * noise(i, 5);
            let landed = shape.spring().progress(t - land_at);
            Bird {
                position: Point::new(flying.x + (slot.x - flying.x) * landed, flying.y + (slot.y - flying.y) * landed),
                landed,
                radius: 1.6 + (layout.radius - 1.6) * landed.clamp(0.0, 1.0),
            }
        })
        .collect()
}

/// The canvas program. `t` is seconds since the flock started; `alpha` fades
/// the whole drawing with the step transition.
pub struct Flock {
    pub shape: Shape,
    pub t: f32,
    pub alpha: f32,
    pub colors: Colors,
}

#[derive(Clone, Copy)]
pub struct Colors {
    pub backdrop: Color,
    pub bird: Color,
    pub landed: Color,
    pub shell: Color,
}

impl Colors {
    pub fn from(p: &Palette) -> Self {
        Self { backdrop: p.bg, bird: p.text_muted, landed: p.accent, shell: p.surface }
    }
}

impl<Message> canvas::Program<Message> for Flock {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let centre = frame.center();
        let size = bounds.size();
        let fade = |c: Color, a: f32| Color { a: c.a * a * self.alpha, ..c };
        let settled = self.t >= self.shape.duration();
        // The software renderer repaints only each shape's exact bounds, which
        // misses the antialiased rim of a moving dot and leaves faint specks.
        // Painting the backdrop makes every frame repaint the whole art area.
        frame.fill_rectangle(Point::ORIGIN, size, self.colors.backdrop);

        if self.shape == Shape::Mark {
            // The capsule arrives as the last birds land: a small scale-in, not a pop.
            let unit = layout(self.shape, size).unit;
            let shown = spring::DEFAULT.progress(self.t - (self.shape.duration() - 0.55)).clamp(0.0, 1.0);
            let scale = 0.96 + 0.04 * shown;
            let shell = Size::new(64.0 * unit * scale, 40.0 * unit * scale);
            let corner = Point::new(centre.x - shell.width / 2.0, centre.y - shell.height / 2.0);
            let capsule = Path::rounded_rectangle(corner, shell, (shell.height / 2.0).into());
            frame.fill(&capsule, fade(self.colors.shell, shown));
            frame.stroke(
                &capsule,
                Stroke::default().with_width(1.0).with_color(fade(Color::from_rgba(1.0, 1.0, 1.0, 0.16), shown)),
            );
        }

        if settled && self.shape == Shape::Mark {
            // At rest, draw the bars as clean rounded rectangles instead of overlapping dots.
            let unit = layout(self.shape, size).unit;
            for (i, share) in MARK_BARS.iter().enumerate() {
                let bar = Size::new(4.0 * unit, 22.0 * share * unit);
                let x = centre.x + (15.0 + 8.0 * i as f32 - 32.0) * unit;
                let path =
                    Path::rounded_rectangle(Point::new(x, centre.y - bar.height / 2.0), bar, (2.0 * unit).into());
                frame.fill(&path, fade(self.colors.landed, 1.0));
            }
        } else {
            for bird in birds(self.shape, size, self.t) {
                let landed = bird.landed.clamp(0.0, 1.0);
                let color = mix(self.colors.bird, self.colors.landed, landed);
                let dot = Path::circle(Point::new(centre.x + bird.position.x, centre.y + bird.position.y), bird.radius);
                frame.fill(&dot, fade(color, 0.6 + 0.4 * landed));
            }
        }
        vec![frame.into_geometry()]
    }
}
