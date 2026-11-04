// SPDX-License-Identifier: GPL-3.0-only

use cosmic::Element;
use cosmic::iced::widget::text_editor::{self, Binding, KeyPress};
use cosmic::iced::widget::{column, container, row, text_editor as editor};
use cosmic::iced::{Background, Border, Color, Length, keyboard, mouse};
use cosmic::widget::{self, mouse_area, text};
use uuid::Uuid;

use super::Message;
use super::drag::DragKind;
use crate::fl;
use cosmic_sticky::model::{Content, Note, Rect, Rgb};

const HEADER_HEIGHT: f32 = 22.0;
const GRIP_SIZE: f32 = 16.0;
const TITLE_SIZE: f32 = 17.0;
const BODY_SIZE: f32 = 14.0;

pub struct NoteColors {
    pub background: Color,
    pub text: Color,
}

impl NoteColors {
    pub fn new(background: Rgb) -> Self {
        let rgb = |c: Rgb| Color::from_rgb8(c.r, c.g, c.b);
        Self {
            background: rgb(background),
            text: rgb(background.readable_text()),
        }
    }

    fn faded(&self, alpha: f32) -> Color {
        Color {
            a: alpha,
            ..self.text
        }
    }
}

pub struct NoteParts<'a> {
    pub note: &'a Note,
    pub title: &'a text_editor::Content,
    pub body: &'a text_editor::Content,
    pub hovered: bool,
    pub rect: Rect,
    pub colors: NoteColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Body,
}

pub fn editor_id(id: Uuid, field: Field) -> cosmic::widget::Id {
    cosmic::widget::Id::new(format!("{field:?}-{id}"))
}

pub fn view(parts: NoteParts<'_>) -> Element<'_, Message> {
    let NoteParts {
        note,
        title,
        body,
        hovered,
        rect,
        colors,
    } = parts;
    let id = note.id;
    let fg = colors.text;
    let editor_style = move |_: &cosmic::Theme, _| text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: Color { a: 0.5, ..fg },
        value: fg,
        selection: Color { a: 0.25, ..fg },
    };

    let grip_color = colors.faded(0.45);
    let header_fill = if hovered {
        colors.faded(0.07)
    } else {
        Color::TRANSPARENT
    };
    let header = mouse_area(
        container(widget::space())
            .width(Length::Fill)
            .height(Length::Fixed(HEADER_HEIGHT))
            .style(move |_| container::Style {
                background: Some(header_fill.into()),
                ..Default::default()
            }),
    )
    .on_press(Message::StartDrag(id, DragKind::Move))
    .interaction(mouse::Interaction::Grab);

    let title = editor(title)
        .id(editor_id(id, Field::Title))
        .placeholder(fl!("title-placeholder"))
        .on_action(move |a| Message::Title(id, a))
        .key_binding(move |key| title_binding(id, key))
        .size(TITLE_SIZE)
        .font(cosmic::font::semibold())
        .padding([0, 12])
        .style(editor_style);

    let content: Element<'_, Message> = match &note.content {
        Content::Text(_) => editor(body)
            .id(editor_id(id, Field::Body))
            .placeholder(fl!("body-placeholder"))
            .on_action(move |a| Message::Body(id, a))
            .size(BODY_SIZE)
            .padding([4, 12])
            .height(Length::Fill)
            .style(editor_style)
            .into(),
        Content::Checklist(items) => container(column(items.iter().map(|item| {
            let mark = if item.checked { "☑" } else { "☐" };
            row![text(mark).size(BODY_SIZE), text(&item.text).size(BODY_SIZE)]
                .spacing(6)
                .into()
        })))
        .padding([4, 12])
        .height(Length::Fill)
        .into(),
    };

    let grip: Element<'_, Message> = if hovered {
        mouse_area(
            container(text("◢").size(GRIP_SIZE - 4.0))
                .center(Length::Fixed(GRIP_SIZE))
                .style(move |_| container::Style {
                    text_color: Some(grip_color),
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
    let card = container(column![
        header,
        title,
        content,
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
    }));

    card.into()
}

/// Enter in the title moves to the body instead of inserting a line break.
fn title_binding(id: Uuid, key: KeyPress) -> Option<Binding<Message>> {
    if matches!(key.key, keyboard::Key::Named(keyboard::key::Named::Enter)) {
        return Some(Binding::Custom(Message::FocusBody(id)));
    }
    Binding::from_key_press(key)
}
