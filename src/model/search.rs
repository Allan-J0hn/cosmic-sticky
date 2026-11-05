// SPDX-License-Identifier: GPL-3.0-only

use super::category::CategoryId;
use super::note::{Content, Note};

/// Every whitespace-separated term of `query` appears in the title or body (case-insensitive),
/// and the note is in `category` if one is given.
pub fn matches(note: &Note, query: &str, category: Option<&CategoryId>) -> bool {
    if category.is_some_and(|c| *c != note.category) {
        return false;
    }
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return true;
    }
    let haystack = format!("{}\n{}", note.title, body_text(note)).to_lowercase();
    terms.iter().all(|t| haystack.contains(t.as_str()))
}

fn body_text(note: &Note) -> String {
    match &note.content {
        Content::Text(text) => text.clone(),
        Content::Checklist(items) => items
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// First non-empty line of the body, for list previews.
pub fn snippet(note: &Note) -> String {
    body_text(note)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Categories, ChecklistItem, Rect};

    fn note(cat: &str, title: &str, content: Content) -> Note {
        let cats = Categories::default();
        let mut n = Note::new(
            cats.get(&CategoryId(cat.into())).unwrap(),
            Rect::new(0.0, 0.0, 260.0, 240.0),
            "eDP-1".into(),
            jiff::Timestamp::UNIX_EPOCH,
        );
        n.title = title.into();
        n.content = content;
        n
    }

    fn fixtures() -> Vec<Note> {
        vec![
            note(
                "reminders",
                "Dentist",
                Content::Text("Tuesday 3pm, bring forms".into()),
            ),
            note(
                "todos",
                "Groceries",
                Content::Checklist(vec![
                    ChecklistItem::new("Milk", false),
                    ChecklistItem::new("Oat flakes", true),
                ]),
            ),
            note(
                "self-care",
                "Évening walk",
                Content::Text("30 min, café on the way".into()),
            ),
            note("todos", "Taxes", Content::Text("forms due in April".into())),
        ]
    }

    fn titles(query: &str, cat: Option<&str>) -> Vec<String> {
        let cat = cat.map(|c| CategoryId(c.into()));
        fixtures()
            .into_iter()
            .filter(|n| matches(n, query, cat.as_ref()))
            .map(|n| n.title)
            .collect()
    }

    #[test]
    fn empty_query_matches_all() {
        assert_eq!(titles("", None).len(), 4);
        assert_eq!(titles("   ", None).len(), 4);
    }

    #[test]
    fn title_and_body_case_insensitive() {
        assert_eq!(titles("dentist", None), ["Dentist"]);
        assert_eq!(titles("FORMS", None), ["Dentist", "Taxes"]);
        assert_eq!(titles("oat", None), ["Groceries"]);
        assert_eq!(titles("évening", None), ["Évening walk"]);
        assert_eq!(titles("CAFÉ", None), ["Évening walk"]);
    }

    #[test]
    fn all_terms_must_match() {
        assert_eq!(titles("forms april", None), ["Taxes"]);
        assert!(titles("forms milk", None).is_empty());
    }

    #[test]
    fn category_filter_combines_with_query() {
        assert_eq!(titles("", Some("todos")), ["Groceries", "Taxes"]);
        assert_eq!(titles("forms", Some("todos")), ["Taxes"]);
        assert!(titles("dentist", Some("todos")).is_empty());
        assert!(titles("", Some("gone")).is_empty());
    }

    #[test]
    fn snippets() {
        let f = fixtures();
        assert_eq!(snippet(&f[0]), "Tuesday 3pm, bring forms");
        assert_eq!(snippet(&f[1]), "Milk");
        assert_eq!(
            snippet(&note("todos", "", Content::Text("\n  \n second".into()))),
            "second"
        );
    }
}
