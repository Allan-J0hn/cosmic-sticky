// SPDX-License-Identifier: GPL-3.0-only

mod desk;
mod drag;
mod note_view;

use std::collections::HashMap;
use std::time::Duration;

use cosmic::app::{Core, Task};
use cosmic::cctk::sctk::reexports::client::protocol::wl_output::WlOutput;
use cosmic::iced::core::window::Id as SurfaceId;
use cosmic::iced::event::wayland::{Event as WaylandEvent, OutputEvent};
use cosmic::iced::event::{self, Event, PlatformSpecific};
use cosmic::iced::widget::{pin, stack, text_editor};
use cosmic::iced::{Color, Length, Point, Subscription, keyboard, mouse, window};
use cosmic::{Element, executor, widget};
use uuid::Uuid;

use cosmic_sticky::model::geometry::{self, OutputGeom, Rect};
use cosmic_sticky::model::{self as model, Categories, Content, Note};
use cosmic_sticky::storage::Store;
use desk::Desk;
use drag::{Drag, DragKind};
use note_view::{Field, NoteColors, NoteParts};

const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);

struct NoteState {
    note: Note,
    title: text_editor::Content,
    body: text_editor::Content,
    edits: u64,
    saved_edits: u64,
}

impl NoteState {
    fn new(note: Note) -> Self {
        let body = match &note.content {
            Content::Text(text) => text_editor::Content::with_text(text),
            Content::Checklist(_) => text_editor::Content::new(),
        };
        Self {
            title: text_editor::Content::with_text(&note.title),
            body,
            note,
            edits: 0,
            saved_edits: 0,
        }
    }

    fn dirty(&self) -> bool {
        self.edits != self.saved_edits
    }
}

pub struct App {
    core: Core,
    store: Option<Store>,
    categories: Categories,
    /// Bottom to top.
    notes: Vec<NoteState>,
    desks: HashMap<WlOutput, Desk>,
    cursor: HashMap<SurfaceId, Point>,
    drag: Option<(Drag, Rect)>,
    hovered: Option<Uuid>,
    /// The editor last clicked, re-focused after a raise reorders the widget tree.
    editing: Option<(Uuid, Field)>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Output(Box<OutputEvent>, WlOutput),
    CursorMoved(SurfaceId, Point),
    PointerPressed(SurfaceId),
    PointerReleased,
    CursorLeft(SurfaceId),
    Escape,
    Focused(SurfaceId, bool),
    StartDrag(Uuid, DragKind),
    Title(Uuid, text_editor::Action),
    Body(Uuid, text_editor::Action),
    FocusBody(Uuid),
    SaveDue(Uuid, u64),
}

impl App {
    fn outputs(&self) -> Vec<OutputGeom> {
        self.desks.values().map(|d| d.geom.clone()).collect()
    }

    fn desk_for_surface(&self, surface: SurfaceId) -> Option<&Desk> {
        self.desks.values().find(|d| d.surface == surface)
    }

    fn note_mut(&mut self, id: Uuid) -> Option<&mut NoteState> {
        self.notes.iter_mut().find(|n| n.note.id == id)
    }

    /// The output a note is shown on and its rectangle there.
    fn placement(&self, note: &Note, outputs: &[OutputGeom]) -> Option<(String, Rect)> {
        if let Some((drag, rect)) = &self.drag
            && drag.note == note.id
        {
            let desk = self.desk_for_surface(drag.surface)?;
            return Some((desk.geom.name.clone(), *rect));
        }
        let out = geometry::place(&note.output, note.output_desc.as_deref(), outputs)?;
        Some((out.name.clone(), note.rect.clamp_into(out.size)))
    }

    fn rects_on(&self, output: &str) -> Vec<Rect> {
        let outputs = self.outputs();
        self.notes
            .iter()
            .filter_map(|n| self.placement(&n.note, &outputs))
            .filter(|(name, _)| name == output)
            .map(|(_, rect)| rect)
            .collect()
    }

    fn update_input_regions(&self) -> Task<Message> {
        Task::batch(
            self.desks
                .values()
                .map(|d| d.set_input_region(self.rects_on(&d.geom.name))),
        )
    }

