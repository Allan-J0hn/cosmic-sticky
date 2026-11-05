// SPDX-License-Identifier: GPL-3.0-only

//! Deleting notes into the trash, undo, restore, and the timed purge.

use std::time::Duration;

use cosmic::Element;
use cosmic::app::Task;
use cosmic::iced::task::Handle as TaskHandle;
use cosmic::iced::widget::{container, row};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, text};
use jiff::Timestamp;
use uuid::Uuid;

use super::{App, Message, NoteState, timer};
use crate::fl;
use cosmic_sticky::model::{self as model, Rect};
use cosmic_sticky::storage::TrashedNote;

const UNDO_TIMEOUT: Duration = Duration::from_secs(10);
pub const CHIP_SIZE: (f32, f32) = (240.0, 44.0);

/// "Note deleted — Undo", shown where the note was.
pub struct UndoChip {
    pub note: Uuid,
    pub output: String,
    pub rect: Rect,
    _timer: TaskHandle,
}

impl App {
    pub(super) fn delete_note(&mut self, id: Uuid) -> Task<Message> {
        let outputs = self.outputs();
        let Some(index) = self.notes.iter().position(|n| n.note.id == id) else {
            return Task::none();
        };
        let placement = self.placement(&self.notes[index].note, &outputs);
        let now = model::now();
        if let Some(store) = &self.store
            && let Err(e) = store.trash_note(&self.notes[index].note, now)
        {
            tracing::error!("moving note {id} to the trash: {e}");
            return Task::none();
        }
        let state = self.notes.remove(index);
        tracing::info!(note = %id, "note trashed");
        self.trash.push(TrashedNote {
            note: state.note,
            trashed_at: now,
        });
        if self.editing.is_some_and(|(n, _)| n == id) {
            self.editing = None;
        }
        if self.hovered == Some(id) {
            self.hovered = None;
        }
        let mut tasks = Vec::new();
        if self.reveal.as_ref().is_some_and(|r| r.note == id) {
            tasks.push(self.end_reveal());
        }
        if let Some((output, note_rect)) = placement {
            let size = outputs
                .iter()
                .find(|o| o.name == output)
                .map_or(CHIP_SIZE, |o| o.size);
            let x = note_rect.x.min(size.0 - CHIP_SIZE.0).max(0.0);
            let y = note_rect.y.min(size.1 - CHIP_SIZE.1).max(0.0);
            let (task, handle) = timer(UNDO_TIMEOUT, Message::UndoExpired(id));
            self.undo = Some(UndoChip {
                note: id,
                output,
                rect: Rect::new(x, y, CHIP_SIZE.0, CHIP_SIZE.1),
                _timer: handle,
            });
            tasks.push(task);
        }
        tasks.push(self.update_input_regions());
        tasks.push(self.schedule_purge());
        Task::batch(tasks)
    }

    pub(super) fn undo_expired(&mut self, id: Uuid) -> Task<Message> {
        if self.undo.as_ref().is_some_and(|u| u.note == id) {
            self.undo = None;
            return self.update_input_regions();
        }
        Task::none()
    }

    /// Moves a trashed note back onto the desk, where it was.
    pub(super) fn restore(&mut self, id: Uuid) -> Task<Message> {
        let note = match &self.store {
            Some(store) => match store.restore_note(id) {
                Ok(note) => note,
                Err(e) => {
                    tracing::error!("restoring note {id}: {e}");
                    return Task::none();
                }
            },
            None => match self.trash.iter().find(|t| t.note.id == id) {
                Some(t) => t.note.clone(),
                None => return Task::none(),
            },
        };
        tracing::info!(note = %id, "note restored");
        self.trash.retain(|t| t.note.id != id);
        if self.undo.as_ref().is_some_and(|u| u.note == id) {
            self.undo = None;
        }
        self.notes.push(NoteState::new(note));
        Task::batch([self.update_input_regions(), self.schedule_purge()])
    }

    pub(super) fn empty_trash(&mut self) -> Task<Message> {
        self.purge(Timestamp::MAX)
    }

    pub(super) fn purge(&mut self, now: Timestamp) -> Task<Message> {
        let Some(store) = &self.store else {
            return Task::none();
        };
        match store.purge_trash(now) {
            Ok(purge) => {
                if !purge.purged.is_empty() {
                    tracing::info!(count = purge.purged.len(), "trash purged");
                }
                self.trash.retain(|t| !purge.purged.contains(&t.note.id));
                if self
                    .undo
                    .as_ref()
                    .is_some_and(|u| purge.purged.contains(&u.note))
                {
                    self.undo = None;
                }
            }
            Err(e) => tracing::error!("purging trash: {e}"),
        }
        Task::batch([self.schedule_purge(), self.update_input_regions()])
    }

    /// One timer for the next trash item to expire; no polling.
    pub(super) fn schedule_purge(&mut self) -> Task<Message> {
        let next = self.trash.iter().map(TrashedNote::expires_at).min();
        let Some(next) = next else {
            self.purge_timer = None;
            return Task::none();
        };
        let wait = Duration::try_from(next.duration_since(model::now())).unwrap_or_default();
        let (task, handle) = timer(wait + Duration::from_secs(1), Message::PurgeDue);
        self.purge_timer = Some(handle);
        task
    }

    pub(super) fn undo_chip_view(&self, chip: &UndoChip) -> Element<'_, Message> {
        container(
            row![
                text::body(fl!("note-deleted")).width(Length::Fill),
                button::text(fl!("undo")).on_press(Message::Restore(chip.note)),
            ]
            .align_y(Alignment::Center)
            .padding([0, 4, 0, 14]),
        )
        .width(Length::Fixed(chip.rect.width))
        .height(Length::Fixed(chip.rect.height))
        .center_y(Length::Fixed(chip.rect.height))
        .class(cosmic::theme::Container::Dialog(false))
        .into()
    }
}
