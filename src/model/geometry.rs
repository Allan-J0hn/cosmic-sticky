// SPDX-License-Identifier: GPL-3.0-only

pub const MIN_NOTE_SIZE: (f32, f32) = (160.0, 120.0);
pub const DEFAULT_NOTE_SIZE: (f32, f32) = (260.0, 240.0);
const NEW_NOTE_GAP: f32 = 16.0;

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

    pub fn contains(&self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn translate(self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    /// Shrinks the rect to fit `size` if needed, then moves it fully inside `(0, 0, size)`.
    /// Non-finite values (e.g. from a hand-edited file) are replaced with defaults.
    pub fn clamp_into(self, (out_w, out_h): (f32, f32)) -> Rect {
        let finite = |v: f32, default: f32| if v.is_finite() { v } else { default };
        let width = finite(self.width, MIN_NOTE_SIZE.0)
            .max(MIN_NOTE_SIZE.0)
            .min(out_w.max(1.0));
        let height = finite(self.height, MIN_NOTE_SIZE.1)
            .max(MIN_NOTE_SIZE.1)
            .min(out_h.max(1.0));
        let x = finite(self.x, 0.0).clamp(0.0, (out_w - width).max(0.0));
        let y = finite(self.y, 0.0).clamp(0.0, (out_h - height).max(0.0));
        Rect::new(x, y, width, height)
    }

    pub fn round(self) -> Rect {
        Rect::new(
            self.x.round(),
            self.y.round(),
            self.width.round(),
            self.height.round(),
        )
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

/// Where a new note of `size` goes next to `parent`: to its right, else to its left, else
/// offset over it; always fully on the output.
pub fn beside(parent: Rect, size: (f32, f32), output_size: (f32, f32)) -> Rect {
    let (w, h) = size;
    let right = parent.right() + NEW_NOTE_GAP;
    let left = parent.x - NEW_NOTE_GAP - w;
    let (x, y) = if right + w <= output_size.0 {
        (right, parent.y)
    } else if left >= 0.0 {
        (left, parent.y)
    } else {
        (parent.x + 24.0, parent.y + 24.0)
    };
    Rect::new(x, y, w, h).clamp_into(output_size).round()
}

/// A connected output in the global compositor space.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputGeom {
    pub name: String,
    pub description: Option<String>,
    pub position: (f32, f32),
    pub size: (f32, f32),
}

impl OutputGeom {
    pub fn bounds(&self) -> Rect {
        Rect::new(self.position.0, self.position.1, self.size.0, self.size.1)
    }
}

/// The output a note belongs on: by connector name, then by description, then the top-left
/// output. The note's own record is left alone, so it returns when its output reconnects.
pub fn place<'a>(
    name: &str,
    description: Option<&str>,
    outputs: &'a [OutputGeom],
) -> Option<&'a OutputGeom> {
    outputs
        .iter()
        .find(|o| o.name == name)
        .or_else(|| {
            let desc = description.filter(|d| !d.is_empty())?;
            outputs
                .iter()
                .find(|o| o.description.as_deref() == Some(desc))
        })
        .or_else(|| {
            outputs.iter().min_by(|a, b| {
                (a.position.1, a.position.0, &a.name)
                    .partial_cmp(&(b.position.1, b.position.0, &b.name))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
}

/// Where a note dropped at `cursor` (global) ends up: the output under the cursor, or `from`
/// if the cursor is between outputs. `rect` is relative to `from`; the result is relative to
/// the returned output and fully on it.
pub fn hand_off<'a>(
    rect: Rect,
    from: &'a OutputGeom,
    cursor: (f32, f32),
    outputs: &'a [OutputGeom],
) -> (&'a OutputGeom, Rect) {
    let to = outputs
        .iter()
        .find(|o| o.bounds().contains(cursor))
        .unwrap_or(from);
    let moved = rect.translate(
        from.position.0 - to.position.0,
        from.position.1 - to.position.1,
    );
    (to, moved.clamp_into(to.size).round())
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

    fn out(name: &str, x: f32, y: f32) -> OutputGeom {
        OutputGeom {
            name: name.into(),
            description: Some(format!("Make Model {name}")),
            position: (x, y),
            size: OUTPUT,
        }
    }

    #[test]
    fn clamp_moves_inside() {
        let r = Rect::new(1800.0, -20.0, 260.0, 240.0).clamp_into(OUTPUT);
        assert_eq!(r, Rect::new(1660.0, 0.0, 260.0, 240.0));
        let inside = Rect::new(10.0, 10.0, 260.0, 240.0);
        assert_eq!(inside.clamp_into(OUTPUT), inside);
    }

    #[test]
    fn clamp_shrinks_to_small_output_and_enforces_minimum() {
        assert_eq!(
            Rect::new(50.0, 50.0, 3000.0, 2000.0).clamp_into((800.0, 600.0)),
            Rect::new(0.0, 0.0, 800.0, 600.0)
        );
        assert_eq!(
            Rect::new(5.0, 5.0, 10.0, 10.0).clamp_into(OUTPUT),
            Rect::new(5.0, 5.0, MIN_NOTE_SIZE.0, MIN_NOTE_SIZE.1)
        );
    }

    #[test]
    fn clamp_repairs_non_finite_values() {
        let r = Rect::new(f32::NAN, f32::INFINITY, f32::NAN, -f32::INFINITY).clamp_into(OUTPUT);
        assert!([r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite()));
        assert_eq!(r, r.clamp_into(OUTPUT));
    }

    #[test]
    fn place_by_name_then_description_then_top_left() {
        let outputs = [out("HDMI-A-1", 1920.0, 0.0), out("eDP-1", 0.0, 0.0)];
        assert_eq!(place("HDMI-A-1", None, &outputs).unwrap().name, "HDMI-A-1");
        assert_eq!(
            place("DP-3", Some("Make Model HDMI-A-1"), &outputs)
                .unwrap()
                .name,
            "HDMI-A-1"
        );
        assert_eq!(
            place("DP-3", Some("Other"), &outputs).unwrap().name,
            "eDP-1"
        );
        assert_eq!(place("", None, &outputs).unwrap().name, "eDP-1");
        assert_eq!(place("eDP-1", None, &[]), None);
    }

    #[test]
    fn hand_off_to_output_under_cursor() {
        let outputs = [out("eDP-1", 0.0, 0.0), out("HDMI-A-1", 1920.0, 0.0)];
        let rect = Rect::new(1850.0, 300.0, 260.0, 240.0);
        let (to, r) = hand_off(rect, &outputs[0], (1990.0, 310.0), &outputs);
        assert_eq!(to.name, "HDMI-A-1");
        assert_eq!(r, Rect::new(0.0, 300.0, 260.0, 240.0));

        let rect = Rect::new(-100.0, 300.0, 260.0, 240.0);
        let (to, r) = hand_off(rect, &outputs[1], (1900.0, 320.0), &outputs);
        assert_eq!(to.name, "eDP-1");
        assert_eq!(r, Rect::new(1660.0, 300.0, 260.0, 240.0));
    }

    #[test]
    fn hand_off_stays_when_cursor_is_on_same_output_or_in_a_gap() {
        let outputs = [out("eDP-1", 0.0, 0.0), out("HDMI-A-1", 1920.0, 200.0)];
        let rect = Rect::new(1800.0, 40.5, 260.0, 240.0);
        let (to, r) = hand_off(rect, &outputs[0], (1950.0, 50.0), &outputs);
        assert_eq!(to.name, "eDP-1");
        assert_eq!(r, Rect::new(1660.0, 41.0, 260.0, 240.0));
    }

    #[test]
    fn new_note_goes_right_then_left_then_offset() {
        let size = DEFAULT_NOTE_SIZE;
        let parent = Rect::new(100.0, 100.0, 260.0, 240.0);
        assert_eq!(
            beside(parent, size, OUTPUT),
            Rect::new(376.0, 100.0, 260.0, 240.0)
        );
        let parent = Rect::new(1600.0, 100.0, 260.0, 240.0);
        assert_eq!(
            beside(parent, size, OUTPUT),
            Rect::new(1324.0, 100.0, 260.0, 240.0)
        );
        let wide = Rect::new(100.0, 900.0, 1700.0, 240.0);
        assert_eq!(
            beside(wide, size, OUTPUT),
            Rect::new(124.0, 840.0, 260.0, 240.0)
        );
    }

    #[test]
    fn touching_edges_do_not_intersect() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 10.0, 10.0);
        assert_eq!(a.intersection(&b), None);
    }
}