    fn on_output(&mut self, event: OutputEvent, output: WlOutput) -> Task<Message> {
        match event {
            OutputEvent::Created(Some(info)) => {
                let geom = output_geom(&info);
                tracing::info!(output = %geom.name, size = ?geom.size, pos = ?geom.position, "output added");
                let desk = Desk::new(geom);
                let old = self.desks.insert(output.clone(), desk);
                let desk = &self.desks[&output];
                let create = desk.create(&output, self.rects_on(&desk.geom.name));
                let mut tasks = vec![create, self.update_input_regions()];
                if let Some(old) = old {
                    tasks.push(old.destroy());
                }
                Task::batch(tasks)
            }
            OutputEvent::Created(None) => {
                tracing::warn!("output added without info; ignoring");
                Task::none()
            }
            OutputEvent::InfoUpdate(info) => {
                let Some(desk) = self.desks.get_mut(&output) else {
                    return Task::none();
                };
                let fresh = output_geom(&info);
                desk.geom = OutputGeom {
                    name: if fresh.name.is_empty() {
                        desk.geom.name.clone()
                    } else {
                        fresh.name
                    },
                    description: fresh.description.or(desk.geom.description.take()),
                    position: fresh.position,
                    size: if fresh.size == (0.0, 0.0) {
                        desk.geom.size
                    } else {
                        fresh.size
                    },
                };
                self.update_input_regions()
            }
            OutputEvent::Removed => match self.desks.remove(&output) {
                Some(desk) => {
                    tracing::info!(output = %desk.geom.name, "output removed");
                    if self.drag.is_some_and(|(d, _)| d.surface == desk.surface) {
                        self.drag = None;
                    }
                    Task::batch([desk.destroy(), self.update_input_regions()])
                }
                None => Task::none(),
            },
        }
    }

    fn edited(&mut self, id: Uuid) -> Task<Message> {
        let Some(n) = self.note_mut(id) else {
            return Task::none();
        };
        n.note.modified = model::now();
        n.edits += 1;
        let edits = n.edits;
        cosmic::task::future(async move {
            tokio::time::sleep(AUTOSAVE_DELAY).await;
            Message::SaveDue(id, edits)
        })
    }

    fn save(&mut self, id: Uuid) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let Some(n) = self.note_mut(id) else {
            return;
        };
        match store.save_note(&n.note) {
            Ok(()) => {
                n.saved_edits = n.edits;
                tracing::debug!(note = %id, "saved");
            }
            Err(e) => tracing::error!("saving note {id}: {e}"),
        }
    }

    fn save_dirty(&mut self) {
        let dirty: Vec<Uuid> = self
            .notes
            .iter()
            .filter(|n| n.dirty())
            .map(|n| n.note.id)
            .collect();
        for id in dirty {
            self.save(id);
        }
    }

    /// Index of the topmost note under the cursor on `surface`.
    fn note_at(&self, surface: SurfaceId) -> Option<usize> {
        let desk = self.desk_for_surface(surface)?;
        let cursor = self.cursor.get(&surface)?;
        let outputs = self.outputs();
        self.notes.iter().rposition(|n| {
            self.placement(&n.note, &outputs)
                .is_some_and(|(o, r)| o == desk.geom.name && r.contains((cursor.x, cursor.y)))
        })
    }

    fn set_hovered(&mut self, hovered: Option<Uuid>) {
        if self.hovered != hovered {
            tracing::debug!(note = ?hovered, "hover");
            self.hovered = hovered;
        }
    }

    /// Moves the note under the cursor to the top. The widget tree keeps state by position,
    /// so focus would stay with whichever note takes the old slot; re-focus the right editor.
    fn raise_at(&mut self, surface: SurfaceId) -> Task<Message> {
        let hit = self.note_at(surface);
        let note = hit.map(|i| self.notes[i].note.id);
        tracing::debug!(cursor = ?self.cursor.get(&surface), note = ?note, "press");
        match hit {
            Some(i) if i + 1 != self.notes.len() => {
                let n = self.notes.remove(i);
                self.notes.push(n);
                self.refocus()
            }
            _ => Task::none(),
        }
    }

    fn refocus(&self) -> Task<Message> {
        match self.editing {
            Some((id, field)) => {
                cosmic::iced::widget::operation::focus(note_view::editor_id(id, field))
            }
            None => Task::none(),
        }
    }

    fn start_drag(&mut self, id: Uuid, kind: DragKind) {
        let outputs = self.outputs();
        let Some(n) = self.notes.iter().find(|n| n.note.id == id) else {
            return;
        };
        let Some((output, rect)) = self.placement(&n.note, &outputs) else {
            return;
        };
        let Some(desk) = self.desks.values().find(|d| d.geom.name == output) else {
            return;
        };
        let cursor = self.cursor.get(&desk.surface).copied().unwrap_or_default();
        let drag = Drag::new(id, desk.surface, kind, (cursor.x, cursor.y), rect);
        self.drag = Some((drag, rect));
    }

    fn finish_drag(&mut self) -> Task<Message> {
        let Some((drag, rect)) = self.drag.take() else {
            return Task::none();
        };
        let Some(from) = self.desk_for_surface(drag.surface).map(|d| d.geom.clone()) else {
            return Task::none();
        };
        let outputs = self.outputs();
        let local = self.cursor.get(&drag.surface).copied().unwrap_or_default();
        let global = (from.position.0 + local.x, from.position.1 + local.y);
        let (to, rect) = geometry::hand_off(rect, &from, global, &outputs);
        let (to_name, to_desc) = (to.name.clone(), to.description.clone());
        if let Some(n) = self.note_mut(drag.note) {
            let unchanged = n.note.rect == rect && n.note.output == to_name;
            if !unchanged {
                tracing::debug!(note = %drag.note, output = %to_name, ?rect, "note moved");
                n.note.rect = rect;
                n.note.output = to_name;
                n.note.output_desc = to_desc;
                n.note.modified = model::now();
                n.edits += 1;
                self.save(drag.note);
            }
        }
        self.update_input_regions()
    }
}

