// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Categories, Category, CategoryId, Content, Font, Mode, Note, Rect, Rgb};

use crate::model::geometry::DEFAULT_NOTE_SIZE;

const DEFAULT_WIDTH: f32 = DEFAULT_NOTE_SIZE.0;
const DEFAULT_HEIGHT: f32 = DEFAULT_NOTE_SIZE.1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    MissingFrontmatter,
    Yaml(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFrontmatter => f.write_str("no `---` frontmatter block"),
            Self::Yaml(e) => write!(f, "invalid YAML: {e}"),
        }
    }
}

impl std::error::Error for FormatError {}

fn yaml_err(e: impl fmt::Display) -> FormatError {
    FormatError::Yaml(e.to_string())
}

#[derive(Serialize, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    id: Option<Uuid>,
    #[serde(default, deserialize_with = "null_as_default")]
    title: String,
    #[serde(default, deserialize_with = "null_as_default")]
    category: CategoryId,
    #[serde(default)]
    color_override: Option<Rgb>,
    #[serde(default, deserialize_with = "font_or_default")]
    font: Font,
    #[serde(default, deserialize_with = "null_as_default")]
    mode: Mode,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default = "default_width")]
    width: f32,
    #[serde(default = "default_height")]
    height: f32,
    #[serde(default, deserialize_with = "null_as_default")]
    output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_desc: Option<String>,
    #[serde(default)]
    created: Option<Timestamp>,
    #[serde(default)]
    modified: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trashed_at: Option<Timestamp>,
}

/// `title:` with no value is YAML null; treat it like a missing field.
fn null_as_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// A font this version doesn't know (e.g. written by a newer one) falls back to the default
/// instead of making the whole note unreadable.
fn font_or_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Font, D::Error> {
    use serde::de::IntoDeserializer;
    let name = Option::<String>::deserialize(d)?;
    Ok(name
        .and_then(|n| {
            Font::deserialize(IntoDeserializer::<serde::de::value::Error>::into_deserializer(n))
                .ok()
        })
        .unwrap_or_default())
}

fn default_width() -> f32 {
    DEFAULT_WIDTH
}

fn default_height() -> f32 {
    DEFAULT_HEIGHT
}

/// A note file: YAML frontmatter between `---` lines, then the Markdown body.
pub fn note_to_markdown(note: &Note, trashed_at: Option<Timestamp>) -> Result<String, FormatError> {
    let fm = Frontmatter {
        id: Some(note.id),
        title: note.title.clone(),
        category: note.category.clone(),
        color_override: note.color_override,
        font: note.font,
        mode: note.mode(),
        x: note.rect.x,
        y: note.rect.y,
        width: note.rect.width,
        height: note.rect.height,
        output: note.output.clone(),
        output_desc: note.output_desc.clone(),
        created: Some(note.created),
        modified: Some(note.modified),
        trashed_at,
    };
    let yaml = serde_saphyr::to_string(&fm).map_err(yaml_err)?;
    let mut out = String::with_capacity(yaml.len() + 16);
    out.push_str("---\n");
    out.push_str(&yaml);
    if !yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(&note.content.to_markdown());
    Ok(out)
}

/// Parses a note file. `id` comes from the file name; `fallback_time` fills in missing timestamps.
pub fn note_from_markdown(
    text: &str,
    id: Uuid,
    fallback_time: Timestamp,
) -> Result<(Note, Option<Timestamp>), FormatError> {
    let (yaml, body) = split_frontmatter(text)?;
    let fm: Frontmatter = if yaml.trim().is_empty() {
        serde_saphyr::from_str("{}")
    } else {
        serde_saphyr::from_str(&yaml)
    }
    .map_err(yaml_err)?;
    let body = body.replace("\r\n", "\n");
    let note = Note {
        id,
        title: fm.title,
        category: fm.category,
        color_override: fm.color_override,
        font: fm.font,
        content: Content::from_markdown(fm.mode, &body),
        rect: Rect::new(fm.x, fm.y, fm.width, fm.height),
        output: fm.output,
        output_desc: fm.output_desc,
        created: fm.created.or(fm.modified).unwrap_or(fallback_time),
        modified: fm.modified.or(fm.created).unwrap_or(fallback_time),
    };
    Ok((note, fm.trashed_at))
}

fn split_frontmatter(text: &str) -> Result<(String, &str), FormatError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    let is_fence = |line: &str, fences: &[&str]| {
        fences.contains(&line.trim_end_matches(['\r', '\n']).trim_end())
    };
    let mut offset = match lines.next() {
        Some(first) if is_fence(first, &["---"]) => first.len(),
        _ => return Err(FormatError::MissingFrontmatter),
    };
    let mut yaml = String::new();
    for line in lines {
        offset += line.len();
        if is_fence(line, &["---", "..."]) {
            return Ok((yaml, &text[offset..]));
        }
        yaml.push_str(line.trim_end_matches(['\r', '\n']));
        yaml.push('\n');
    }
    Err(FormatError::MissingFrontmatter)
}

