// SPDX-License-Identifier: GPL-3.0-only

pub mod format;
pub mod fs;
pub mod trash;

use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use uuid::Uuid;

use crate::model::{Categories, Note};
pub use format::{FormatError, Settings};
pub use trash::{Purge, TRASH_RETENTION, TrashedNote};

const APP_DIR: &str = "cosmic-sticky";

/// Items that loaded, plus the files that could not be parsed and were moved to `corrupt/`.
#[derive(Debug)]
pub struct Loaded<T> {
    pub items: Vec<T>,
    pub quarantined: Vec<PathBuf>,
}

/// The data directory: `notes/`, `trash/`, `corrupt/`, `categories.yaml`, `settings.yaml`.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// `$XDG_DATA_HOME/cosmic-sticky`, or `~/.local/share/cosmic-sticky`.
    pub fn default_root() -> Option<PathBuf> {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(data.join(APP_DIR))
    }

    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let store = Self { root: root.into() };
        std::fs::create_dir_all(store.notes_dir())?;
        std::fs::create_dir_all(store.trash_dir())?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn notes_dir(&self) -> PathBuf {
        self.root.join("notes")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.root.join("trash")
    }

    pub fn corrupt_dir(&self) -> PathBuf {
        self.root.join("corrupt")
    }

    fn categories_path(&self) -> PathBuf {
        self.root.join("categories.yaml")
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.yaml")
    }

    pub fn note_path(&self, id: Uuid) -> PathBuf {
        self.notes_dir().join(format!("{id}.md"))
    }

    pub fn save_note(&self, note: &Note) -> io::Result<()> {
        let text = format::note_to_markdown(note, None).map_err(invalid_data)?;
        fs::write_atomic(&self.note_path(note.id), text.as_bytes())
    }

    pub fn load_note(&self, id: Uuid) -> io::Result<Note> {
        read_note(&self.note_path(id)).map(|(note, _)| note)
    }

    pub fn load_notes(&self) -> io::Result<Loaded<Note>> {
        self.load_dir(&self.notes_dir(), |note, _| note)
    }

    pub fn load_categories(&self) -> io::Result<Categories> {
        self.load_yaml(&self.categories_path(), format::categories_from_yaml, |c| {
            self.save_categories(c)
        })
    }

    pub fn save_categories(&self, categories: &Categories) -> io::Result<()> {
        let yaml = format::categories_to_yaml(categories).map_err(invalid_data)?;
        fs::write_atomic(&self.categories_path(), yaml.as_bytes())
    }

    pub fn load_settings(&self) -> io::Result<Settings> {
        self.load_yaml(&self.settings_path(), format::settings_from_yaml, |s| {
            self.save_settings(s)
        })
    }

    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        let yaml = format::settings_to_yaml(settings).map_err(invalid_data)?;
        fs::write_atomic(&self.settings_path(), yaml.as_bytes())
    }

    /// Reads a YAML file; if it is missing or unparseable, the default is written in its place
    /// (an unparseable file is quarantined first).
    fn load_yaml<T: Default>(
        &self,
        path: &Path,
        parse: impl Fn(&str) -> Result<T, FormatError>,
        save: impl Fn(&T) -> io::Result<()>,
    ) -> io::Result<T> {
        match std::fs::read_to_string(path) {
            Ok(text) => match parse(&text) {
                Ok(value) => return Ok(value),
                Err(e) => {
                    tracing::warn!("{}: {e}", path.display());
                    fs::quarantine(path, &self.corrupt_dir())?;
                }
            },
            Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                fs::quarantine(path, &self.corrupt_dir())?;
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            Err(_) => {}
        }
        let value = T::default();
        save(&value)?;
        Ok(value)
    }

    fn load_dir<T>(
        &self,
        dir: &Path,
        make: impl Fn(Note, Option<Timestamp>) -> T,
    ) -> io::Result<Loaded<T>> {
        let mut loaded = Loaded {
            items: Vec::new(),
            quarantined: Vec::new(),
        };
        for path in fs::markdown_files(dir)? {
            match read_note(&path) {
                Ok((note, trashed_at)) => loaded.items.push(make(note, trashed_at)),
                Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                    tracing::warn!("{}: {e}", path.display());
                    loaded
                        .quarantined
                        .push(fs::quarantine(&path, &self.corrupt_dir())?);
                }
                Err(e) => tracing::warn!("skipping {}: {e}", path.display()),
            }
        }
        Ok(loaded)
    }
}