fn output_geom(info: &cosmic::cctk::sctk::output::OutputInfo) -> OutputGeom {
    let description = match (info.make.trim(), info.model.trim()) {
        ("", "") => info.description.clone(),
        (make, model) => Some(format!("{make} {model}").trim().to_owned()),
    };
    OutputGeom {
        name: info.name.clone().unwrap_or_default(),
        description,
        position: info
            .logical_position
            .map_or((0.0, 0.0), |(x, y)| (x as f32, y as f32)),
        size: info
            .logical_size
            .map_or((0.0, 0.0), |(w, h)| (w as f32, h as f32)),
    }
}

fn single_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

impl cosmic::Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "io.github.Allan_J0hn.CosmicSticky";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, _flags: ()) -> (Self, Task<Message>) {
        // With frosted themes, libcosmic asks the compositor to blur everything behind our
        // full-output transparent surfaces, i.e. the whole desktop.
        core.set_auto_blur(Default::default());

        let store = Store::default_root()
            .ok_or_else(|| std::io::Error::other("neither XDG_DATA_HOME nor HOME is set"))
            .and_then(Store::open)
            .inspect_err(|e| tracing::error!("cannot open note storage: {e}"))
            .ok();
        let mut categories = Categories::default();
        let mut notes = Vec::new();
        if let Some(store) = &store {
            tracing::info!(root = %store.root().display(), "storage");
            match store.load_categories() {
                Ok(c) => categories = c,
                Err(e) => tracing::error!("loading categories: {e}"),
            }
            match store.load_notes() {
                Ok(loaded) => notes = loaded.items,
                Err(e) => tracing::error!("loading notes: {e}"),
            }
        }
        notes.sort_by_key(|n| n.modified);
        tracing::info!(count = notes.len(), "notes loaded");

