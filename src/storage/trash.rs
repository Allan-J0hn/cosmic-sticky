// SPDX-License-Identifier: GPL-3.0-only

use std::io;
use std::path::PathBuf;

use jiff::{SignedDuration, Timestamp};
use uuid::Uuid;

use super::{Loaded, Store, format, fs, invalid_data, read_note};
use crate::model::Note;

pub const TRASH_RETENTION: SignedDuration = SignedDuration::from_hours(7 * 24);

#[derive(Debug, Clone, PartialEq)]
pub struct TrashedNote {
    pub note: Note,
    pub trashed_at: Timestamp,
}

impl TrashedNote {
    pub fn expires_at(&self) -> Timestamp {
        self.trashed_at
            .saturating_add(TRASH_RETENTION)
            .unwrap_or(Timestamp::MAX)
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Purge {
    pub purged: Vec<Uuid>,
    pub quarantined: Vec<PathBuf>,
    /// When the oldest remaining item expires, for scheduling the next purge.
    pub next_expiry: Option<Timestamp>,
}

impl Store {
    fn trash_path(&self, id: Uuid) -> PathBuf {
        self.trash_dir().join(format!("{id}.md"))
    }

    /// Writes `note` into the trash, then removes it from `notes/`.
    pub fn trash_note(&self, note: &Note, now: Timestamp) -> io::Result<()> {
        let text = format::note_to_markdown(note, Some(now)).map_err(invalid_data)?;
        fs::write_atomic(&self.trash_path(note.id), text.as_bytes())?;
        fs::remove_if_exists(&self.note_path(note.id))
    }

    /// Moves a note back to `notes/`. Fails without changes if a note with that id already exists.
    pub fn restore_note(&self, id: Uuid) -> io::Result<Note> {
        let target = self.note_path(id);
        if target.symlink_metadata().is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", target.display()),
            ));
        }
        let (note, _) = read_note(&self.trash_path(id))?;
        self.save_note(&note)?;
        fs::remove_if_exists(&self.trash_path(id))?;
        Ok(note)
    }

    /// Trashed notes; files without `trashed_at` count from their modification time.
    pub fn load_trash(&self) -> io::Result<Loaded<TrashedNote>> {
        self.load_dir(&self.trash_dir(), |note, trashed_at| TrashedNote {
            trashed_at: trashed_at.unwrap_or(note.modified),
            note,
        })
    }

    /// Permanently deletes trashed notes older than [`TRASH_RETENTION`].
    pub fn purge_trash(&self, now: Timestamp) -> io::Result<Purge> {
        let loaded = self.load_trash()?;
        let mut purge = Purge {
            quarantined: loaded.quarantined,
            ..Purge::default()
        };
        for item in loaded.items {
            let expires = item.expires_at();
            if expires <= now {
                fs::remove_if_exists(&self.trash_path(item.note.id))?;
                purge.purged.push(item.note.id);
            } else {
                purge.next_expiry = Some(purge.next_expiry.map_or(expires, |t| t.min(expires)));
            }
        }
        Ok(purge)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::tests::{note, store};

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn trash_and_restore() {
        let (_dir, s) = store();
        let n = note("keep me");
        s.save_note(&n).unwrap();
        let now = ts("2026-11-04T18:00:00Z");
        s.trash_note(&n, now).unwrap();

        assert!(s.load_notes().unwrap().items.is_empty());
        let trash = s.load_trash().unwrap().items;
        assert_eq!(
            trash,
            vec![TrashedNote {
                note: n.clone(),
                trashed_at: now
            }]
        );

        assert_eq!(s.restore_note(n.id).unwrap(), n);
        assert!(s.load_trash().unwrap().items.is_empty());
        assert_eq!(s.load_notes().unwrap().items, vec![n.clone()]);
        assert!(
            !std::fs::read_to_string(s.note_path(n.id))
                .unwrap()
                .contains("trashed_at")
        );
    }

    #[test]
    fn trashing_an_unsaved_note_keeps_its_latest_content() {
        let (_dir, s) = store();
        let mut n = note("draft");
        s.save_note(&n).unwrap();
        n.content = crate::model::Content::Text("edited after last autosave".into());
        s.trash_note(&n, ts("2026-11-04T18:00:00Z")).unwrap();
        assert_eq!(s.restore_note(n.id).unwrap(), n);

        let never_saved = note("never saved");
        s.trash_note(&never_saved, ts("2026-11-04T18:00:00Z"))
            .unwrap();
        assert_eq!(s.restore_note(never_saved.id).unwrap(), never_saved);
    }

    #[test]
    fn restore_does_not_overwrite_existing_note() {
        let (_dir, s) = store();
        let n = note("original");
        s.trash_note(&n, ts("2026-11-04T18:00:00Z")).unwrap();
        let mut newer = n.clone();
        newer.title = "synced back from elsewhere".into();
        s.save_note(&newer).unwrap();

        let err = s.restore_note(n.id).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(s.load_note(n.id).unwrap(), newer);
        assert_eq!(s.load_trash().unwrap().items.len(), 1);
    }

    #[test]
    fn restore_missing_is_not_found() {
        let (_dir, s) = store();
        assert_eq!(
            s.restore_note(Uuid::new_v4()).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn purge_only_items_older_than_seven_days() {
        let (_dir, s) = store();
        let now = ts("2026-11-20T12:00:00Z");
        let old = note("old");
        let exactly = note("exactly 7 days");
        let fresh = note("6 days 23 h");
        let newest = note("1 h");
        s.trash_note(&old, ts("2026-11-01T00:00:00Z")).unwrap();
        s.trash_note(&exactly, ts("2026-11-13T12:00:00Z")).unwrap();
        s.trash_note(&fresh, ts("2026-11-13T13:00:00Z")).unwrap();
        s.trash_note(&newest, ts("2026-11-20T11:00:00Z")).unwrap();

        let mut purge = s.purge_trash(now).unwrap();
        purge.purged.sort();
        let mut expected = vec![old.id, exactly.id];
        expected.sort();
        assert_eq!(purge.purged, expected);
        assert_eq!(purge.next_expiry, Some(ts("2026-11-20T13:00:00Z")));

        let left: Vec<_> = s
            .load_trash()
            .unwrap()
            .items
            .into_iter()
            .map(|t| t.note.title)
            .collect();
        assert_eq!(left.len(), 2);
        assert!(left.contains(&"6 days 23 h".to_string()) && left.contains(&"1 h".to_string()));

        let later = s.purge_trash(ts("2026-11-27T10:59:59Z")).unwrap();
        assert_eq!(later.purged, vec![fresh.id]);
        assert_eq!(later.next_expiry, Some(ts("2026-11-27T11:00:00Z")));
    }

    #[test]
    fn purge_never_deletes_live_notes_or_corrupt_files() {
        let (_dir, s) = store();
        let live = note("live");
        s.save_note(&live).unwrap();
        let bad = s.trash_dir().join(format!("{}.md", Uuid::new_v4()));
        std::fs::write(&bad, "not a note").unwrap();

        let purge = s.purge_trash(Timestamp::MAX).unwrap();
        assert!(purge.purged.is_empty());
        assert_eq!(purge.quarantined.len(), 1);
        assert_eq!(purge.next_expiry, None);
        assert_eq!(s.load_notes().unwrap().items, vec![live]);
        assert_eq!(
            std::fs::read_to_string(s.corrupt_dir().join(bad.file_name().unwrap())).unwrap(),
            "not a note"
        );
    }

    #[test]
    fn missing_trashed_at_counts_from_modified() {
        let (_dir, s) = store();
        let n = note("moved by hand");
        s.save_note(&n).unwrap();
        std::fs::rename(
            s.note_path(n.id),
            s.trash_dir().join(format!("{}.md", n.id)),
        )
        .unwrap();
        let trash = s.load_trash().unwrap().items;
        assert_eq!(trash[0].trashed_at, n.modified);
    }
}
