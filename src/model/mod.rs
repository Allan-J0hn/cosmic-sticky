// SPDX-License-Identifier: GPL-3.0-only

pub mod category;
pub mod color;
pub mod geometry;
pub mod note;

pub use category::{Categories, Category, CategoryError, CategoryId};
pub use color::Rgb;
pub use geometry::{OutputGeom, Rect};
pub use note::{ChecklistItem, Content, Font, Mode, Note};

/// Current time at second precision, so timestamps in note files stay readable.
pub fn now() -> jiff::Timestamp {
    let now = jiff::Timestamp::now();
    jiff::Timestamp::from_second(now.as_second()).unwrap_or(now)
}
