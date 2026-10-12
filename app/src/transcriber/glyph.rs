//! Each engine's mark, drawn with the same stroke as the onboarding art: a
//! terminal prompt for Codex, a bolt for Groq (the fast one), four panes for
//! Windows. Drawn, not icons from a font, so they match the logo's weight.

use super::Engine;
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

/// The glyph's box is about 32 x 24 at scale 1, centred on `center`.
pub fn draw(frame: &mut Frame, engine: Engine, center: Point, color: Color) {
    let at = |x: f32, y: f32| Point::new(center.x + x, center.y + y);
    let line = Stroke::default()
        .with_width(3.0)
        .with_color(color)
        .with_line_cap(canvas::LineCap::Round)
        .with_line_join(canvas::LineJoin::Round);
    match engine {
        Engine::Codex => {
            frame.stroke(
                &Path::new(|b| {
                    b.move_to(at(-16.0, -10.0));
                    b.line_to(at(-4.0, 0.0));
                    b.line_to(at(-16.0, 10.0));
                }),
                line,
            );
            frame.stroke(&Path::line(at(2.0, 11.0), at(16.0, 11.0)), line);
        }
        Engine::Groq => frame.stroke(
            &Path::new(|b| {
                b.move_to(at(4.0, -13.0));
                b.line_to(at(-7.0, 2.0));
                b.line_to(at(3.0, 2.0));
                b.line_to(at(-4.0, 13.0));
            }),
            line,
        ),
        Engine::Windows => {
            for (x, y) in [(-11.0, -11.0), (1.0, -11.0), (-11.0, 1.0), (1.0, 1.0)] {
                frame.fill(&Path::rounded_rectangle(at(x, y), Size::new(10.0, 10.0), 2.5.into()), color);
            }
        }
    }
}

/// One glyph as a widget, on its own backdrop.
pub struct Glyph {
    pub engine: Engine,
    pub color: Color,
    /// Painted first: the software renderer otherwise leaves stroke rims behind when it redraws.
    pub backdrop: Color,
}

impl<M> canvas::Program<M> for Glyph {
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
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.backdrop);
        let center = frame.center();
        draw(&mut frame, self.engine, center, self.color);
        vec![frame.into_geometry()]
    }
}
