// SPDX-License-Identifier: GPL-3.0-only

use cosmic::iced::font::{Family, Weight};

use crate::fl;
use cosmic_sticky::model::Font;

pub const ALL: [Font; 10] = [
    Font::Sans,
    Font::Serif,
    Font::Mono,
    Font::Handwriting,
    Font::Neat,
    Font::Architect,
    Font::Marker,
    Font::Typewriter,
    Font::Script,
    Font::Terminal,
];

const SERIF_FAMILY: &str = "Noto Serif";

/// Single-weight faces shipped with the app (licenses next to them in resources/fonts).
const BUNDLED: [(Font, &str, &[u8]); 7] = [
    (
        Font::Handwriting,
        "Shadows Into Light",
        include_bytes!("../resources/fonts/ShadowsIntoLight.ttf"),
    ),
    (
        Font::Neat,
        "Patrick Hand",
        include_bytes!("../resources/fonts/PatrickHand-Regular.ttf"),
    ),
    (
        Font::Architect,
        "Architects Daughter",
        include_bytes!("../resources/fonts/ArchitectsDaughter-Regular.ttf"),
    ),
    (
        Font::Marker,
        "Permanent Marker",
        include_bytes!("../resources/fonts/PermanentMarker-Regular.ttf"),
    ),
    (
        Font::Typewriter,
        "Special Elite",
        include_bytes!("../resources/fonts/SpecialElite-Regular.ttf"),
    ),
    (
        Font::Script,
        "Pacifico",
        include_bytes!("../resources/fonts/Pacifico-Regular.ttf"),
    ),
    (
        Font::Terminal,
        "VT323",
        include_bytes!("../resources/fonts/VT323-Regular.ttf"),
    ),
];

fn bundled(font: Font) -> Option<&'static str> {
    BUNDLED
        .iter()
        .find(|(f, ..)| *f == font)
        .map(|(_, family, _)| *family)
}

/// Loads the bundled fonts into the renderer's global font system. `iced::font::load` is not
/// used because it is dropped while no window exists, which is how this app starts.
pub fn load() {
    use cosmic::iced::advanced::graphics::text::font_system;
    match font_system().write() {
        Ok(mut fonts) => {
            for (_, _, ttf) in BUNDLED {
                fonts.load_font(ttf.into());
            }
        }
        Err(e) => tracing::error!("loading bundled fonts: {e}"),
    }
}

/// Sans and Mono follow the COSMIC interface and monospace font settings. A missing family
/// falls back to the default font.
pub fn regular(font: Font) -> cosmic::iced::Font {
    let family = match font {
        Font::Sans => return cosmic::font::default(),
        Font::Mono => return cosmic::font::mono(),
        Font::Serif => SERIF_FAMILY,
        _ => bundled(font).unwrap_or_default(),
    };
    cosmic::iced::Font {
        family: Family::Name(family),
        ..cosmic::iced::Font::DEFAULT
    }
}

/// Bundled faces have one weight; asking for another would fall back to a different family.
pub fn title(font: Font) -> cosmic::iced::Font {
    if bundled(font).is_some() {
        return regular(font);
    }
    cosmic::iced::Font {
        weight: Weight::Semibold,
        ..regular(font)
    }
}

/// Evens out faces that look small (or large) for their nominal size.
pub fn scale(font: Font) -> f32 {
    match font {
        Font::Handwriting => 1.3,
        Font::Terminal => 1.35,
        Font::Neat => 1.15,
        Font::Script => 0.95,
        _ => 1.0,
    }
}

pub fn label(font: Font) -> String {
    match font {
        Font::Sans => fl!("font-sans"),
        Font::Serif => fl!("font-serif"),
        Font::Mono => fl!("font-mono"),
        Font::Handwriting => fl!("font-handwriting"),
        Font::Neat => fl!("font-neat"),
        Font::Architect => fl!("font-architect"),
        Font::Marker => fl!("font-marker"),
        Font::Typewriter => fl!("font-typewriter"),
        Font::Script => fl!("font-script"),
        Font::Terminal => fl!("font-terminal"),
    }
}
