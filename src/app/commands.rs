// SPDX-License-Identifier: GPL-3.0-only

//! Commands from the command line (directly or via the running instance), and show/hide.

use cosmic::app::Task;

use super::desk::Desk;
use super::note_view::Panel;
use super::{App, Message, NoteState};
use cosmic_sticky::cli::Command;
use cosmic_sticky::model::geometry::{self, DEFAULT_NOTE_SIZE};
use cosmic_sticky::model::{self as model, CategoryId, Note};
use cosmic_sticky::storage::Settings;

impl App {
    pub(super) fn run_command(&mut self, command: Command) -> Task<Message> {
        tracing::info!(?command, "command");
        match command {
            Command::Toggle => self.set_visible(!self.visible),
            Command::List => self.open_list(),
            Command::New { category } => {
                let id = category.and_then(|name| match self.categories.find_by_name(&name) {
                    Some(c) => Some(c.id.clone()),
                    None => {
                        tracing::warn!(%name, "no such category; asking instead");
                        None
                    }
                });
                self.new_note(id)
            }
        }
    }

    /// Destroying the desk surfaces, rather than just emptying them, frees their buffers.
    pub(super) fn set_visible(&mut self, visible: bool) -> Task<Message> {
        if visible == self.visible {
            return Task::none();
        }
        self.visible = visible;
        tracing::info!(visible, "notes visibility");
        if let Some(store) = &self.store
            && let Err(e) = store.save_settings(&Settings {
                notes_visible: visible,
            })
        {
            tracing::error!("saving settings: {e}");
        }
        if visible {
            for desk in self.desks.values_mut() {
                *desk = Desk::new(desk.geom.clone());
            }
            let tasks: Vec<_> = self
                .desks
                .iter()
                .map(|(output, desk)| desk.create(output, self.rects_on(&desk.geom.name)))
                .collect();
            Task::batch(tasks)
        } else {
            self.save_dirty();
            self.reveal = None;
            self.drag = None;
            self.hovered = None;
            self.editing = None;
            self.cursor.clear();
            for n in &mut self.notes {
                n.panel = None;
            }
            Task::batch(self.desks.values().map(Desk::destroy))
        }
    }

    /// New note on the top-left output, shown above windows. Without a category, its category
    /// picker is open.
    pub(super) fn new_note(&mut self, category: Option<CategoryId>) -> Task<Message> {
        let mut tasks = vec![self.set_visible(true)];
        let outputs = self.outputs();
        let Some(out) = geometry::place("", None, &outputs).cloned() else {
            return Task::batch(tasks);
        };
        let existing = self.rects_on(&out.name);
        let rect = geometry::free_spot(&existing, DEFAULT_NOTE_SIZE, out.size);
        let first = self.categories.as_slice()[0].clone();
        let mut note = Note::new(&first, rect, out.name.clone(), model::now());
        note.output_desc = out.description.clone();
        let id = note.id;
        tracing::info!(note = %id, output = %out.name, ?rect, "note created");
        let mut state = NoteState::new(note);
        state.panel = Some(Panel::Category { creating: true });
        state.edits = 1;
        self.notes.push(state);
        self.save(id);
        tasks.push(self.update_input_regions());
        tasks.push(self.reveal(id));
        if let Some(category) = category {
            tasks.push(self.choose_category(id, category));
        }
        Task::batch(tasks)
    }
}
