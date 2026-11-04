// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::core::window::Id as SurfaceId;
use uuid::Uuid;

use cosmic_sticky::model::geometry::{MIN_NOTE_SIZE, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragKind {
    Move,
    Resize,
}

/// A header drag or grip resize in progress. Coordinates are local to `surface`; the pointer
/// grab keeps reporting positions outside the surface while the button is held.
#[derive(Debug, Clone, Copy)]
pub struct Drag {
    pub note: Uuid,
    pub surface: SurfaceId,
    pub kind: DragKind,
    start_cursor: (f32, f32),
    start_rect: Rect,
}

impl Drag {
    pub fn new(
        note: Uuid,
        surface: SurfaceId,
        kind: DragKind,
        cursor: (f32, f32),
        rect: Rect,
    ) -> Self {
        Self {
            note,
            surface,
            kind,
            start_cursor: cursor,
            start_rect: rect,
        }
    }

    /// The note's rectangle with the cursor at `cursor`.
    pub fn rect_at(&self, cursor: (f32, f32)) -> Rect {
        let dx = cursor.0 - self.start_cursor.0;
        let dy = cursor.1 - self.start_cursor.1;
        let r = self.start_rect;
        match self.kind {
            DragKind::Move => r.translate(dx, dy).round(),
            DragKind::Resize => Rect::new(
                r.x,
                r.y,
                (r.width + dx).max(MIN_NOTE_SIZE.0),
                (r.height + dy).max(MIN_NOTE_SIZE.1),
            )
            .round(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(kind: DragKind) -> Drag {
        Drag::new(
            Uuid::nil(),
            SurfaceId::NONE,
            kind,
            (210.0, 205.0),
            Rect::new(200.0, 200.0, 260.0, 240.0),
        )
    }

    #[test]
    fn move_follows_cursor_delta() {
        let d = drag(DragKind::Move);
        assert_eq!(
            d.rect_at((210.0, 205.0)),
            Rect::new(200.0, 200.0, 260.0, 240.0)
        );
        assert_eq!(
            d.rect_at((710.0, 505.0)),
            Rect::new(700.0, 500.0, 260.0, 240.0)
        );
        assert_eq!(
            d.rect_at((0.4, -20.0)),
            Rect::new(-10.0, -25.0, 260.0, 240.0)
        );
    }

    #[test]
    fn move_beyond_surface_is_not_clamped_until_drop() {
        let d = drag(DragKind::Move);
        assert_eq!(d.rect_at((2100.0, 205.0)).x, 2090.0);
    }

    #[test]
    fn resize_grows_and_respects_minimum() {
        let d = drag(DragKind::Resize);
        assert_eq!(
            d.rect_at((310.0, 305.0)),
            Rect::new(200.0, 200.0, 360.0, 340.0)
        );
        assert_eq!(
            d.rect_at((-500.0, -500.0)),
            Rect::new(200.0, 200.0, MIN_NOTE_SIZE.0, MIN_NOTE_SIZE.1)
        );
    }
}