pub fn categories_to_yaml(categories: &Categories) -> Result<String, FormatError> {
    serde_saphyr::to_string(&categories.as_slice()).map_err(yaml_err)
}

pub fn categories_from_yaml(text: &str) -> Result<Categories, FormatError> {
    let list: Vec<Category> = serde_saphyr::from_str(text).map_err(yaml_err)?;
    Ok(Categories::from_vec(list))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "yes")]
    pub notes_visible: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notes_visible: true,
        }
    }
}

fn yes() -> bool {
    true
}

pub fn settings_to_yaml(settings: &Settings) -> Result<String, FormatError> {
    serde_saphyr::to_string(settings).map_err(yaml_err)
}

pub fn settings_from_yaml(text: &str) -> Result<Settings, FormatError> {
    if text.trim().is_empty() {
        return Ok(Settings::default());
    }
    serde_saphyr::from_str(text).map_err(yaml_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChecklistItem;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn sample() -> Note {
        let cats = Categories::default();
        let mut n = Note::new(
            cats.resolve(&CategoryId("reminders".into())),
            Rect::new(700.0, 500.5, 260.0, 240.0),
            "eDP-1".into(),
            ts("2026-11-04T17:00:00Z"),
        );
        n.modified = ts("2026-11-04T17:05:00Z");
        n.title = "Groceries".into();
        n.content = Content::Text("buy milk\nand bread".into());
        n
    }

    fn round_trip(note: &Note) -> Note {
        let md = note_to_markdown(note, None).unwrap();
        let (back, trashed) = note_from_markdown(&md, note.id, Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(trashed, None);
        back
    }

    #[test]
    fn plain_round_trip() {
        let n = sample();
        assert_eq!(round_trip(&n), n);
    }

    #[test]
    fn file_layout() {
        let n = sample();
        let md = note_to_markdown(&n, None).unwrap();
        assert!(md.starts_with("---\n"), "{md}");
        assert!(md.ends_with("\n---\nbuy milk\nand bread"), "{md}");
        assert!(md.contains("title: Groceries\n"), "{md}");
        assert!(md.contains("category: reminders\n"), "{md}");
        assert!(md.contains("mode: text\n"), "{md}");
        assert!(md.contains("created: \"2026-11-04T17:00:00Z\"\n"), "{md}");
        assert!(!md.contains("trashed_at"), "{md}");
    }

    #[test]
    fn unicode_and_yaml_special_characters() {
        let mut n = sample();
        n.title = "Ünïcödé: “quotes” # not a comment — 日本語 🎉 'single' \"double\"".into();
        n.content = Content::Text("---\nnot a fence\n# heading\nemoji 🧠\n...\n".into());
        n.output_desc = Some("Samsung Electric Company S27: 0x01".into());
        n.color_override = Some(Rgb::new(0x12, 0x34, 0x56));
        n.font = Font::Handwriting;
        assert_eq!(round_trip(&n), n);
    }

    #[test]
    fn titles_that_look_like_other_yaml_types() {
        for title in [
            "",
            "null",
            "~",
            "true",
            "no",
            "123",
            "1e3",
            "0x10",
            "- a",
            "[a]",
            "{a: b}",
            "  padded  ",
            "a\nb",
            "@at",
            "*star",
            "&amp",
            "!tag",
            "%pct",
            "|",
            ">",
        ] {
            let mut n = sample();
            n.title = title.into();
            assert_eq!(round_trip(&n).title, title, "title {title:?}");
        }
    }

    #[test]
    fn empty_body() {
        let mut n = sample();
        n.content = Content::Text(String::new());
        let md = note_to_markdown(&n, None).unwrap();
        assert!(md.ends_with("---\n"));
        assert_eq!(round_trip(&n), n);

        n.content = Content::Checklist(Vec::new());
        assert_eq!(round_trip(&n), n);
    }

    #[test]
    fn checklist_body() {
        let mut n = sample();
        n.content = Content::Checklist(vec![
            ChecklistItem::new("eggs", false),
            ChecklistItem::new("milk", true),
        ]);
        let md = note_to_markdown(&n, None).unwrap();
        assert!(md.contains("mode: checklist\n"));
        assert!(md.ends_with("---\n- [ ] eggs\n- [x] milk"));
        assert_eq!(round_trip(&n), n);
    }

    #[test]
    fn crlf_file() {
        let n = sample();
        let crlf = note_to_markdown(&n, None).unwrap().replace('\n', "\r\n");
        let (back, _) = note_from_markdown(&crlf, n.id, Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn bom_and_trailing_whitespace_on_fences() {
        let text = "\u{feff}--- \ntitle: x\n---  \nbody";
        let (n, _) = note_from_markdown(text, Uuid::nil(), Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(n.title, "x");
        assert_eq!(n.content, Content::Text("body".into()));
    }

    #[test]
    fn trashed_at_round_trips() {
        let n = sample();
        let when = ts("2026-11-10T08:00:00Z");
        let md = note_to_markdown(&n, Some(when)).unwrap();
        let (back, trashed) = note_from_markdown(&md, n.id, Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!((back, trashed), (n, Some(when)));
    }

    #[test]
    fn minimal_hand_written_note() {
        let id = Uuid::new_v4();
        let now = ts("2026-01-01T00:00:00Z");
        let (n, _) = note_from_markdown("---\ntitle: From phone\n---\nhello\n", id, now).unwrap();
        assert_eq!(n.id, id);
        assert_eq!(n.title, "From phone");
        assert_eq!(n.mode(), Mode::Text);
        assert_eq!(n.rect, Rect::new(0.0, 0.0, DEFAULT_WIDTH, DEFAULT_HEIGHT));
        assert_eq!((n.created, n.modified), (now, now));
        assert_eq!(n.content, Content::Text("hello\n".into()));

        let (n, _) = note_from_markdown("---\n---\n", id, now).unwrap();
        assert_eq!(n.title, "");
    }

    #[test]
    fn empty_values_mean_defaults() {
        let text = "---\ntitle:\ncategory:\nfont: ~\nmode: null\noutput:\ncolor_override:\n---\n";
        let (n, _) = note_from_markdown(text, Uuid::nil(), Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!(n.title, "");
        assert_eq!(n.mode(), Mode::Text);
        assert_eq!(n.output, "");
        assert_eq!(n.color_override, None);
    }

    #[test]
    fn every_font_round_trips_and_unknown_fonts_fall_back() {
        use crate::model::Font::*;
        for font in [
            Sans,
            Serif,
            Mono,
            Handwriting,
            Neat,
            Architect,
            Marker,
            Typewriter,
            Script,
            Terminal,
        ] {
            let mut n = sample();
            n.font = font;
            let md = note_to_markdown(&n, None).unwrap();
            assert_eq!(
                note_from_markdown(&md, n.id, Timestamp::UNIX_EPOCH)
                    .unwrap()
                    .0
                    .font,
                font
            );
        }
        let text = "---\ntitle: hi\nfont: wingdings\n---\nbody";
        let (n, _) = note_from_markdown(text, Uuid::nil(), Timestamp::UNIX_EPOCH).unwrap();
        assert_eq!((n.title.as_str(), n.font), ("hi", Font::Sans));
    }

    #[test]
    fn file_name_id_wins_over_frontmatter() {
        let n = sample();
        let md = note_to_markdown(&n, None).unwrap();
        let other = Uuid::new_v4();
        assert_eq!(
            note_from_markdown(&md, other, Timestamp::UNIX_EPOCH)
                .unwrap()
                .0
                .id,
            other
        );
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let text = "---\ntitle: x\nfuture_field: 3\n---\n";
        assert!(note_from_markdown(text, Uuid::nil(), Timestamp::UNIX_EPOCH).is_ok());
    }

    #[test]
    fn invalid_files() {
        let bad = [
            "",
            "just text",
            "---\ntitle: x\n",
            "title: x\n---\n",
            "---\ntitle: [unclosed\n---\n",
            "---\ncolor_override: \"#zzzzzz\"\n---\n",
            "---\nmode: sideways\n---\n",
            "---\nx: left\n---\n",
            "---\ncreated: yesterday\n---\n",
            "---\n- a list\n---\n",
        ];
        for text in bad {
            assert!(
                note_from_markdown(text, Uuid::nil(), Timestamp::UNIX_EPOCH).is_err(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn categories_round_trip() {
        let mut cats = Categories::default();
        cats.add("Ébauches: 草稿", Rgb::new(1, 2, 3)).unwrap();
        let yaml = categories_to_yaml(&cats).unwrap();
        assert_eq!(categories_from_yaml(&yaml).unwrap(), cats);
        assert!(categories_from_yaml("- id: a\n  name: A\n").is_err());
        assert_eq!(categories_from_yaml("[]").unwrap(), Categories::default());
    }

    #[test]
    fn settings_round_trip() {
        let s = Settings {
            notes_visible: false,
        };
        assert_eq!(
            settings_from_yaml(&settings_to_yaml(&s).unwrap()).unwrap(),
            s
        );
        assert_eq!(settings_from_yaml("").unwrap(), Settings::default());
        assert_eq!(settings_from_yaml("{}").unwrap(), Settings::default());
    }
}
