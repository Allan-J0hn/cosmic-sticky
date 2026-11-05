// SPDX-License-Identifier: GPL-3.0-only

//! Live reload of notes and categories changed by other programs.

use std::path::PathBuf;

use cosmic::app::Task;
use cosmic::iced::Subscription;
use cosmic::iced::futures::channel::mpsc;
use cosmic::iced::futures::{SinkExt, StreamExt};
use notify::event::{EventKind, ModifyKind};
use notify::{RecursiveMode, Watcher};
use uuid::Uuid;

use super::{App, Message, NoteState};
use cosmic_sticky::model::{self as model, Note};
use cosmic_sticky::storage::{Change, Store};

pub fn subscription(root: PathBuf) -> Subscription<Message> {
    Subscription::run_with(root, |root| {
        let root = root.clone();
        cosmic::iced::stream::channel(64, move |mut output: mpsc::Sender<Message>| async move {
            let (tx, mut rx) = mpsc::unbounded();
            // Reads (ours included) raise access events; only content and name changes matter.
            let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else { return };
                let relevant = match event.kind {
                    EventKind::Create(_) | EventKind::Remove(_) => true,
                    EventKind::Modify(ModifyKind::Metadata(_)) => false,
                    EventKind::Modify(_) => true,
                    _ => false,
                };
                if relevant {
                    for path in event.paths {
                        let _ = tx.unbounded_send(path);
                    }
                }
            });
            let mut watcher = match watcher {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("cannot watch the notes folder: {e}");
                    return;
                }
            };
            for dir in [root.join("notes"), root.clone()] {
                if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
                    tracing::error!("cannot watch {}: {e}", dir.display());
                }
            }
            while let Some(path) = rx.next().await {
                if output.send(Message::FileChanged(path)).await.is_err() {
                    break;
                }
            }
        })
    })
}

impl App {
    pub(super) fn on_file_changed(&mut self, path: PathBuf) -> Task<Message> {
        let Some(store) = &self.store else {
            return Task::none();
        };
        match store.classify(&path) {
            Change::Ignore => Task::none(),
            Change::Categories => {
                if let Ok(categories) = store.read_categories()
                    && categories != self.categories
                {
                    tracing::info!("categories reloaded");
                    self.categories = categories;
                }
                Task::none()
            }
            Change::Removed(id) => self.removed_externally(id),
            Change::Note { id, text, note } => {
                let Some(note) = note else {
                    tracing::debug!(note = %id, "external change does not parse (yet)");
                    return Task::none();
                };
                self.changed_externally(*note, text)
            }
        }
    }

    fn changed_externally(&mut self, note: Note, text: String) -> Task<Message> {
        let id = note.id;
        let editing = self.editing.is_some_and(|(e, _)| e == id);
        let Some(n) = self.note_mut(id) else {
            if self.trash.iter().any(|t| t.note.id == id) {
                return Task::none();
            }
            tracing::info!(note = %id, "note added externally");
            let mut state = NoteState::new(note);
            state.written = text;
            self.notes.push(state);
            return self.update_input_regions();
        };
        if text == n.written || note == n.note {
            n.written = text;
            return Task::none();
        }
        n.written = text;
        if editing || n.dirty() {
            tracing::info!(note = %id, "external change deferred while editing");
            n.pending = Some(note);
            return Task::none();
        }
        tracing::info!(note = %id, "note reloaded");
        n.note = note;
        n.sync_editors();
        self.update_input_regions()
    }

    fn removed_externally(&mut self, id: Uuid) -> Task<Message> {
        let Some(index) = self.notes.iter().position(|n| n.note.id == id) else {
            return Task::none();
        };
        tracing::info!(note = %id, "note removed externally");
        self.notes.remove(index);
        if self.editing.is_some_and(|(e, _)| e == id) {
            self.editing = None;
        }
        if self.hovered == Some(id) {
            self.hovered = None;
        }
        let mut tasks = vec![self.update_input_regions()];
        if self.reveal.as_ref().is_some_and(|r| r.note == id) {
            tasks.push(self.end_reveal());
        }
        Task::batch(tasks)
    }

    /// External versions that arrived while a note was being edited, once editing has ended
    /// without further local changes.
    pub(super) fn apply_pending(&mut self) -> Task<Message> {
        let editing = self.editing.map(|(id, _)| id);
        let mut changed = false;
        for n in &mut self.notes {
            if n.pending.is_some() && editing != Some(n.note.id) && !n.dirty() {
                tracing::info!(note = %n.note.id, "deferred external change applied");
                n.note = n.pending.take().expect("checked");
                n.sync_editors();
                changed = true;
            }
        }
        if changed {
            self.update_input_regions()
        } else {
            Task::none()
        }
    }

    /// Local edits win over an external version that arrived meanwhile; that version is kept
    /// as a conflict copy.
    pub(super) fn keep_conflict(store: &Store, pending: Option<Note>) {
        let Some(theirs) = pending else { return };
        match store.save_conflict(&theirs, model::now()) {
            Ok(path) => {
                tracing::warn!(note = %theirs.id, path = %path.display(), "conflict: kept the external version")
            }
            Err(e) => tracing::error!("saving conflict copy of {}: {e}", theirs.id),
        }
    }
}
