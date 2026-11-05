// SPDX-License-Identifier: GPL-3.0-only

//! Showing one note above all windows, from the list view.

use std::time::Duration;

use cosmic::app::Task;
use cosmic::iced::core::window::Id as SurfaceId;
use cosmic::iced::task::Handle as TaskHandle;
use uuid::Uuid;

use super::{App, Message, timer};

/// The reveal ends this long after the last interaction with the note.
const REVEAL_TIMEOUT: Duration = Duration::from_secs(8);

pub struct Reveal {
    pub note: Uuid,
    pub surface: SurfaceId,
    generation: u64,
    _timer: TaskHandle,
}

impl App {
    /// Raises the note's desk above windows, showing only that note, until Escape, a click
    /// outside it, or [`REVEAL_TIMEOUT`] without interaction.
    pub(super) fn reveal(&mut self, id: Uuid) -> Task<Message> {
        let mut tasks = vec![self.end_reveal()];
        let outputs = self.outputs();
        let Some(n) = self.notes.iter().find(|n| n.note.id == id) else {
            return Task::batch(tasks);
        };
        let Some((output, _)) = self.placement(&n.note, &outputs) else {
            return Task::batch(tasks);
        };
        let Some(desk) = self.desks.values().find(|d| d.geom.name == output) else {
            return Task::batch(tasks);
        };
        let surface = desk.surface;
        tasks.push(desk.raise());
        if let Some(i) = self.notes.iter().position(|n| n.note.id == id) {
            let n = self.notes.remove(i);
            self.notes.push(n);
        }
        self.reveal_generation += 1;
        let (task, handle) = timer(
            REVEAL_TIMEOUT,
            Message::RevealExpired(self.reveal_generation),
        );
        tasks.push(task);
        self.reveal = Some(Reveal {
            note: id,
            surface,
            generation: self.reveal_generation,
            _timer: handle,
        });
        tracing::info!(note = %id, %output, "reveal");
        Task::batch(tasks)
    }

    pub(super) fn extend_reveal(&mut self) -> Task<Message> {
        let Some(reveal) = &mut self.reveal else {
            return Task::none();
        };
        self.reveal_generation += 1;
        let (task, handle) = timer(
            REVEAL_TIMEOUT,
            Message::RevealExpired(self.reveal_generation),
        );
        reveal.generation = self.reveal_generation;
        reveal._timer = handle;
        task
    }

    pub(super) fn reveal_expired(&mut self, generation: u64) -> Task<Message> {
        if self
            .reveal
            .as_ref()
            .is_some_and(|r| r.generation == generation)
        {
            return self.end_reveal();
        }
        Task::none()
    }

    pub(super) fn end_reveal(&mut self) -> Task<Message> {
        let Some(reveal) = self.reveal.take() else {
            return Task::none();
        };
        tracing::info!(note = %reveal.note, "reveal ended");
        let Some(desk) = self.desk_for_surface(reveal.surface) else {
            return Task::none();
        };
        desk.lower(self.rects_on(&desk.geom.name))
    }

    pub(super) fn revealed_on(&self, surface: SurfaceId) -> Option<Uuid> {
        self.reveal
            .as_ref()
            .filter(|r| r.surface == surface)
            .map(|r| r.note)
    }
}
