// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};

use super::color::Rgb;
use super::note::Note;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CategoryId(pub String);

impl CategoryId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().simple().to_string())
    }
}

impl fmt::Display for CategoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    pub color: Rgb,
    /// New notes in this category start in checklist mode.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checklist: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryError {
    NotFound,
    EmptyName,
    DuplicateName,
    LastCategory,
    ReassignToSelf,
}

impl fmt::Display for CategoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "category not found",
            Self::EmptyName => "category name is empty",
            Self::DuplicateName => "a category with that name already exists",
            Self::LastCategory => "the last category cannot be deleted",
            Self::ReassignToSelf => "notes must move to a different category",
        })
    }
}

impl std::error::Error for CategoryError {}

/// Ordered, non-empty list of categories with unique names (case-insensitive).
#[derive(Debug, Clone, PartialEq)]
pub struct Categories(Vec<Category>);

impl Default for Categories {
    fn default() -> Self {
        let cat = |id: &str, name: &str, color: Rgb, checklist| Category {
            id: CategoryId(id.into()),
            name: name.into(),
            color,
            checklist,
        };
        Self(vec![
            cat("self-care", "Self care", Rgb::new(0xa5, 0xd6, 0xa7), false),
            cat("reminders", "Reminders", Rgb::new(0xff, 0xd5, 0x4f), false),
            cat("todos", "To-do's", Rgb::new(0x90, 0xca, 0xf9), true),
        ])
    }
}

impl Categories {
    /// Drops entries with empty or duplicate names or ids; falls back to the defaults if nothing is left.
    pub fn from_vec(list: Vec<Category>) -> Self {
        let mut out = Self(Vec::with_capacity(list.len()));
        for mut c in list {
            c.name = c.name.trim().to_owned();
            if !c.name.is_empty() && out.find_by_name(&c.name).is_none() && out.get(&c.id).is_none()
            {
                out.0.push(c);
            }
        }
        if out.0.is_empty() {
            Self::default()
        } else {
            out
        }
    }

    pub fn as_slice(&self) -> &[Category] {
        &self.0
    }

    pub fn iter(&self) -> impl Iterator<Item = &Category> {
        self.0.iter()
    }

    pub fn get(&self, id: &CategoryId) -> Option<&Category> {
        self.0.iter().find(|c| &c.id == id)
    }

    /// The category itself, or the first one if `id` is unknown (e.g. a note edited elsewhere).
    pub fn resolve(&self, id: &CategoryId) -> &Category {
        self.get(id).unwrap_or(&self.0[0])
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Category> {
        let name = name.trim().to_lowercase();
        self.0.iter().find(|c| c.name.to_lowercase() == name)
    }

    pub fn add(&mut self, name: &str, color: Rgb) -> Result<CategoryId, CategoryError> {
        let name = self.check_name(name, None)?;
        let id = CategoryId::generate();
        self.0.push(Category {
            id: id.clone(),
            name,
            color,
            checklist: false,
        });
        Ok(id)
    }

    pub fn rename(&mut self, id: &CategoryId, name: &str) -> Result<(), CategoryError> {
        let name = self.check_name(name, Some(id))?;
        self.get_mut(id)?.name = name;
        Ok(())
    }

    pub fn recolor(&mut self, id: &CategoryId, color: Rgb) -> Result<(), CategoryError> {
        self.get_mut(id)?.color = color;
        Ok(())
    }

    /// Removes `id` and moves its notes to `reassign_to`. Returns how many notes moved.
    pub fn delete<'a>(
        &mut self,
        id: &CategoryId,
        reassign_to: &CategoryId,
        notes: impl IntoIterator<Item = &'a mut Note>,
    ) -> Result<usize, CategoryError> {
        let index = self
            .0
            .iter()
            .position(|c| &c.id == id)
            .ok_or(CategoryError::NotFound)?;
        if self.0.len() == 1 {
            return Err(CategoryError::LastCategory);
        }
        if id == reassign_to {
            return Err(CategoryError::ReassignToSelf);
        }
        if self.get(reassign_to).is_none() {
            return Err(CategoryError::NotFound);
        }
        self.0.remove(index);
        let mut moved = 0;
        for note in notes {
            if &note.category == id {
                note.category = reassign_to.clone();
                moved += 1;
            }
        }
        Ok(moved)
    }

    fn get_mut(&mut self, id: &CategoryId) -> Result<&mut Category, CategoryError> {
        self.0
            .iter_mut()
            .find(|c| &c.id == id)
            .ok_or(CategoryError::NotFound)
    }

