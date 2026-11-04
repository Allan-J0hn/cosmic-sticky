// SPDX-License-Identifier: GPL-3.0-only

/// Axis-aligned rectangle in logical pixels, relative to an output's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
    }
}

/// The parts of `rects` that lie on an output of `output_size`, for the desk surface's input region.
pub fn input_region(rects: impl IntoIterator<Item = Rect>, output_size: (f32, f32)) -> Vec<Rect> {
    let bounds = Rect::new(0.0, 0.0, output_size.0, output_size.1);
    rects
        .into_iter()
        .filter_map(|r| r.intersection(&bounds))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: (f32, f32) = (1920.0, 1080.0);

    #[test]
    fn rect_inside_output_is_kept() {
        let r = Rect::new(100.0, 100.0, 260.0, 240.0);
        assert_eq!(input_region([r], OUTPUT), vec![r]);
    }

    #[test]
    fn rect_crossing_edge_is_clipped() {
        let r = Rect::new(1800.0, 1000.0, 260.0, 240.0);
        assert_eq!(
            input_region([r], OUTPUT),
            vec![Rect::new(1800.0, 1000.0, 120.0, 80.0)]
        );
    }

    #[test]
    fn rect_outside_output_is_dropped() {
        let r = Rect::new(-300.0, 50.0, 260.0, 240.0);
        assert!(input_region([r], OUTPUT).is_empty());
    }

    #[test]
    fn empty_rect_is_dropped() {
        assert!(input_region([Rect::new(10.0, 10.0, 0.0, 50.0)], OUTPUT).is_empty());
    }

    #[test]
    fn touching_edges_do_not_intersect() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 10.0, 10.0);
        assert_eq!(a.intersection(&b), None);
    }
}
