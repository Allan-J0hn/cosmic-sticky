// SPDX-License-Identifier: GPL-3.0-only

use cosmic::Element;
use cosmic::iced::{Border, Color, Length};
use cosmic::widget::{column, container, text};

use crate::fl;
use cosmic_sticky::model::geometry::Rect;

const DEMO_COLOR: Color = Color::from_rgb(1.0, 0.89, 0.42);

pub fn demo<'a, M: 'a>(rect: Rect) -> Element<'a, M> {
    let content = column::with_children(vec![
        text::title4(fl!("demo-title")).into(),
        text::body(fl!("demo-body")).into(),
    ])
    .spacing(8)
    .padding(12);

    container(content)
        .width(Length::Fixed(rect.width))
        .height(Length::Fixed(rect.height))
        .class(cosmic::theme::Container::custom(|theme| container::Style {
            background: Some(DEMO_COLOR.into()),
            text_color: Some(Color::BLACK),
            border: Border {
                radius: theme.cosmic().corner_radii.radius_s.into(),
                ..Default::default()
            },
            ..Default::default()
        }))
        .into()
}
