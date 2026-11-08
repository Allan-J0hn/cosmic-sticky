// SPDX-License-Identifier: GPL-3.0-only

use cosmic::app::Task;
use cosmic::iced::widget::text_editor::{self, Action, Motion};
use uuid::Uuid;

use super::note_view::{Field, Panel};
use super::{App, NoteState, single_line};
use cosmic_sticky::model::geometry::{self, DEFAULT_NOTE_SIZE};
use cosmic_sticky::model::{
    self as model, CategoryId, ChecklistItem, Content, Font, Mode, Note, Rgb, TextSize,
};

use super::Message;

impl NoteState {
    /// Rebuilds the editor contents from the note, e.g. after a mode change.
    pub(super) fn sync_editors(&mut self) {
        match &self.note.content {
            Content::Text(text) => {
                self.body = text_editor::Content::with_text(text);
                self.items.clear();
            }
            Content::Checklist(items) => {
                self.body = text_editor::Content::new();
                self.items = items
                    .iter()
                    .map(|i| text_editor::Content::with_text(&i.text))
                    .collect();
            }
        }
    }
}

impl App {
    pub(super) fn on_edit(&mut self, id: Uuid, field: Field, action: Action) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        let is_edit = action.is_edit();
        let click = matches!(action, Action::Click(_));
        match field {
            Field::Title => {
                n.title.perform(action);
                if is_edit {
                    n.note.title = flatten(&mut n.title);
                }
            }
            Field::Body => {
                n.body.perform(action);
                if is_edit {
                    n.note.content = Content::Text(n.body.text());
                }
            }
            Field::Item(i) => {
                let Some(content) = n.items.get_mut(i) else {
                    return Task::none();
                };
                content.perform(action);
                if is_edit {
                    let text = flatten(content);
                    if let Some(item) = n.note.items_mut().and_then(|items| items.get_mut(i)) {
                        item.text = text;
                    }
                }
            }
        }
        let mut tasks = Vec::new();
        if click {
            self.editing = Some((id, field));
            tasks.push(self.refocus());
        }
        if is_edit {
            tasks.push(self.edited(id));
        }
        Task::batch(tasks)
    }

    pub(super) fn insert_item(&mut self, id: Uuid, index: usize) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        let Some(items) = n.note.items_mut() else {
            return Task::none();
        };
        let index = index.min(items.len());
        items.insert(index, ChecklistItem::default());
        n.items.insert(index, text_editor::Content::new());
        self.editing = Some((id, Field::Item(index)));
        Task::batch([self.refocus(), self.edited(id)])
    }

    pub(super) fn remove_item(&mut self, id: Uuid, index: usize) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        let Some(items) = n.note.items_mut() else {
            return Task::none();
        };
        if index >= items.len() {
            return Task::none();
        }
        items.remove(index);
        n.items.remove(index);
        // Later items move up a slot in the widget tree, and their focus state with them.
        self.editing = match self.editing {
            Some((note, Field::Item(i))) if note == id && i == index => {
                let previous = index.checked_sub(1);
                if let (Some(p), Some(n)) = (previous, self.note_mut(id)) {
                    n.items[p].perform(Action::Move(Motion::DocumentEnd));
                }
                previous.map(|p| (id, Field::Item(p)))
            }
            Some((note, Field::Item(i))) if note == id && i > index => {
                Some((id, Field::Item(i - 1)))
            }
            other => other,
        };
        Task::batch([self.refocus(), self.edited(id)])
    }

    pub(super) fn toggle_item(&mut self, id: Uuid, index: usize) -> Task<Message> {
        let Some(item) = self
            .note_mut(id)
            .and_then(|n| n.note.items_mut())
            .and_then(|items| items.get_mut(index))
        else {
            return Task::none();
        };
        item.checked = !item.checked;
        tracing::debug!(note = %id, index, checked = item.checked, "item toggled");
        self.edited(id)
    }

    pub(super) fn toggle_mode(&mut self, id: Uuid) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        let mode = match n.note.mode() {
            Mode::Text => Mode::Checklist,
            Mode::Checklist => Mode::Text,
        };
        n.note.set_mode(mode);
        n.sync_editors();
        tracing::debug!(note = %id, ?mode, "mode set");
        Task::batch([self.unfocus_editors(), self.edited(id)])
    }

    pub(super) fn open_panel(&mut self, id: Uuid, panel: Panel) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        n.panel = if n.panel == Some(panel) {
            None
        } else {
            Some(panel)
        };
        tracing::debug!(note = %id, panel = ?n.panel, "panel");
        if self.editing.is_some_and(|(note, _)| note == id) {
            return self.unfocus_editors();
        }
        Task::none()
    }

    pub(super) fn close_panel(&mut self, id: Uuid) -> Task<Message> {
        if let Some(n) = self.note_mut(id) {
            n.panel = None;
        }
        Task::none()
    }

    pub(super) fn close_all_panels(&mut self) {
        for n in &mut self.notes {
            n.panel = None;
        }
    }

    pub(super) fn choose_category(&mut self, id: Uuid, category: CategoryId) -> Task<Message> {
        let Some(category) = self.categories.get(&category).cloned() else {
            return Task::none();
        };
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        let creating = matches!(n.panel, Some(Panel::Category { creating: true }));
        if creating {
            n.note.choose_initial_category(&category);
            n.sync_editors();
        } else {
            n.note.category = category.id.clone();
        }
        n.panel = None;
        tracing::debug!(note = %id, category = %category.id, creating, mode = ?n.note.mode(), "category set");
        let mut tasks = vec![self.edited(id)];
        if creating {
            self.editing = Some((id, Field::Title));
            tasks.push(self.refocus());
        }
        Task::batch(tasks)
    }

    pub(super) fn set_color(&mut self, id: Uuid, color: Option<Rgb>) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        n.note.color_override = color;
        n.panel = None;
        tracing::debug!(note = %id, color = ?color.map(|c| c.to_string()), "color set");
        self.edited(id)
    }

    pub(super) fn set_font(&mut self, id: Uuid, font: Font) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        n.note.font = font;
        n.panel = None;
        tracing::debug!(note = %id, ?font, "font set");
        self.edited(id)
    }

    pub(super) fn set_text_size(&mut self, id: Uuid, size: TextSize) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        n.note.text_size = size;
        n.panel = None;
        tracing::debug!(note = %id, size = size.percent(), "text size set");
        self.edited(id)
    }

    /// A new note next to `parent`, with the category picker open.
    pub(super) fn new_note_beside(&mut self, parent: Uuid) -> Task<Message> {
        let outputs = self.outputs();
        let Some(p) = self.notes.iter().find(|n| n.note.id == parent) else {
            return Task::none();
        };
        let Some((output, parent_rect)) = self.placement(&p.note, &outputs) else {
            return Task::none();
        };
        let Some(out) = outputs.iter().find(|o| o.name == output) else {
            return Task::none();
        };
        let rect = geometry::beside(parent_rect, DEFAULT_NOTE_SIZE, out.size);
        let category = self.categories.resolve(&p.note.category).clone();
        let mut note = Note::new(&category, rect, out.name.clone(), model::now());
        note.output_desc = out.description.clone();
        let id = note.id;
        tracing::info!(note = %id, parent = %parent, output = %out.name, ?rect, "note created");
        let mut state = NoteState::new(note);
        state.panel = Some(Panel::Category { creating: true });
        state.edits = 1;
        self.notes.push(state);
        self.save(id);
        self.update_input_regions()
    }
}

/// Keeps a single-line editor single-line (pasted text may contain line breaks).
fn flatten(content: &mut text_editor::Content) -> String {
    let text = content.text();
    if text.contains(['\r', '\n']) {
        let flat = single_line(&text);
        *content = text_editor::Content::with_text(&flat);
        content.perform(Action::Move(Motion::DocumentEnd));
        return flat;
    }
    text
}
