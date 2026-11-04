// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseColorError(pub String);

impl fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid color {:?}, expected #rrggbb", self.0)
    }
}

impl std::error::Error for ParseColorError {}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// WCAG 2.x relative luminance.
    pub fn luminance(self) -> f64 {
        fn channel(c: u8) -> f64 {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    pub fn contrast_ratio(self, other: Rgb) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Black or white, whichever contrasts more with `self` as a background.
    pub fn readable_text(self) -> Rgb {
        if self.contrast_ratio(Rgb::BLACK) >= self.contrast_ratio(Rgb::WHITE) {
            Rgb::BLACK
        } else {
            Rgb::WHITE
        }
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl FromStr for Rgb {
    type Err = ParseColorError;

    /// Accepts `#rrggbb` or `#rgb`, with or without the `#`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseColorError(s.to_owned());
        let hex = s.trim();
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(err());
        }
        let digit =
            |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).map_err(|_| err());
        match hex.len() {
            6 => Ok(Rgb::new(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?)),
            3 => {
                let (r, g, b) = (digit(0, 1)?, digit(1, 1)?, digit(2, 1)?);
                Ok(Rgb::new(r * 17, g * 17, b * 17))
            }
            _ => Err(err()),
        }
    }
}

impl serde::Serialize for Rgb {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(s: &str) -> Rgb {
        s.parse().unwrap()
    }

    #[test]
    fn parses_long_and_short_hex() {
        assert_eq!(rgb("#ffd54f"), Rgb::new(0xff, 0xd5, 0x4f));
        assert_eq!(rgb("FFD54F"), Rgb::new(0xff, 0xd5, 0x4f));
        assert_eq!(rgb("#fa0"), Rgb::new(0xff, 0xaa, 0x00));
        assert_eq!(rgb("  #000000 "), Rgb::BLACK);
    }

    #[test]
    fn rejects_bad_hex() {
        for bad in [
            "",
            "#",
            "#ff",
            "#ffff",
            "#fffffff",
            "#gggggg",
            "#ff d5f",
            "#ффф",
            "rgb(1,2,3)",
        ] {
            assert!(bad.parse::<Rgb>().is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn formats_lowercase_and_round_trips() {
        let c = Rgb::new(0xAB, 0x01, 0xEF);
        assert_eq!(c.to_string(), "#ab01ef");
        assert_eq!(rgb(&c.to_string()), c);
    }

    #[test]
    fn luminance_extremes() {
        assert_eq!(Rgb::BLACK.luminance(), 0.0);
        assert!((Rgb::WHITE.luminance() - 1.0).abs() < 1e-12);
        assert!((Rgb::BLACK.contrast_ratio(Rgb::WHITE) - 21.0).abs() < 1e-9);
    }

    #[test]
    fn readable_text_on_sample_colors() {
        let black = [
            "#ffffff", "#ffd54f", "#fff59d", "#a5d6a7", "#90caf9", "#f8bbd0", "#ffcc80", "#80deea",
            "#ff0000", "#00ff00", "#00ffff", "#ffff00", "#ff00ff", "#e0e0e0", "#767676", "#777777",
        ];
        let white = [
            "#000000", "#0000ff", "#1a237e", "#4a148c", "#b71c1c", "#1b5e20", "#3e2723", "#424242",
            "#757575", "#747474", "#008000", "#800080",
        ];
        for c in black {
            assert_eq!(rgb(c).readable_text(), Rgb::BLACK, "{c}");
        }
        for c in white {
            assert_eq!(rgb(c).readable_text(), Rgb::WHITE, "{c}");
        }
    }

    #[test]
    fn readable_text_always_wins_contrast() {
        for v in (0..=255u8).step_by(3) {
            for c in [
                Rgb::new(v, v, v),
                Rgb::new(v, 255 - v, v / 2),
                Rgb::new(255, v, 0),
            ] {
                let chosen = c.readable_text();
                let other = if chosen == Rgb::BLACK {
                    Rgb::WHITE
                } else {
                    Rgb::BLACK
                };
                assert!(c.contrast_ratio(chosen) >= c.contrast_ratio(other), "{c}");
                assert!(c.contrast_ratio(chosen) >= 4.5, "{c}");
            }
        }
    }

    #[test]
    fn serde_uses_hex_string() {
        let yaml = serde_saphyr::to_string(&Rgb::new(1, 2, 3)).unwrap();
        assert_eq!(yaml.trim(), "\"#010203\"");
        assert_eq!(
            serde_saphyr::from_str::<Rgb>("\"#010203\"").unwrap(),
            Rgb::new(1, 2, 3)
        );
        assert!(serde_saphyr::from_str::<Rgb>("\"blue\"").is_err());
    }
}
