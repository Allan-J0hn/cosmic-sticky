// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::font::{Family, Weight};

use crate::fl;
use cosmic_sticky::model::Font;

pub const ALL: [Font; 4] = [Font::Sans, Font::Serif, Font::Mono, Font::Handwriting];

const HANDWRITING_FAMILY: &str = "Shadows Into Light";
const HANDWRITING_TTF: &[u8] = include_bytes!("../resources/fonts/ShadowsIntoLight.ttf");
const SERIF_FAMILY: &str = "Noto Serif";

/// Loads the bundled fonts into the renderer's global font system. `iced::font::load` is not
/// used because it is dropped while no window exists, which is how this app starts.
pub fn load() {
    use cosmic::iced::advanced::graphics::text::font_system;
    match font_system().write() {
        Ok(mut fonts) => fonts.load_font(HANDWRITING_TTF.into()),
        Err(e) => tracing::error!("loading bundled fonts: {e}"),
    }
}

/// Sans and Mono follow the COSMIC interface and monospace font settings. A missing family
/// falls back to the default font.
pub fn regular(font: Font) -> cosmic::iced::Font {
    match font {
        Font::Sans => cosmic::font::default(),
        Font::Mono => cosmic::font::mono(),
        Font::Serif => cosmic::iced::Font {
            family: Family::Name(SERIF_FAMILY),
            ..cosmic::iced::Font::DEFAULT
        },
        Font::Handwriting => cosmic::iced::Font {
            family: Family::Name(HANDWRITING_FAMILY),
            ..cosmic::iced::Font::DEFAULT
        },
    }
}

pub fn title(font: Font) -> cosmic::iced::Font {
    match font {
        Font::Handwriting => regular(font),
        _ => cosmic::iced::Font {
            weight: Weight::Semibold,
            ..regular(font)
        },
    }
}

/// The handwriting face is small for its nominal size.
pub fn scale(font: Font) -> f32 {
    match font {
        Font::Handwriting => 1.3,
        _ => 1.0,
    }
}

pub fn label(font: Font) -> String {
    match font {
        Font::Sans => fl!("font-sans"),
        Font::Serif => fl!("font-serif"),
        Font::Mono => fl!("font-mono"),
        Font::Handwriting => fl!("font-handwriting"),
    }
}
