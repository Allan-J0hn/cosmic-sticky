// SPDX-License-Identifier: GPL-3.0-only

mod desk;
mod note_view;

use std::collections::HashMap;

use cosmic::app::{Core, Task};
use cosmic::cctk::sctk::reexports::client::protocol::wl_output::WlOutput;
use cosmic::iced::core::window::Id as SurfaceId;
use cosmic::iced::event::wayland::{Event as WaylandEvent, OutputEvent};
use cosmic::iced::event::{self, Event, PlatformSpecific};
use cosmic::iced::widget::pin;
use cosmic::iced::{Color, Length, Subscription, mouse};
use cosmic::{Element, executor, widget};

use cosmic_sticky::model::geometry::Rect;
use desk::Desk;

const DEMO_NOTE: Rect = Rect::new(96.0, 96.0, 280.0, 200.0);

pub struct App {
    core: Core,
    desks: HashMap<WlOutput, Desk>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Output(Box<OutputEvent>, WlOutput),
    PointerPressed(SurfaceId),
}

impl App {
    fn note_rects(&self) -> impl Iterator<Item = Rect> + use<> {
        std::iter::once(DEMO_NOTE)
    }

    fn desk_for_surface(&self, surface: SurfaceId) -> Option<&Desk> {
        self.desks.values().find(|d| d.surface == surface)
    }

    fn on_output(&mut self, event: OutputEvent, output: WlOutput) -> Task<Message> {
        match event {
            OutputEvent::Created(Some(info)) => {
                let name = info.name.clone().unwrap_or_default();
                let size = info
                    .logical_size
                    .map_or((0.0, 0.0), |(w, h)| (w as f32, h as f32));
                tracing::info!(output = %name, ?size, "output added");
                let desk = Desk::new(name, size);
                let task = desk.create(&output, self.note_rects());
                if let Some(old) = self.desks.insert(output, desk) {
                    return Task::batch([old.destroy(), task]);
                }
                task
            }
            OutputEvent::Created(None) => {
                tracing::warn!("output added without info; ignoring");
                Task::none()
            }
            OutputEvent::InfoUpdate(info) => {
                let rects = self.note_rects();
                let Some(desk) = self.desks.get_mut(&output) else {
                    return Task::none();
                };
                if let Some((w, h)) = info.logical_size {
                    desk.size = (w as f32, h as f32);
                }
                if let Some(name) = info.name {
                    desk.output_name = name;
                }
                desk.set_input_region(rects)
            }
            OutputEvent::Removed => match self.desks.remove(&output) {
                Some(desk) => {
                    tracing::info!(output = %desk.output_name, "output removed");
                    desk.destroy()
                }
                None => Task::none(),
            },
        }
    }
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

        let app = App {
            core,
            desks: HashMap::new(),
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
            Event::Mouse(mouse::Event::ButtonPressed(_)) => Some(Message::PointerPressed(surface)),
            _ => None,
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Output(event, output) => self.on_output(*event, output),
            Message::PointerPressed(surface) => {
                if let Some(desk) = self.desk_for_surface(surface) {
                    tracing::debug!(output = %desk.output_name, "pointer press on desk");
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        widget::space().into()
    }

    fn view_window(&self, surface: SurfaceId) -> Element<'_, Message> {
        if self.desk_for_surface(surface).is_none() {
            return widget::space().into();
        }
        pin(note_view::demo(DEMO_NOTE))
            .x(DEMO_NOTE.x)
            .y(DEMO_NOTE.y)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