    fn check_name(&self, name: &str, except: Option<&CategoryId>) -> Result<String, CategoryError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CategoryError::EmptyName);
        }
        match self.find_by_name(name) {
            Some(c) if Some(&c.id) != except => Err(CategoryError::DuplicateName),
            _ => Ok(name.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Rect, note::Note};

    fn note_in(cats: &Categories, id: &str) -> Note {
        let cat = cats.get(&CategoryId(id.into())).unwrap();
        Note::new(
            cat,
            Rect::new(0.0, 0.0, 260.0, 240.0),
            "eDP-1".into(),
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    fn id(s: &str) -> CategoryId {
        CategoryId(s.into())
    }

    #[test]
    fn defaults() {
        let cats = Categories::default();
        let names: Vec<_> = cats.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Self care", "Reminders", "To-do's"]);
        assert!(cats.get(&id("todos")).unwrap().checklist);
        assert!(!cats.get(&id("reminders")).unwrap().checklist);
    }

    #[test]
    fn add_rename_recolor() {
        let mut cats = Categories::default();
        let work = cats.add("  Work ", Rgb::WHITE).unwrap();
        assert_eq!(cats.get(&work).unwrap().name, "Work");
        cats.rename(&work, "Job").unwrap();
        assert_eq!(cats.find_by_name("job").unwrap().id, work);
        cats.recolor(&work, Rgb::BLACK).unwrap();
        assert_eq!(cats.get(&work).unwrap().color, Rgb::BLACK);
        assert_eq!(cats.as_slice().len(), 4);
    }

    #[test]
    fn names_must_be_non_empty_and_unique() {
        let mut cats = Categories::default();
        assert_eq!(cats.add("   ", Rgb::WHITE), Err(CategoryError::EmptyName));
        assert_eq!(
            cats.add("reminders", Rgb::WHITE),
            Err(CategoryError::DuplicateName)
        );
        assert_eq!(
            cats.rename(&id("todos"), "SELF CARE"),
            Err(CategoryError::DuplicateName)
        );
        assert_eq!(cats.rename(&id("todos"), "to-do's"), Ok(()));
        assert_eq!(cats.find_by_name("TO-DO'S").unwrap().name, "to-do's");
        assert_eq!(cats.rename(&id("nope"), "x"), Err(CategoryError::NotFound));
        assert_eq!(
            cats.recolor(&id("nope"), Rgb::WHITE),
            Err(CategoryError::NotFound)
        );
    }

    #[test]
    fn unicode_names_compare_case_insensitively() {
        let mut cats = Categories::default();
        cats.add("Ébauches", Rgb::WHITE).unwrap();
        assert_eq!(
            cats.add("ébauches", Rgb::WHITE),
            Err(CategoryError::DuplicateName)
        );
    }

    #[test]
    fn delete_reassigns_every_note() {
        let mut cats = Categories::default();
        let mut notes = [
            note_in(&cats, "todos"),
            note_in(&cats, "self-care"),
            note_in(&cats, "todos"),
            note_in(&cats, "reminders"),
            note_in(&cats, "todos"),
        ];
        let moved = cats
            .delete(&id("todos"), &id("reminders"), notes.iter_mut())
            .unwrap();
        assert_eq!(moved, 3);
        assert!(cats.get(&id("todos")).is_none());
        assert!(notes.iter().all(|n| n.category != id("todos")));
        let in_reminders = notes
            .iter()
            .filter(|n| n.category == id("reminders"))
            .count();
        assert_eq!(in_reminders, 4);
        assert_eq!(notes.len(), 5);
    }

    #[test]
    fn delete_validates_before_changing_anything() {
        let mut cats = Categories::default();
        let mut notes = vec![note_in(&cats, "todos")];
        let before = (cats.clone(), notes.clone());
        assert_eq!(
            cats.delete(&id("todos"), &id("todos"), notes.iter_mut()),
            Err(CategoryError::ReassignToSelf)
        );
        assert_eq!(
            cats.delete(&id("todos"), &id("gone"), notes.iter_mut()),
            Err(CategoryError::NotFound)
        );
        assert_eq!(
            cats.delete(&id("gone"), &id("todos"), notes.iter_mut()),
            Err(CategoryError::NotFound)
        );
        assert_eq!((cats, notes), before);
    }

    #[test]
    fn cannot_delete_last_category() {
        let mut cats = Categories::default();
        cats.delete(&id("todos"), &id("self-care"), []).unwrap();
        cats.delete(&id("reminders"), &id("self-care"), []).unwrap();
        assert_eq!(cats.as_slice().len(), 1);
        for target in ["self-care", "todos", "x"] {
            assert_eq!(
                cats.delete(&id("self-care"), &id(target), []),
                Err(CategoryError::LastCategory)
            );
        }
        assert_eq!(cats.as_slice().len(), 1);
    }

    #[test]
    fn resolve_falls_back_to_first() {
        let cats = Categories::default();
        assert_eq!(cats.resolve(&id("deleted-elsewhere")).id, id("self-care"));
        assert_eq!(cats.resolve(&id("todos")).id, id("todos"));
    }

    #[test]
    fn from_vec_sanitizes() {
        let c = |i: &str, n: &str| Category {
            id: id(i),
            name: n.into(),
            color: Rgb::WHITE,
            checklist: false,
        };
        let cats = Categories::from_vec(vec![
            c("a", "A"),
            c("b", " a "),
            c("a", "B"),
            c("c", ""),
            c("d", "D"),
        ]);
        let ids: Vec<_> = cats.iter().map(|c| c.id.0.as_str()).collect();
        assert_eq!(ids, ["a", "d"]);
        assert_eq!(Categories::from_vec(vec![]), Categories::default());
    }
}
