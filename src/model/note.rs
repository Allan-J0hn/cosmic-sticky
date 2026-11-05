// SPDX-License-Identifier: GPL-3.0-only

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::category::{Categories, Category, CategoryId};
use super::color::Rgb;
use super::geometry::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Font {
    #[default]
    Sans,
    Serif,
    Mono,
    Handwriting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Text,
    Checklist,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChecklistItem {
    pub text: String,
    pub checked: bool,
}

impl ChecklistItem {
    pub fn new(text: impl Into<String>, checked: bool) -> Self {
        Self {
            text: text.into(),
            checked,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Checklist(Vec<ChecklistItem>),
}

impl Default for Content {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl Content {
    pub fn empty(mode: Mode) -> Self {
        match mode {
            Mode::Text => Self::Text(String::new()),
            Mode::Checklist => Self::Checklist(Vec::new()),
        }
    }

    pub fn mode(&self) -> Mode {
        match self {
            Self::Text(_) => Mode::Text,
            Self::Checklist(_) => Mode::Checklist,
        }
    }

    /// Converts between modes without losing text or checked state; blank lines are dropped
    /// when text becomes a checklist.
    pub fn into_mode(self, mode: Mode) -> Self {
        match (self, mode) {
            (Self::Text(text), Mode::Checklist) => Self::Checklist(parse_checklist(&text)),
            (Self::Checklist(items), Mode::Text) => Self::Text(render_checklist(&items)),
            (same, _) => same,
        }
    }

    /// Body as stored in the note file.
    pub fn to_markdown(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Checklist(items) => render_checklist(items),
        }
    }

    pub fn from_markdown(mode: Mode, body: &str) -> Self {
        match mode {
            Mode::Text => Self::Text(body.to_owned()),
            Mode::Checklist => Self::Checklist(parse_checklist(body)),
        }
    }
}

/// One item per non-blank line; `- [ ]`/`- [x]` task syntax sets the checked state.
pub fn parse_checklist(text: &str) -> Vec<ChecklistItem> {
    text.lines()
        .filter_map(|line| {
            parse_task(line).or_else(|| {
                (!line.trim().is_empty()).then(|| ChecklistItem::new(line.trim_end(), false))
            })
        })
        .collect()
}

pub fn render_checklist(items: &[ChecklistItem]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(if item.checked { "- [x] " } else { "- [ ] " });
        out.extend(
            item.text
                .chars()
                .map(|c| if c == '\n' || c == '\r' { ' ' } else { c }),
        );
    }
    out
}

fn parse_task(line: &str) -> Option<ChecklistItem> {
    let rest = line
        .trim_start()
        .strip_prefix(['-', '*', '+'])?
        .strip_prefix(' ')?;
    let (checked, rest) = if let Some(r) = rest.strip_prefix("[ ]") {
        (false, r)
    } else {
        (
            true,
            rest.strip_prefix("[x]")
                .or_else(|| rest.strip_prefix("[X]"))?,
        )
    };
    let text = if rest.trim().is_empty() {
        ""
    } else {
        rest.strip_prefix(' ')?
    };
    Some(ChecklistItem::new(text, checked))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    pub category: CategoryId,
    pub color_override: Option<Rgb>,
    pub font: Font,
    pub content: Content,
    pub rect: Rect,
    /// Connector name, e.g. `eDP-1`.
    pub output: String,
    /// Make and model, used when connector names change between boots.
    pub output_desc: Option<String>,
    pub created: Timestamp,
    pub modified: Timestamp,
}

impl Note {
    pub fn new(category: &Category, rect: Rect, output: String, now: Timestamp) -> Self {
        let mode = if category.checklist {
            Mode::Checklist
        } else {
            Mode::Text
        };
        Self {
            id: Uuid::new_v4(),
            title: String::new(),
            category: category.id.clone(),
            color_override: None,
            font: Font::default(),
            content: Content::empty(mode),
            rect,
            output,
            output_desc: None,
            created: now,
            modified: now,
        }
    }

    pub fn mode(&self) -> Mode {
        self.content.mode()
    }

    pub fn is_blank(&self) -> bool {
        self.title.trim().is_empty()
            && match &self.content {
                Content::Text(text) => text.trim().is_empty(),
                Content::Checklist(items) => items.iter().all(|i| i.text.trim().is_empty()),
            }
    }

    /// Category chosen while creating a note: a still-blank note also takes the category's
    /// starting mode.
    pub fn choose_initial_category(&mut self, category: &Category) {
        self.category = category.id.clone();
        if self.is_blank() {
            let mode = if category.checklist {
                Mode::Checklist
            } else {
                Mode::Text
            };
            self.content = Content::empty(mode);
        }
    }

    pub fn items_mut(&mut self) -> Option<&mut Vec<ChecklistItem>> {
        match &mut self.content {
            Content::Checklist(items) => Some(items),
            Content::Text(_) => None,
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.content = std::mem::take(&mut self.content).into_mode(mode);
    }

    pub fn color(&self, categories: &Categories) -> Rgb {
        self.color_override
            .unwrap_or_else(|| categories.resolve(&self.category).color)
    }

    pub fn text_color(&self, categories: &Categories) -> Rgb {
        self.color(categories).readable_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[(&str, bool)]) -> Vec<ChecklistItem> {
        list.iter()
            .map(|&(t, c)| ChecklistItem::new(t, c))
            .collect()
    }

    fn sample(cats: &Categories, category: &str) -> Note {
        let cat = cats.get(&CategoryId(category.into())).unwrap();
        Note::new(
            cat,
            Rect::new(10.0, 20.0, 260.0, 240.0),
            "eDP-1".into(),
            Timestamp::UNIX_EPOCH,
        )
    }

    #[test]
    fn text_lines_become_items() {
        let c = Content::Text("eggs\n\n  milk  \n- [x] bread\n* [X] jam\n+ [ ] tea\n".into());
        assert_eq!(
            c.into_mode(Mode::Checklist),
            Content::Checklist(items(&[
                ("eggs", false),
                ("  milk", false),
                ("bread", true),
                ("jam", true),
                ("tea", false),
            ]))
        );
    }

    #[test]
    fn checklist_to_text_to_checklist_is_identity() {
        let original = items(&[
            ("eggs", false),
            ("milk 🥛", true),
            ("", false),
            ("", true),
            ("- [ ] looks like a task", false),
            ("  indented", true),
            ("[x] brackets", false),
        ]);
        let text = Content::Checklist(original.clone()).into_mode(Mode::Text);
        assert_eq!(
            text.into_mode(Mode::Checklist),
            Content::Checklist(original)
        );
    }

    #[test]
    fn checklist_renders_as_task_list() {
        let c = Content::Checklist(items(&[("eggs", false), ("milk", true)]));
        assert_eq!(
            c.into_mode(Mode::Text),
            Content::Text("- [ ] eggs\n- [x] milk".into())
        );
    }

    #[test]
    fn newlines_inside_items_are_flattened() {
        assert_eq!(
            render_checklist(&items(&[("a\nb\r\nc", false)])),
            "- [ ] a b  c"
        );
    }

    #[test]
    fn crlf_and_malformed_tasks() {
        assert_eq!(
            parse_checklist("- [x] a\r\n-[ ] b\r\n- [x]c\r\n- [ ]\r\n- [y] d\r\n"),
            items(&[
                ("a", true),
                ("-[ ] b", false),
                ("- [x]c", false),
                ("", false),
                ("- [y] d", false)
            ])
        );
    }

    #[test]
    fn same_mode_is_unchanged() {
        let t = Content::Text("a\n\nb".into());
        assert_eq!(t.clone().into_mode(Mode::Text), t);
        assert_eq!(
            Content::from_markdown(Mode::Text, "x\r\ny"),
            Content::Text("x\r\ny".into())
        );
    }

    #[test]
    fn new_note_mode_follows_category() {
        let cats = Categories::default();
        assert_eq!(sample(&cats, "todos").mode(), Mode::Checklist);
        assert_eq!(sample(&cats, "reminders").mode(), Mode::Text);
    }

    #[test]
    fn set_mode_converts_content() {
        let cats = Categories::default();
        let mut n = sample(&cats, "reminders");
        n.content = Content::Text("a\nb".into());
        n.set_mode(Mode::Checklist);
        assert_eq!(
            n.content,
            Content::Checklist(items(&[("a", false), ("b", false)]))
        );
        n.set_mode(Mode::Text);
        assert_eq!(n.content, Content::Text("- [ ] a\n- [ ] b".into()));
    }

    #[test]
    fn color_override_and_reset() {
        let cats = Categories::default();
        let mut n = sample(&cats, "reminders");
        assert_eq!(n.color(&cats), Rgb::new(0xff, 0xd5, 0x4f));
        assert_eq!(n.text_color(&cats), Rgb::BLACK);
        n.color_override = Some(Rgb::new(0x1a, 0x23, 0x7e));
        assert_eq!(n.text_color(&cats), Rgb::WHITE);
        n.color_override = None;
        assert_eq!(n.color(&cats), Rgb::new(0xff, 0xd5, 0x4f));
    }

    #[test]
    fn initial_category_sets_mode_only_while_blank() {
        let cats = Categories::default();
        let todos = cats.get(&CategoryId("todos".into())).unwrap();
        let mut n = sample(&cats, "reminders");
        n.choose_initial_category(todos);
        assert_eq!(
            (n.category.0.as_str(), n.mode()),
            ("todos", Mode::Checklist)
        );

        let mut n = sample(&cats, "reminders");
        n.content = Content::Text("already typed".into());
        n.choose_initial_category(todos);
        assert_eq!(n.category, todos.id);
        assert_eq!(n.content, Content::Text("already typed".into()));
    }

    #[test]
    fn blank_notes() {
        let cats = Categories::default();
        let mut n = sample(&cats, "todos");
        assert!(n.is_blank());
        n.items_mut().unwrap().push(ChecklistItem::new(" ", false));
        assert!(n.is_blank());
        n.items_mut().unwrap().push(ChecklistItem::new("x", false));
        assert!(!n.is_blank());
        n.set_mode(Mode::Text);
        assert!(n.items_mut().is_none());
    }

    #[test]
    fn unknown_category_uses_first_color() {
        let cats = Categories::default();
        let mut n = sample(&cats, "reminders");
        n.category = CategoryId("gone".into());
        assert_eq!(n.color(&cats), cats.as_slice()[0].color);
    }
}
