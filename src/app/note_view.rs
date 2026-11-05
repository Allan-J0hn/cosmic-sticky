// SPDX-License-Identifier: GPL-3.0-only

use cosmic::Element;
use cosmic::iced::widget::text_editor::{self, Binding, KeyPress, Status};
use cosmic::iced::widget::{column, container, row, svg, text_editor as editor};
use cosmic::iced::{Alignment, Background, Border, Color, Length, keyboard, mouse};
use cosmic::widget::{self, icon, mouse_area, scrollable, text};
use uuid::Uuid;

use super::Message;
use super::drag::DragKind;
use super::pickers;
use crate::fl;
use crate::fonts;
use cosmic_sticky::model::{Categories, Content, Note, Rect, Rgb};

pub const HEADER_HEIGHT: f32 = 28.0;
pub const ICON_SIZE: u16 = 16;
const GRIP_SIZE: f32 = 16.0;
const TITLE_SIZE: f32 = 17.0;
const BODY_SIZE: f32 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Body,
    Item(usize),
}

pub fn editor_id(id: Uuid, field: Field) -> cosmic::widget::Id {
    cosmic::widget::Id::new(format!("{field:?}-{id}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Category { creating: bool },
    Color,
    Font,
}

#[derive(Clone, Copy)]
pub struct NoteColors {
    pub background: Color,
    pub text: Color,
}

impl NoteColors {
    pub fn new(background: Rgb) -> Self {
        Self {
            background: color(background),
            text: color(background.readable_text()),
        }
    }

    pub fn faded(&self, alpha: f32) -> Color {
        Color {
            a: alpha,
            ..self.text
        }
    }
}

pub fn color(c: Rgb) -> Color {
    Color::from_rgb8(c.r, c.g, c.b)
}

pub struct NoteParts<'a> {
    pub note: &'a Note,
    pub categories: &'a Categories,
    pub title: &'a text_editor::Content,
    pub body: &'a text_editor::Content,
    pub items: &'a [text_editor::Content],
    pub panel: Option<Panel>,
    pub hovered: bool,
    pub rect: Rect,
}

pub fn view(parts: NoteParts<'_>) -> Element<'_, Message> {
    let NoteParts {
        note,
        categories,
        panel,
        hovered,
        rect,
        ..
    } = parts;
    let id = note.id;
    let colors = NoteColors::new(note.color(categories));
    let fg = colors.text;

    let main: Element<'_, Message> = match panel {
        Some(panel) => pickers::view(note, categories, panel, colors),
        None => column![title_view(&parts, colors), content_view(&parts, colors)].into(),
    };

    let grip: Element<'_, Message> = if hovered {
        mouse_area(
            container(text("◢").size(GRIP_SIZE - 4.0))
                .center(Length::Fixed(GRIP_SIZE))
                .style(move |_| container::Style {
                    text_color: Some(colors.faded(0.45)),
                    ..Default::default()
                }),
        )
        .on_press(Message::StartDrag(id, DragKind::Resize))
        .interaction(mouse::Interaction::ResizingDiagonallyDown)
        .into()
    } else {
        widget::space().width(GRIP_SIZE).height(GRIP_SIZE).into()
    };

    let background = colors.background;
    container(column![
        header(note, categories, hovered || panel.is_some(), colors),
        container(main).height(Length::Fill),
        row![widget::space().width(Length::Fill), grip]
    ])
    .width(Length::Fixed(rect.width))
    .height(Length::Fixed(rect.height))
    .clip(true)
    .class(cosmic::theme::Container::custom(move |theme| {
        container::Style {
            background: Some(background.into()),
            text_color: Some(fg),
            border: Border {
                radius: theme.cosmic().corner_radii.radius_s.into(),
                width: 1.0,
                color: Color { a: 0.15, ..fg },
            },
            ..Default::default()
        }
    }))
    .into()
}

fn header<'a>(
    note: &'a Note,
    categories: &'a Categories,
    show_controls: bool,
    colors: NoteColors,
) -> Element<'a, Message> {
    let id = note.id;
    let controls: Element<'a, Message> = if show_controls {
        let category_dot = container(widget::space())
            .width(12)
            .height(12)
            .style(move |_| container::Style {
                background: Some(color(categories.resolve(&note.category).color).into()),
                border: Border {
                    radius: 6.0.into(),
                    width: 1.5,
                    color: colors.faded(0.6),
                },
                ..Default::default()
            });
        let mode_icon = match note.content {
            Content::Text(_) => "view-list-bullet-symbolic",
            Content::Checklist(_) => "format-justify-left-symbolic",
        };
        row![
            tap(symbolic("list-add-symbolic", colors), Message::NewNote(id)),
            widget::space().width(Length::Fill),
            tap(
                category_dot,
                Message::OpenPanel(id, Panel::Category { creating: false })
            ),
            tap(
                symbolic("color-select-symbolic", colors),
                Message::OpenPanel(id, Panel::Color)
            ),
            tap(
                container(text("Aa").size(12).font(cosmic::font::semibold()))
                    .center(Length::Fixed(f32::from(ICON_SIZE))),
                Message::OpenPanel(id, Panel::Font)
            ),
            tap(symbolic(mode_icon, colors), Message::ToggleMode(id)),
        ]
        .spacing(2)
        .padding([0, 4])
        .align_y(Alignment::Center)
        .height(Length::Fill)
        .into()
    } else {
        widget::space().into()
    };
    let fill = if show_controls {
        colors.faded(0.07)
    } else {
        Color::TRANSPARENT
    };
    mouse_area(
        container(controls)
            .width(Length::Fill)
            .height(Length::Fixed(HEADER_HEIGHT))
            .style(move |_| container::Style {
                background: Some(fill.into()),
                ..Default::default()
            }),
    )
    .on_press(Message::StartDrag(id, DragKind::Move))
    .interaction(mouse::Interaction::Grab)
    .into()
}

