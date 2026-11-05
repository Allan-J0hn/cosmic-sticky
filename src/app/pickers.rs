// SPDX-License-Identifier: GPL-3.0-only

use cosmic::Element;
use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::widget::{column, container, row};
use cosmic::iced::{Alignment, Border, Length};
use cosmic::widget::{self, flex_row, scrollable, text};

use super::Message;
use super::note_view::{NoteColors, Panel, color, symbolic, tap};
use crate::fl;
use crate::fonts;
use cosmic_sticky::model::{Categories, Note, PALETTE, Rgb};

const SWATCH: f32 = 22.0;
const SWATCHES_PER_ROW: usize = 6;
/// Two font tiles fit side by side in a default-size note.
const FONT_TILE: f32 = 110.0;

/// Category, color or font choices, shown in place of the note's content.
pub fn view<'a>(
    note: &'a Note,
    categories: &'a Categories,
    panel: Panel,
    colors: NoteColors,
) -> Element<'a, Message> {
    let id = note.id;
    let (heading, body): (String, Element<'a, Message>) = match panel {
        Panel::Category { creating } => {
            let heading = if creating {
                fl!("new-note-category")
            } else {
                fl!("category")
            };
            let rows = categories.iter().map(|c| {
                let selected = c.id == note.category;
                tap(
                    row![
                        dot(c.color, false, colors),
                        text(&c.name).size(14).width(Length::Fill),
                        check(selected, colors),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    Message::ChooseCategory(id, c.id.clone()),
                )
            });
            (heading, column(rows).spacing(2).into())
        }
        Panel::Color => {
            let current = note.color_override;
            let swatches = PALETTE.chunks(SWATCHES_PER_ROW).map(|chunk| {
                row(chunk.iter().map(|&c| {
                    tap(
                        dot(c, current == Some(c), colors),
                        Message::SetColor(id, Some(c)),
                    )
                }))
                .spacing(2)
                .into()
            });
            let reset = tap(
                row![
                    dot(
                        categories.resolve(&note.category).color,
                        current.is_none(),
                        colors
                    ),
                    text(fl!("use-category-color")).size(14),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                Message::SetColor(id, None),
            );
            (fl!("color"), column(swatches).push(reset).spacing(4).into())
        }
        Panel::Font => {
            let tiles = fonts::ALL
                .iter()
                .map(|&f| {
                    tap(
                        row![
                            text(fonts::label(f))
                                .size(14.0 * fonts::scale(f))
                                .font(fonts::regular(f))
                                .wrapping(Wrapping::None)
                                .width(Length::Fill),
                            check(note.font == f, colors),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center)
                        .width(Length::Fixed(FONT_TILE)),
                        Message::SetFont(id, f),
                    )
                })
                .collect();
            (
                fl!("font"),
                flex_row(tiles).row_spacing(2).column_spacing(2).into(),
            )
        }
    };
    column![
        row![
            text(heading).size(13).width(Length::Fill),
            tap(
                symbolic("window-close-symbolic", colors),
                Message::ClosePanel(id)
            ),
        ]
        .align_y(Alignment::Center)
        .padding([0, 4, 0, 12]),
        scrollable(container(body).padding([2, 8])).height(Length::Fill),
    ]
    .into()
}

fn dot<'a>(c: Rgb, selected: bool, colors: NoteColors) -> Element<'a, Message> {
    let ring = if selected {
        colors.text
    } else {
        colors.faded(0.3)
    };
    container(widget::space())
        .width(SWATCH)
        .height(SWATCH)
        .style(move |_| container::Style {
            background: Some(color(c).into()),
            border: Border {
                radius: (SWATCH / 2.0).into(),
                width: if selected { 2.5 } else { 1.0 },
                color: ring,
            },
            ..Default::default()
        })
        .into()
}

fn check<'a>(selected: bool, colors: NoteColors) -> Element<'a, Message> {
    if selected {
        symbolic("object-select-symbolic", colors)
    } else {
        widget::space().width(16).into()
    }
}