/// Reads a note file named `<uuid>.md`. Bad names, non-UTF-8 and parse errors are `InvalidData`.
fn read_note(path: &Path) -> io::Result<(Note, Option<Timestamp>)> {
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| Uuid::try_parse(s).ok())
        .ok_or_else(|| invalid_data("file name is not a note id"))?;
    let text = std::fs::read_to_string(path)?;
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| Timestamp::try_from(t).ok())
        .unwrap_or(Timestamp::UNIX_EPOCH);
    format::note_from_markdown(&text, id, mtime).map_err(invalid_data)
}

fn invalid_data(e: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CategoryId, Content, Rect, Rgb};

    pub(crate) fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("data")).unwrap();
        (dir, store)
    }

    pub(crate) fn note(title: &str) -> Note {
        let cats = Categories::default();
        let mut n = Note::new(
            cats.resolve(&CategoryId("reminders".into())),
            Rect::new(1.0, 2.0, 260.0, 240.0),
            "eDP-1".into(),
            "2026-11-04T17:00:00Z".parse().unwrap(),
        );
        n.title = title.into();
        n.content = Content::Text(format!("body of {title}"));
        n
    }

    #[test]
    fn open_creates_directories() {
        let (_dir, s) = store();
        assert!(s.notes_dir().is_dir());
        assert!(s.trash_dir().is_dir());
        assert!(!s.corrupt_dir().exists());
    }

    #[test]
    fn save_and_load_notes() {
        let (_dir, s) = store();
        let a = note("a");
        let b = note("b");
        s.save_note(&a).unwrap();
        s.save_note(&b).unwrap();
        let mut loaded = s.load_notes().unwrap();
        loaded.items.sort_by(|x, y| x.title.cmp(&y.title));
        assert_eq!(loaded.items, vec![a.clone(), b]);
        assert!(loaded.quarantined.is_empty());
        assert_eq!(s.load_note(a.id).unwrap(), a);
    }

    #[test]
    fn corrupt_notes_are_quarantined_not_deleted() {
        let (_dir, s) = store();
        let good = note("good");
        s.save_note(&good).unwrap();
        let bad_yaml = s.note_path(Uuid::new_v4());
        std::fs::write(&bad_yaml, "---\ntitle: [oops\n---\nprecious text").unwrap();
        let bad_utf8 = s.note_path(Uuid::new_v4());
        std::fs::write(&bad_utf8, b"---\ntitle: \xff\xfe\n---\n").unwrap();
        let bad_name = s.notes_dir().join("groceries.md");
        std::fs::write(&bad_name, "---\ntitle: fine\n---\n").unwrap();

        let loaded = s.load_notes().unwrap();
        assert_eq!(loaded.items, vec![good]);
        assert_eq!(loaded.quarantined.len(), 3);
        for p in [&bad_yaml, &bad_utf8, &bad_name] {
            assert!(!p.exists());
            let moved = s.corrupt_dir().join(p.file_name().unwrap());
            assert!(moved.exists(), "{}", moved.display());
        }
        let kept =
            std::fs::read_to_string(s.corrupt_dir().join(bad_yaml.file_name().unwrap())).unwrap();
        assert!(kept.ends_with("precious text"));
        assert_eq!(s.load_notes().unwrap().quarantined.len(), 0);
    }

    #[test]
    fn categories_default_then_persist() {
        let (_dir, s) = store();
        let mut cats = s.load_categories().unwrap();
        assert_eq!(cats, Categories::default());
        assert!(s.root().join("categories.yaml").exists());
        cats.add("Work", Rgb::new(1, 2, 3)).unwrap();
        s.save_categories(&cats).unwrap();
        assert_eq!(s.load_categories().unwrap(), cats);
    }

    #[test]
    fn corrupt_categories_are_quarantined_and_reset() {
        let (_dir, s) = store();
        std::fs::write(s.root().join("categories.yaml"), "- {id: [}").unwrap();
        assert_eq!(s.load_categories().unwrap(), Categories::default());
        assert_eq!(
            std::fs::read_to_string(s.corrupt_dir().join("categories.yaml")).unwrap(),
            "- {id: [}"
        );
        assert_eq!(s.load_categories().unwrap(), Categories::default());
    }

    #[test]
    fn settings_default_then_persist() {
        let (_dir, s) = store();
        assert!(s.load_settings().unwrap().notes_visible);
        s.save_settings(&Settings {
            notes_visible: false,
        })
        .unwrap();
        assert!(!s.load_settings().unwrap().notes_visible);
    }
}