fn editor_style(colors: NoteColors) -> impl Fn(&cosmic::Theme, Status) -> text_editor::Style {
    move |_, _| text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: colors.faded(0.5),
        value: colors.text,
        selection: colors.faded(0.25),
    }
}

fn title_view<'a>(parts: &NoteParts<'a>, colors: NoteColors) -> Element<'a, Message> {
    let id = parts.note.id;
    let font = parts.note.font;
    editor(parts.title)
        .id(editor_id(id, Field::Title))
        .placeholder(fl!("title-placeholder"))
        .on_action(move |a| Message::Edit(id, Field::Title, a))
        .key_binding(move |key| {
            if is_enter(&key) && is_focused(&key) {
                return Some(Binding::Custom(Message::TitleEnter(id)));
            }
            Binding::from_key_press(key)
        })
        .size(TITLE_SIZE * fonts::scale(font))
        .font(fonts::title(font))
        .padding([2, 12])
        .style(editor_style(colors))
        .into()
}

fn content_view<'a>(parts: &NoteParts<'a>, colors: NoteColors) -> Element<'a, Message> {
    let id = parts.note.id;
    let font = parts.note.font;
    let size = BODY_SIZE * fonts::scale(font);
    match &parts.note.content {
        Content::Text(_) => editor(parts.body)
            .id(editor_id(id, Field::Body))
            .placeholder(fl!("body-placeholder"))
            .on_action(move |a| Message::Edit(id, Field::Body, a))
            .size(size)
            .font(fonts::regular(font))
            .padding([4, 12])
            .height(Length::Fill)
            .style(editor_style(colors))
            .into(),
        Content::Checklist(items) => {
            let rows = items
                .iter()
                .zip(parts.items)
                .enumerate()
                .map(|(i, (item, content))| {
                    item_row(id, i, item.checked, content, parts.hovered, font, colors)
                });
            let add = tap(
                row![
                    symbolic("list-add-symbolic", colors),
                    text(fl!("add-item")).size(size).font(fonts::regular(font))
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                Message::AddItem(id),
            );
            scrollable(column(rows).push(add).spacing(2).padding([4, 8, 4, 10]))
                .height(Length::Fill)
                .into()
        }
    }
}

fn item_row<'a>(
    id: Uuid,
    index: usize,
    checked: bool,
    content: &'a text_editor::Content,
    hovered: bool,
    font: cosmic_sticky::model::Font,
    colors: NoteColors,
) -> Element<'a, Message> {
    let empty = content.text().is_empty();
    let check = if checked {
        "checkbox-checked-symbolic"
    } else {
        "checkbox-symbolic"
    };
    let text_colors = if checked {
        NoteColors {
            text: colors.faded(0.55),
            ..colors
        }
    } else {
        colors
    };
    let field = Field::Item(index);
    let editor = editor(content)
        .id(editor_id(id, field))
        .placeholder(fl!("item-placeholder"))
        .on_action(move |a| Message::Edit(id, field, a))
        .key_binding(move |key| {
            if is_focused(&key) {
                if is_enter(&key) {
                    return Some(Binding::Custom(Message::ItemEnter(id, index)));
                }
                if empty && is_key(&key, keyboard::key::Named::Backspace) {
                    return Some(Binding::Custom(Message::RemoveItem(id, index)));
                }
            }
            Binding::from_key_press(key)
        })
        .size(BODY_SIZE * fonts::scale(font))
        .font(fonts::regular(font))
        .padding(0)
        .style(editor_style(text_colors));
    let remove: Element<'a, Message> = if hovered {
        tap(
            symbolic("window-close-symbolic", colors.dim(0.5)),
            Message::RemoveItem(id, index),
        )
    } else {
        widget::space().width(ICON_SIZE).into()
    };
    row![
        tap(symbolic(check, colors), Message::ToggleItem(id, index)),
        editor,
        remove
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

impl NoteColors {
    fn dim(self, alpha: f32) -> Self {
        Self {
            text: self.faded(alpha),
            ..self
        }
    }
}

pub fn symbolic<'a>(name: &'static str, colors: NoteColors) -> Element<'a, Message> {
    let fg = colors.text;
    icon::from_name(name)
        .size(ICON_SIZE)
        .icon()
        .class(cosmic::theme::Svg::custom(move |_| svg::Style {
            color: Some(fg),
        }))
        .into()
}

/// A clickable area; plain mouse areas keep the note's own contrast colors.
pub fn tap<'a>(content: impl Into<Element<'a, Message>>, message: Message) -> Element<'a, Message> {
    mouse_area(container(content).padding(3))
        .on_press(message)
        .interaction(mouse::Interaction::Pointer)
        .into()
}

fn is_focused(key: &KeyPress) -> bool {
    matches!(key.status, Status::Focused { .. })
}

fn is_enter(key: &KeyPress) -> bool {
    is_key(key, keyboard::key::Named::Enter) && !key.modifiers.shift()
}

fn is_key(key: &KeyPress, named: keyboard::key::Named) -> bool {
    matches!(&key.key, keyboard::Key::Named(k) if *k == named)
}
