//! The app icon (docs/brand/logo/icon.svg) drawn into RGBA pixels, for the
//! window icon and the tray. Antialiased with signed distances, so it is sharp
//! at any size without shipping image files.

use super::theme::{oklch, palette};
use iced::Color;

/// Bar centres, half heights and the tile's corner radius, in the SVG's 512 units.
const BARS: [(f32, f32); 5] = [(128.0, 58.5), (192.0, 110.5), (256.0, 71.5), (320.0, 110.5), (384.0, 58.5)];
const BAR_HALF_WIDTH: f32 = 18.0;
const TILE_RADIUS: f32 = 116.0;

/// A `side` x `side` RGBA image of the icon, unpremultiplied, row by row.
pub fn rgba(side: u32) -> Vec<u8> {
    let p = palette();
    let (top, bottom) = (oklch(0.25, 0.016, 275.0), p.bg);
    let unit = side as f32 / 512.0;
    let mut out = Vec::with_capacity((side * side * 4) as usize);
    for py in 0..side {
        for px in 0..side {
            // Work in the SVG's units so every number above matches the file.
            let (x, y) = ((px as f32 + 0.5) / unit, (py as f32 + 0.5) / unit);
            let coverage = |distance: f32| (0.5 - distance * unit).clamp(0.0, 1.0);
            let tile = coverage(rounded_box(x, y, 256.0, 256.0, 256.0, 256.0, TILE_RADIUS));
            let bar = BARS
                .iter()
                .map(|&(cx, half_h)| coverage(rounded_box(x, y, cx, 256.0, BAR_HALF_WIDTH, half_h, BAR_HALF_WIDTH)))
                .fold(0.0, f32::max);
            let color = mix(mix(top, bottom, y / 512.0), p.accent, bar);
            out.extend([color.r, color.g, color.b].map(|c| (c * 255.0).round() as u8));
            out.push((tile * 255.0).round() as u8);
        }
    }
    out
}

/// Distance from (x, y) to a rounded box centred at (cx, cy); negative inside.
fn rounded_box(x: f32, y: f32, cx: f32, cy: f32, half_w: f32, half_h: f32, r: f32) -> f32 {
    let (dx, dy) = ((x - cx).abs() - half_w + r, (y - cy).abs() - half_h + r);
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - r
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color { r: a.r + (b.r - a.r) * t, g: a.g + (b.g - a.g) * t, b: a.b + (b.b - a.b) * t, a: 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(icon: &[u8], side: usize, x: usize, y: usize) -> [u8; 4] {
        let at = (y * side + x) * 4;
        [icon[at], icon[at + 1], icon[at + 2], icon[at + 3]]
    }

    #[test]
    fn the_icon_is_peach_bars_on_a_rounded_dusk_tile() {
        let icon = rgba(64);
        assert_eq!(icon.len(), 64 * 64 * 4);
        // The rounded corner is outside the tile.
        assert_eq!(pixel(&icon, 64, 0, 0)[3], 0);
        // The middle bar is the accent from the brand board, rgb(242, 160, 124).
        let bar = pixel(&icon, 64, 32, 32);
        assert!((i32::from(bar[0]) - 242).abs() <= 2 && (i32::from(bar[2]) - 124).abs() <= 2, "{bar:?}");
        // Between the first two bars: the dark tile, fully opaque.
        let gap = pixel(&icon, 64, 20, 32);
        assert_eq!(gap[3], 255);
        assert!(gap[0] < 50, "{gap:?}");
    }
}
