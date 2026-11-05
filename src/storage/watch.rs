// SPDX-License-Identifier: GPL-3.0-only

//! Making sense of changes other programs (editors, Syncthing) make in the data directory.

use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use uuid::Uuid;

use super::{Store, format, fs, invalid_data};
use crate::model::{Categories, Note};

#[derive(Debug)]
pub enum Change {
    /// A note file was written. `note` is `None` while it doesn't parse (e.g. half-written).
    Note {
        id: Uuid,
        text: String,
        note: Option<Box<Note>>,
    },
    Removed(Uuid),
    Categories,
    Ignore,
}

impl Store {
    pub fn conflicts_dir(&self) -> PathBuf {
        self.root.join("conflicts")
    }

    /// What a change at `path` means, reading the file as it is now.
    pub fn classify(&self, path: &Path) -> Change {
        if path == self.categories_path() {
            return Change::Categories;
        }
        if path.parent() != Some(self.notes_dir().as_path()) {
            return Change::Ignore;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let Some(id) = name
            .strip_suffix(".md")
            .filter(|_| !name.starts_with('.'))
            .and_then(|stem| Uuid::try_parse(stem).ok())
        else {
            return Change::Ignore;
        };
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mtime = std::fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| Timestamp::try_from(t).ok())
                    .unwrap_or(Timestamp::UNIX_EPOCH);
                let note = format::note_from_markdown(&text, id, mtime)
                    .ok()
                    .map(|(note, _)| Box::new(note));
                Change::Note { id, text, note }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Change::Removed(id),
            Err(_) => Change::Ignore,
        }
    }

    /// Categories as they are on disk, without the repair `load_categories` does: a file
    /// caught mid-write must not be quarantined.
    pub fn read_categories(&self) -> io::Result<Categories> {
        let text = std::fs::read_to_string(self.categories_path())?;
        format::categories_from_yaml(&text).map_err(invalid_data)
    }

    pub fn note_text(note: &Note) -> io::Result<String> {
        format::note_to_markdown(note, None).map_err(invalid_data)
    }

    pub fn write_note_text(&self, id: Uuid, text: &str) -> io::Result<()> {
        fs::write_atomic(&self.note_path(id), text.as_bytes())
    }

    /// Keeps the version of a note that lost a conflict, outside `notes/` so it never loads
    /// as a second copy.
    pub fn save_conflict(&self, note: &Note, now: Timestamp) -> io::Result<PathBuf> {
        let dir = self.conflicts_dir();
        std::fs::create_dir_all(&dir)?;
        let stamp = now.strftime("%Y%m%dT%H%M%SZ");
        let path = dir.join(format!("{}.conflict-{stamp}.md", note.id));
        fs::write_atomic(&path, Self::note_text(note)?.as_bytes())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{note, store};
    use super::*;

    #[test]
    fn classifies_note_files() {
        let (_dir, s) = store();
        let n = note("a");
        s.save_note(&n).unwrap();
        match s.classify(&s.note_path(n.id)) {
            Change::Note { id, text, note } => {
                assert_eq!(id, n.id);
                assert_eq!(text, Store::note_text(&n).unwrap());
                assert_eq!(note.as_deref(), Some(&n));
            }
            other => panic!("{other:?}"),
        }
        std::fs::remove_file(s.note_path(n.id)).unwrap();
        assert!(matches!(s.classify(&s.note_path(n.id)), Change::Removed(id) if id == n.id));
    }

    #[test]
    fn half_written_note_does_not_parse() {
        let (_dir, s) = store();
        let n = note("a");
        std::fs::write(s.note_path(n.id), "---\ntitle: [").unwrap();
        assert!(matches!(
            s.classify(&s.note_path(n.id)),
            Change::Note { note: None, .. }
        ));
        assert!(
            s.note_path(n.id).exists(),
            "never quarantined from the watcher"
        );
    }

    #[test]
    fn ignores_temp_and_foreign_files() {
        let (_dir, s) = store();
        let id = Uuid::new_v4();
        for name in [
            format!(".{id}.md.tmp"),
            format!(".{id}.md"),
            "notes.txt".into(),
            "x.md".into(),
        ] {
            let path = s.notes_dir().join(name);
            std::fs::write(&path, "x").unwrap();
            assert!(
                matches!(s.classify(&path), Change::Ignore),
                "{}",
                path.display()
            );
        }
        assert!(matches!(
            s.classify(&s.trash_dir().join(format!("{id}.md"))),
            Change::Ignore
        ));
        assert!(matches!(
            s.classify(&s.root().join("categories.yaml")),
            Change::Categories
        ));
    }

    #[test]
    fn conflict_copy_is_outside_notes() {
        let (_dir, s) = store();
        let n = note("mine");
        let path = s
            .save_conflict(&n, "2026-11-05T10:00:00Z".parse().unwrap())
            .unwrap();
        assert_eq!(path.parent(), Some(s.conflicts_dir().as_path()));
        assert!(path.ends_with(format!("{}.conflict-20261105T100000Z.md", n.id)));
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("title: mine")
        );
        assert!(s.load_notes().unwrap().items.is_empty());
    }

    #[test]
    fn read_categories_never_repairs() {
        let (_dir, s) = store();
        s.save_categories(&Categories::default()).unwrap();
        assert_eq!(s.read_categories().unwrap(), Categories::default());
        std::fs::write(s.root().join("categories.yaml"), "- {id: [").unwrap();
        assert!(s.read_categories().is_err());
        assert!(s.root().join("categories.yaml").exists());
    }
}
