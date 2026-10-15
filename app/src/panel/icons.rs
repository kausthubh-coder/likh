//! The window's small drawn icons: the sidebar's tab marks, the update button
//! and the close cross. Drawn, not taken from a font, so they share the
//! logo's stroke.
//!
//! A canvas paints its own backdrop (the software renderer leaves stroke rims
//! behind otherwise), so an icon never sits on a hover fill. It lights its
//! own ink instead, when the pointer is anywhere over the control it belongs to.

use iced::widget::canvas::{self, Frame, Path, Stroke, path::Arc};
use iced::{Color, Padding, Point, Radians, Rectangle, Renderer, Theme, Vector, border, mouse};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Mic,
    Wave,
    Code,
    /// A four-point spark: your actions.
    Spark,
    /// An open book: the dictionary.
    Book,
    Clock,
    Gear,
    /// A heartbeat line: diagnostics.
    Pulse,
    Refresh,
    /// A three-quarter ring, turned this far: checking for an update.
    Spin(f32),
    Up,
    Cross,
}

pub struct Icon {
    pub shape: Shape,
    pub ink: Color,
    /// The ink while the pointer is over the control.
    pub lit: Color,
    pub backdrop: Color,
    /// A rounded fill behind the mark, filling the canvas: the update button when there's one to install.
    pub plate: Option<Color>,
    /// How far the control reaches past the canvas on each side, for the pointer.
    pub reach: Padding,
}

impl<M> canvas::Program<M> for Icon {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.backdrop);
        if let Some(plate) = self.plate {
            frame.fill(&Path::rounded_rectangle(Point::ORIGIN, bounds.size(), border::radius(8.0)), plate);
        }
        let control = Rectangle {
            x: bounds.x - self.reach.left,
            y: bounds.y - self.reach.top,
            width: bounds.width + self.reach.left + self.reach.right,
            height: bounds.height + self.reach.top + self.reach.bottom,
        };
        let ink = if cursor.is_over(control) { self.lit } else { self.ink };
        // Shapes are drawn on a 16-unit square, centred in whatever the canvas is.
        let unit = 15.0 / 16.0;
        let side = 16.0 * unit;
        let origin = Vector::new((bounds.width - side) / 2.0, (bounds.height - side) / 2.0);
        let at = |x: f32, y: f32| Point::new(origin.x + x * unit, origin.y + y * unit);
        let line = Stroke::default()
            .with_width(1.5)
            .with_color(ink)
            .with_line_cap(canvas::LineCap::Round)
            .with_line_join(canvas::LineJoin::Round);
        let ring = |b: &mut canvas::path::Builder, start: f32, end: f32, r: f32| {
            b.arc(Arc {
                center: at(8.0, 8.0),
                radius: r * unit,
                start_angle: Radians(start.to_radians()),
                end_angle: Radians(end.to_radians()),
            });
        };
        let path = match self.shape {
            Shape::Mic => Path::new(|b| {
                b.rounded_rectangle(at(5.5, 1.5), iced::Size::new(5.0 * unit, 8.0 * unit), border::radius(2.5 * unit));
                b.move_to(at(3.0, 7.5));
                b.arc(Arc {
                    center: at(8.0, 7.5),
                    radius: 5.0 * unit,
                    start_angle: Radians(std::f32::consts::PI),
                    end_angle: Radians(0.0),
                });
                b.move_to(at(8.0, 12.5));
                b.line_to(at(8.0, 14.5));
            }),
            Shape::Wave => Path::new(|b| {
                for (x, top, bottom) in
                    [(2.0, 8.0, 8.0), (5.0, 5.5, 10.5), (8.0, 3.0, 13.0), (11.0, 5.5, 10.5), (14.0, 8.0, 8.0)]
                {
                    b.move_to(at(x, top));
                    b.line_to(at(x + if top == bottom { 0.5 } else { 0.0 }, bottom));
                }
            }),
            Shape::Code => Path::new(|b| {
                b.move_to(at(4.5, 3.5));
                b.line_to(at(2.0, 8.0));
                b.line_to(at(4.5, 12.5));
                b.move_to(at(11.5, 3.5));
                b.line_to(at(14.0, 8.0));
                b.line_to(at(11.5, 12.5));
                b.move_to(at(9.5, 3.0));
                b.line_to(at(6.5, 13.0));
            }),
            Shape::Book => Path::new(|b| {
                for edge in [2.0, 14.0] {
                    b.move_to(at(8.0, 4.0));
                    b.line_to(at(edge, 2.5));
                    b.line_to(at(edge, 12.0));
                    b.line_to(at(8.0, 13.5));
                }
                b.move_to(at(8.0, 4.0));
                b.line_to(at(8.0, 13.5));
            }),
            // The pill's action mark, outlined like the other tabs.
            Shape::Spark => Path::new(|b| {
                b.move_to(at(8.0, 1.5));
                b.quadratic_curve_to(at(9.3, 6.7), at(14.5, 8.0));
                b.quadratic_curve_to(at(9.3, 9.3), at(8.0, 14.5));
                b.quadratic_curve_to(at(6.7, 9.3), at(1.5, 8.0));
                b.quadratic_curve_to(at(6.7, 6.7), at(8.0, 1.5));
                b.close();
            }),
            Shape::Clock => Path::new(|b| {
                b.circle(at(8.0, 8.0), 6.0 * unit);
                b.move_to(at(8.0, 4.5));
                b.line_to(at(8.0, 8.0));
                b.line_to(at(10.5, 9.5));
            }),
            Shape::Gear => Path::new(|b| {
                b.circle(at(8.0, 8.0), 2.2 * unit);
                for step in 0..8 {
                    let angle = step as f32 * std::f32::consts::FRAC_PI_4;
                    let (cos, sin) = (angle.cos(), angle.sin());
                    b.move_to(at(8.0 + 4.5 * cos, 8.0 + 4.5 * sin));
                    b.line_to(at(8.0 + 6.5 * cos, 8.0 + 6.5 * sin));
                }
            }),
            Shape::Pulse => Path::new(|b| {
                b.move_to(at(1.5, 8.0));
                b.line_to(at(4.5, 8.0));
                b.line_to(at(6.5, 3.5));
                b.line_to(at(9.5, 12.5));
                b.line_to(at(11.5, 8.0));
                b.line_to(at(14.5, 8.0));
            }),
            Shape::Refresh => Path::new(|b| {
                ring(b, 0.0, 314.0, 5.0);
                b.move_to(at(13.0, 2.5));
                b.line_to(at(13.0, 5.5));
                b.line_to(at(10.0, 5.5));
            }),
            Shape::Spin(turned) => Path::new(|b| ring(b, turned, turned + 270.0, 5.0)),
            Shape::Up => Path::new(|b| {
                b.move_to(at(8.0, 13.0));
                b.line_to(at(8.0, 3.5));
                b.move_to(at(4.0, 7.5));
                b.line_to(at(8.0, 3.5));
                b.line_to(at(12.0, 7.5));
            }),
            Shape::Cross => Path::new(|b| {
                b.move_to(at(4.0, 4.0));
                b.line_to(at(12.0, 12.0));
                b.move_to(at(12.0, 4.0));
                b.line_to(at(4.0, 12.0));
            }),
        };
        frame.stroke(&path, line);
        vec![frame.into_geometry()]
    }
}