        let app = App {
            core,
            store,
            categories,
            notes: notes.into_iter().map(NoteState::new).collect(),
            desks: HashMap::new(),
            cursor: HashMap::new(),
            drag: None,
            hovered: None,
            editing: None,
        };
        (app, Task::none())
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        let theme = self.core.system_theme().cosmic();
        Some(cosmic::iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.on_bg_color().into(),
            icon_color: theme.on_bg_color().into(),
        })
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, _status, surface| match event {
            Event::PlatformSpecific(PlatformSpecific::Wayland(WaylandEvent::Output(e, o))) => {
                Some(Message::Output(Box::new(e), o))
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(surface, position))
            }
            Event::Mouse(mouse::Event::CursorLeft) => Some(Message::CursorLeft(surface)),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                Some(Message::PointerPressed(surface))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Message::PointerReleased)
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => Some(Message::Escape),
            Event::Window(window::Event::Focused) => Some(Message::Focused(surface, true)),
            Event::Window(window::Event::Unfocused) => Some(Message::Focused(surface, false)),
            _ => None,
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Output(event, output) => self.on_output(*event, output),
            Message::CursorMoved(surface, position) => {
                self.cursor.insert(surface, position);
                if let Some((drag, rect)) = &mut self.drag {
                    if drag.surface == surface {
                        *rect = drag.rect_at((position.x, position.y));
                    }
                } else {
                    let hovered = self.note_at(surface).map(|i| self.notes[i].note.id);
                    self.set_hovered(hovered);
                }
                Task::none()
            }
            Message::CursorLeft(surface) => {
                self.cursor.remove(&surface);
                if self.drag.is_none() {
                    self.set_hovered(None);
                }
                Task::none()
            }
            Message::PointerPressed(surface) => self.raise_at(surface),
            Message::PointerReleased => self.finish_drag(),
            Message::Escape => {
                self.editing = None;
                self.save_dirty();
                cosmic::iced::runtime::task::effect(cosmic::iced::runtime::Action::widget(
                    cosmic::iced::core::widget::operation::focusable::unfocus(),
                ))
            }
            Message::Focused(surface, focused) => {
                if let Some(desk) = self.desk_for_surface(surface) {
                    tracing::debug!(output = %desk.geom.name, focused, "keyboard focus");
                }
                if !focused {
                    self.save_dirty();
                }
                Task::none()
            }
            Message::StartDrag(id, kind) => {
                self.start_drag(id, kind);
                Task::none()
            }
            Message::Title(id, action) => {
                let Some(n) = self.note_mut(id) else {
                    return Task::none();
                };
                let is_edit = action.is_edit();
                let focus = matches!(action, text_editor::Action::Click(_));
                n.title.perform(action);
                if focus {
                    self.editing = Some((id, Field::Title));
                    return self.refocus();
                }
                if !is_edit {
                    return Task::none();
                }
                let text = n.title.text();
                if text.contains(['\r', '\n']) {
                    n.title = text_editor::Content::with_text(&single_line(&text));
                }
                n.note.title = single_line(&text);
                self.edited(id)
            }
            Message::Body(id, action) => {
                let Some(n) = self.note_mut(id) else {
                    return Task::none();
                };
                let is_edit = action.is_edit();
                let focus = matches!(action, text_editor::Action::Click(_));
                n.body.perform(action);
                if focus {
                    self.editing = Some((id, Field::Body));
                    return self.refocus();
                }
                if !is_edit {
                    return Task::none();
                }
                n.note.content = Content::Text(n.body.text());
                self.edited(id)
            }
            Message::FocusBody(id) => {
                self.editing = Some((id, Field::Body));
                self.refocus()
            }
            Message::SaveDue(id, edits) => {
                if self
                    .notes
                    .iter()
                    .any(|n| n.note.id == id && n.edits == edits)
                {
                    self.save(id);
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        widget::space().into()
    }

    fn view_window(&self, surface: SurfaceId) -> Element<'_, Message> {
        let Some(desk) = self.desk_for_surface(surface) else {
            return widget::space().into();
        };
        let outputs = self.outputs();
        let layers = self.notes.iter().filter_map(|n| {
            let (output, rect) = self.placement(&n.note, &outputs)?;
            if output != desk.geom.name {
                return None;
            }
            let card = note_view::view(NoteParts {
                note: &n.note,
                title: &n.title,
                body: &n.body,
                hovered: self.hovered == Some(n.note.id),
                rect,
                colors: NoteColors::new(n.note.color(&self.categories)),
            });
            Some(pin(card).x(rect.x).y(rect.y).into())
        });
        stack(layers.collect::<Vec<Element<'_, Message>>>())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
