// SPDX-License-Identifier: GPL-3.0-only

//! The list view: a normal window with notes, trash and categories.

use std::collections::HashMap;

use cosmic::Element;
use cosmic::app::Task;
use cosmic::iced::core::window::Id as SurfaceId;
use cosmic::iced::widget::{column, container, row};
use cosmic::iced::{Alignment, Border, Length, Size, window};
use cosmic::widget::segmented_button::{Entity, SingleSelectModel};
use cosmic::widget::{self, button, flex_row, icon, scrollable, segmented_control, text};
use uuid::Uuid;

use super::note_view::color;
use super::{App, Message};
use crate::fl;
use cosmic_sticky::model::{self as model, CategoryId, Note, PALETTE, Rgb, search};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Notes,
    Trash,
    Categories,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CategoryEdit {
    Recolor,
    Delete { target: Option<CategoryId> },
}

pub struct ListWindow {
    pub id: SurfaceId,
    tabs: SingleSelectModel,
    query: String,
    filter: Option<CategoryId>,
    confirm_empty: bool,
    new_category: String,
    names: HashMap<CategoryId, String>,
    editing: Option<(CategoryId, CategoryEdit)>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ListMessage {
    Tab(Entity),
    Query(String),
    Filter(Option<CategoryId>),
    NewNote,
    Reveal(Uuid),
    Restore(Uuid),
    ConfirmEmpty(bool),
    EmptyTrash,
    NewCategoryName(String),
    AddCategory,
    Rename(CategoryId, String),
    CommitRename(CategoryId),
    Edit(CategoryId, Option<CategoryEdit>),
    Recolor(CategoryId, Rgb),
    DeleteTarget(CategoryId, CategoryId),
    ConfirmDelete(CategoryId),
    Close,
    Drag,
}

fn search_id() -> widget::Id {
    widget::Id::new("list-search")
}

fn msg(m: ListMessage) -> Message {
    Message::List(m)
}

impl App {
    pub(super) fn open_list(&mut self) -> Task<Message> {
        if let Some(list) = &self.list {
            return window::gain_focus(list.id);
        }
        let (id, open) = window::open(window::Settings {
            size: Size::new(460.0, 620.0),
            min_size: Some(Size::new(360.0, 420.0)),
            decorations: false,
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: <App as cosmic::Application>::APP_ID.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let tabs = SingleSelectModel::builder()
            .insert(|b| b.text(fl!("tab-notes")).data(Tab::Notes).activate())
            .insert(|b| b.text(fl!("tab-trash")).data(Tab::Trash))
            .insert(|b| b.text(fl!("tab-categories")).data(Tab::Categories))
            .build();
        self.list = Some(ListWindow {
            id,
            tabs,
            query: String::new(),
            filter: None,
            confirm_empty: false,
            new_category: String::new(),
            names: HashMap::new(),
            editing: None,
            error: None,
        });
        tracing::info!("list window opened");
        let title = cosmic::ApplicationExt::set_window_title(self, fl!("app-title"), id);
        Task::batch([open.discard(), title])
    }

    pub(super) fn list_opened(&self) -> Task<Message> {
        widget::text_input::focus(search_id())
    }

    fn filtered_notes(&self, list: &ListWindow) -> Vec<&Note> {
        let mut notes: Vec<&Note> = self
            .notes
            .iter()
            .map(|n| &n.note)
            .filter(|n| search::matches(n, &list.query, list.filter.as_ref()))
            .collect();
        notes.sort_by(|a, b| b.modified.cmp(&a.modified));
        notes
    }

    pub(super) fn on_list(&mut self, message: ListMessage) -> Task<Message> {
        tracing::debug!(?message, "list");
        let Some(list) = &mut self.list else {
            return Task::none();
        };
        let id = list.id;
        match message {
            ListMessage::Tab(entity) => {
                list.tabs.activate(entity);
                list.confirm_empty = false;
                list.editing = None;
                list.error = None;
            }
            ListMessage::Query(query) => {
                list.query = query;
                self.log_filter();
            }
            ListMessage::Filter(filter) => {
                list.filter = filter;
                self.log_filter();
            }
            ListMessage::NewNote => return self.new_note(None),
            ListMessage::Reveal(note) => return self.reveal(note),
            ListMessage::Restore(note) => return self.restore(note),
            ListMessage::ConfirmEmpty(confirm) => list.confirm_empty = confirm,
            ListMessage::EmptyTrash => {
                list.confirm_empty = false;
                return self.empty_trash();
            }
            ListMessage::NewCategoryName(name) => list.new_category = name,
            ListMessage::AddCategory => return self.add_category(),
            ListMessage::Rename(category, name) => {
                list.names.insert(category, name);
            }
            ListMessage::CommitRename(category) => return self.commit_rename(category),
            ListMessage::Edit(category, edit) => {
                let same = list.editing.as_ref().map(|(c, e)| (c, Some(e)))
                    == Some((&category, edit.as_ref()));
                list.editing = if same {
                    None
                } else {
                    edit.map(|e| (category, e))
                };
                list.error = None;
            }
            ListMessage::Recolor(category, rgb) => {
                list.editing = None;
                if self.categories.recolor(&category, rgb).is_ok() {
                    tracing::debug!(%category, color = %rgb, "category recolored");
                    self.save_categories();
                }
            }
            ListMessage::DeleteTarget(category, target) => {
                list.editing = Some((
                    category,
                    CategoryEdit::Delete {
                        target: Some(target),
                    },
                ));
            }
            ListMessage::ConfirmDelete(category) => return self.delete_category(category),
            ListMessage::Close => return window::close(id),
            ListMessage::Drag => return window::drag(id),
        }
        Task::none()
    }

    fn log_filter(&self) {
        if let Some(list) = &self.list {
            let count = self.filtered_notes(list).len();
            tracing::debug!(query = %list.query, filter = ?list.filter, count, "list filtered");
        }
    }

    fn save_categories(&self) {
        if let Some(store) = &self.store
            && let Err(e) = store.save_categories(&self.categories)
        {
            tracing::error!("saving categories: {e}");
        }
    }

    fn set_list_error(&mut self, error: impl ToString) {
        if let Some(list) = &mut self.list {
            list.error = Some(error.to_string());
        }
    }

    fn add_category(&mut self) -> Task<Message> {
        let Some(list) = &mut self.list else {
            return Task::none();
        };
        let name = std::mem::take(&mut list.new_category);
        let used: Vec<Rgb> = self.categories.iter().map(|c| c.color).collect();
        let rgb = PALETTE
            .iter()
            .copied()
            .find(|c| !used.contains(c))
            .unwrap_or(PALETTE[0]);
        match self.categories.add(&name, rgb) {
            Ok(category) => {
                tracing::debug!(%category, name = %name.trim(), "category added");
                list.error = None;
                self.save_categories();
            }
            Err(e) => {
                list.new_category = name;
                self.set_list_error(e);
            }
        }
        Task::none()
    }

    fn commit_rename(&mut self, category: CategoryId) -> Task<Message> {
        let Some(name) = self.list.as_mut().and_then(|l| l.names.remove(&category)) else {
            return Task::none();
        };
        if self
            .categories
            .get(&category)
            .is_some_and(|c| c.name == name)
        {
            return Task::none();
        }
        match self.categories.rename(&category, &name) {
            Ok(()) => {
                tracing::debug!(%category, %name, "category renamed");
                self.save_categories();
            }
            Err(e) => self.set_list_error(e),
        }
        Task::none()
    }

    /// Deletes a category after its notes, live and trashed, move to the chosen one.
    fn delete_category(&mut self, category: CategoryId) -> Task<Message> {
        let Some((
            _,
            CategoryEdit::Delete {
                target: Some(target),
            },
        )) = self.list.as_ref().and_then(|l| l.editing.clone())
        else {
            return Task::none();
        };
        let live: Vec<Uuid> = self
            .notes
            .iter()
            .filter(|n| n.note.category == category)
            .map(|n| n.note.id)
            .collect();
        let trashed: Vec<Uuid> = self
            .trash
            .iter()
            .filter(|t| t.note.category == category)
            .map(|t| t.note.id)
            .collect();
        let notes = self
            .notes
            .iter_mut()
            .map(|n| &mut n.note)
            .chain(self.trash.iter_mut().map(|t| &mut t.note));
        let moved = match self.categories.delete(&category, &target, notes) {
            Ok(moved) => moved,
            Err(e) => {
                self.set_list_error(e);
                return Task::none();
            }
        };
        self.save_categories();
        for id in &live {
            if let Some(n) = self.note_mut(*id) {
                n.edits += 1;
            }
            self.save(*id);
        }
        if let Some(store) = &self.store {
            for t in self.trash.iter().filter(|t| trashed.contains(&t.note.id)) {
                if let Err(e) = store.save_trashed(t) {
                    tracing::error!("saving trashed note {}: {e}", t.note.id);
                }
            }
        }
        if let Some(list) = &mut self.list {
            list.editing = None;
            list.error = None;
            if list.filter.as_ref() == Some(&category) {
                list.filter = None;
            }
        }
        tracing::info!(%category, %target, moved, live = live.len(), trashed = trashed.len(), "category deleted");
        Task::none()
    }

    pub(super) fn list_view<'a>(&'a self, list: &'a ListWindow) -> Element<'a, Message> {
        let focused = self.core.focused_window() == Some(list.id);
        let header = widget::header_bar()
            .title(fl!("app-title"))
            .on_close(msg(ListMessage::Close))
            .on_drag(msg(ListMessage::Drag))
            .focused(focused);
        let tabs =
            segmented_control::horizontal(&list.tabs).on_activate(|e| msg(ListMessage::Tab(e)));
        let body = match list.tabs.active_data::<Tab>() {
            Some(Tab::Trash) => self.trash_tab(list),
            Some(Tab::Categories) => self.categories_tab(list),
            _ => self.notes_tab(list),
        };
        let mut content = column![tabs].spacing(12);
        if let Some(error) = &list.error {
            content = content.push(text::body(error.clone()).class(cosmic::theme::Text::Accent));
        }
        container(column![
            header,
            container(content.push(body)).padding([4, 16, 16, 16])
        ])
        .class(cosmic::theme::Container::WindowBackground)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn notes_tab<'a>(&'a self, list: &'a ListWindow) -> Element<'a, Message> {
        let search = widget::search_input(fl!("search-placeholder"), &list.query)
            .id(search_id())
            .on_input(|q| msg(ListMessage::Query(q)))
            .on_clear(msg(ListMessage::Query(String::new())))
            .width(Length::Fill);
        let new = button::suggested(fl!("new-note")).on_press(msg(ListMessage::NewNote));
        let mut chips = vec![chip(
            fl!("all-categories"),
            list.filter.is_none(),
            msg(ListMessage::Filter(None)),
        )];
        chips.extend(self.categories.iter().map(|c| {
            chip(
                c.name.clone(),
                list.filter.as_ref() == Some(&c.id),
                msg(ListMessage::Filter(Some(c.id.clone()))),
            )
        }));
        let notes = self.filtered_notes(list);
        let rows: Element<'_, Message> = if notes.is_empty() {
            text::body(fl!("no-notes")).into()
        } else {
            let mut items = widget::list_column();
            for note in notes {
                let category = self.categories.resolve(&note.category);
                let title = if note.title.trim().is_empty() {
                    fl!("untitled")
                } else {
                    note.title.clone()
                };
                let line = row![
                    dot(note.color(&self.categories)),
                    column![text::body(title), text::caption(search::snippet(note))]
                        .width(Length::Fill),
                    text::caption(category.name.clone()),
                ]
                .spacing(12)
                .align_y(Alignment::Center);
                items = items.add(
                    button::custom(line)
                        .class(cosmic::theme::Button::MenuItem)
                        .width(Length::Fill)
                        .padding([6, 4])
                        .on_press(msg(ListMessage::Reveal(note.id))),
                );
            }
            items.into()
        };
        column![
            row![search, new].spacing(8).align_y(Alignment::Center),
            flex_row(chips).row_spacing(4).column_spacing(4),
            scrollable(rows).height(Length::Fill),
        ]
        .spacing(12)
        .into()
    }

    fn trash_tab<'a>(&'a self, list: &'a ListWindow) -> Element<'a, Message> {
        if self.trash.is_empty() {
            return text::body(fl!("trash-empty")).into();
        }
        let now = model::now();
        let mut trash: Vec<_> = self.trash.iter().collect();
        trash.sort_by(|a, b| b.trashed_at.cmp(&a.trashed_at));
        let mut items = widget::list_column();
        for t in trash {
            let title = if t.note.title.trim().is_empty() {
                fl!("untitled")
            } else {
                t.note.title.clone()
            };
            let days = now.duration_since(t.trashed_at).as_hours().max(0) / 24;
            items = items.add(
                row![
                    dot(t.note.color(&self.categories)),
                    column![
                        text::body(title),
                        text::caption(fl!("deleted-days-ago", days = days))
                    ]
                    .width(Length::Fill),
                    button::standard(fl!("restore")).on_press(msg(ListMessage::Restore(t.note.id))),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            );
        }
        let footer: Element<'_, Message> = if list.confirm_empty {
            row![
                text::body(fl!("empty-trash-confirm", count = self.trash.len()))
                    .width(Length::Fill),
                button::standard(fl!("cancel")).on_press(msg(ListMessage::ConfirmEmpty(false))),
                button::destructive(fl!("delete-permanently"))
                    .on_press(msg(ListMessage::EmptyTrash)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else {
            row![
                widget::space().width(Length::Fill),
                button::destructive(fl!("empty-trash"))
                    .on_press(msg(ListMessage::ConfirmEmpty(true))),
            ]
            .into()
        };
        column![
            text::caption(fl!("trash-hint")),
            scrollable(items).height(Length::Fill),
            footer
        ]
        .spacing(12)
        .into()
    }

    fn categories_tab<'a>(&'a self, list: &'a ListWindow) -> Element<'a, Message> {
        let only_one = self.categories.as_slice().len() == 1;
        let mut items = widget::list_column();
        for c in self.categories.iter() {
            let count = self
                .notes
                .iter()
                .filter(|n| n.note.category == c.id)
                .count();
            let name = list.names.get(&c.id).unwrap_or(&c.name).clone();
            let id = c.id.clone();
            let mut delete = button::icon(icon::from_name("edit-delete-symbolic"));
            if !only_one {
                delete = delete.on_press(msg(ListMessage::Edit(
                    id.clone(),
                    Some(CategoryEdit::Delete { target: None }),
                )));
            }
            let mut entry = column![
                row![
                    button::custom(dot(c.color))
                        .class(cosmic::theme::Button::Text)
                        .padding(4)
                        .on_press(msg(ListMessage::Edit(
                            id.clone(),
                            Some(CategoryEdit::Recolor)
                        ))),
                    widget::text_input(fl!("category-name"), name)
                        .on_input({
                            let id = id.clone();
                            move |s| msg(ListMessage::Rename(id.clone(), s))
                        })
                        .on_submit({
                            let id = id.clone();
                            move |_| msg(ListMessage::CommitRename(id.clone()))
                        })
                        .on_unfocus(msg(ListMessage::CommitRename(id.clone())))
                        .width(Length::Fill),
                    text::caption(fl!("note-count", count = count)),
                    delete,
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            ]
            .spacing(8);
            match &list.editing {
                Some((editing, CategoryEdit::Recolor)) if *editing == c.id => {
                    entry = entry.push(flex_row(
                        PALETTE
                            .iter()
                            .map(|&p| {
                                button::custom(dot_sized(p, 22.0, p == c.color))
                                    .class(cosmic::theme::Button::Text)
                                    .padding(2)
                                    .on_press(msg(ListMessage::Recolor(id.clone(), p)))
                                    .into()
                            })
                            .collect(),
                    ));
                }
                Some((editing, CategoryEdit::Delete { target })) if *editing == c.id => {
                    let targets = self
                        .categories
                        .iter()
                        .filter(|o| o.id != c.id)
                        .map(|o| {
                            chip(
                                o.name.clone(),
                                target.as_ref() == Some(&o.id),
                                msg(ListMessage::DeleteTarget(id.clone(), o.id.clone())),
                            )
                        })
                        .collect();
                    let mut confirm = button::destructive(fl!("delete-category"));
                    if target.is_some() {
                        confirm = confirm.on_press(msg(ListMessage::ConfirmDelete(id.clone())));
                    }
                    entry = entry.push(
                        column![
                            text::body(fl!("move-notes-to", count = count)),
                            flex_row(targets).row_spacing(4).column_spacing(4),
                            row![
                                widget::space().width(Length::Fill),
                                button::standard(fl!("cancel"))
                                    .on_press(msg(ListMessage::Edit(id.clone(), None))),
                                confirm,
                            ]
                            .spacing(8),
                        ]
                        .spacing(8),
                    );
                }
                _ => {}
            }
            items = items.add(entry);
        }
        let add = row![
            widget::text_input(fl!("new-category"), &list.new_category)
                .on_input(|s| msg(ListMessage::NewCategoryName(s)))
                .on_submit(|_| msg(ListMessage::AddCategory))
                .width(Length::Fill),
            button::standard(fl!("add")).on_press(msg(ListMessage::AddCategory)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        column![scrollable(items).height(Length::Fill), add]
            .spacing(12)
            .into()
    }
}

fn chip<'a>(label: String, selected: bool, message: Message) -> Element<'a, Message> {
    let b = if selected {
        button::suggested(label)
    } else {
        button::standard(label)
    };
    b.on_press(message).into()
}

fn dot<'a>(rgb: Rgb) -> Element<'a, Message> {
    dot_sized(rgb, 14.0, false)
}

fn dot_sized<'a>(rgb: Rgb, size: f32, selected: bool) -> Element<'a, Message> {
    container(widget::space())
        .width(size)
        .height(size)
        .class(cosmic::theme::Container::custom(move |theme| {
            container::Style {
                background: Some(color(rgb).into()),
                border: Border {
                    radius: (size / 2.0).into(),
                    width: if selected { 2.5 } else { 1.0 },
                    color: if selected {
                        theme.cosmic().accent_color().into()
                    } else {
                        theme.cosmic().bg_divider().into()
                    },
                },
                ..Default::default()
            }
        }))
        .into()
}
