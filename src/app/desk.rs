// SPDX-License-Identifier: GPL-3.0-only

use cosmic::app::Task;
use cosmic::cctk::sctk::reexports::client::protocol::wl_output::WlOutput;
use cosmic::iced::core::window::Id as SurfaceId;
use cosmic::iced::event::wayland::OverlapNotifyEvent;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer, destroy_layer_surface, get_layer_surface, set_input_zone,
    set_keyboard_interactivity, set_layer,
};
use cosmic::iced::platform_specific::shell::commands::overlap_notify::overlap_notify;
use cosmic::iced::{Limits, Point, Rectangle, Size};

use cosmic_sticky::model::geometry::{OutputGeom, Rect, input_region};
use cosmic_sticky::model::stacking::Stacking;

const NAMESPACE: &str = "cosmic-sticky";
const MAX_SURFACE_EDGE: f32 = 16384.0;

/// Transparent layer surface covering one output; every note on that output is drawn inside it.
pub struct Desk {
    pub surface: SurfaceId,
    pub geom: OutputGeom,
    pub stacking: Stacking,
    /// Waiting for the drag or reveal in this desk to end.
    pub restack_pending: bool,
}

/// Another client's surface on the desk's layer, appearing or going away.
#[derive(Debug, Clone)]
pub enum Competitor {
    Overlaps(String),
    Gone(String),
}

impl Competitor {
    pub fn from_event(event: OverlapNotifyEvent) -> Option<Self> {
        match event {
            OverlapNotifyEvent::OverlapLayerAdd {
                identifier,
                namespace,
                layer: Some(Layer::Bottom),
                ..
            } if namespace != NAMESPACE => Some(Self::Overlaps(identifier)),
            OverlapNotifyEvent::OverlapLayerRemove { identifier } => Some(Self::Gone(identifier)),
            _ => None,
        }
    }
}

impl Desk {
    pub fn new(geom: OutputGeom) -> Self {
        Self {
            surface: SurfaceId::unique(),
            geom,
            stacking: Stacking::default(),
            restack_pending: false,
        }
    }

    /// A destroyed surface's id can't be reused. The new surface will be the newest on its layer.
    pub fn renew(&mut self) {
        self.surface = SurfaceId::unique();
        self.restack_pending = false;
    }

    /// Replaces the surface with a new one, which goes on top of its layer.
    pub fn recreate<M: 'static>(
        &mut self,
        output: &WlOutput,
        notes: impl IntoIterator<Item = Rect>,
    ) -> Task<M> {
        let old = self.surface;
        self.renew();
        Task::batch([self.create(output, notes), destroy(old)])
    }

    pub fn create<M: 'static>(
        &self,
        output: &WlOutput,
        notes: impl IntoIterator<Item = Rect>,
    ) -> Task<M> {
        get_layer_surface(SctkLayerSurfaceSettings {
            id: self.surface,
            // Background-layer surfaces receive no pointer input on cosmic-comp; see spike/README.md.
            layer: Layer::Bottom,
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            input_zone: Some(self.zone(notes)),
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            output: IcedOutput::Output(output.clone()),
            namespace: NAMESPACE.into(),
            margin: IcedMargin::default(),
            size: Some((None, None)),
            exclusive_zone: -1,
            size_limits: Limits::NONE
                .min_width(1.0)
                .min_height(1.0)
                .max_width(MAX_SURFACE_EDGE)
                .max_height(MAX_SURFACE_EDGE),
        })
        .chain(overlap_notify(self.surface, true))
    }

    pub fn set_input_region<M: 'static>(&self, notes: impl IntoIterator<Item = Rect>) -> Task<M> {
        set_input_zone(self.surface, Some(self.zone(notes)))
    }

    /// Above windows with the whole output taking input, so a click anywhere can end the reveal
    /// and Escape reaches us.
    pub fn raise<M: 'static>(&self) -> Task<M> {
        let (w, h) = self.geom.size;
        Task::batch([
            set_layer(self.surface, Layer::Top),
            set_keyboard_interactivity(self.surface, KeyboardInteractivity::Exclusive),
            set_input_zone(
                self.surface,
                Some(vec![Rectangle::new(Point::ORIGIN, Size::new(w, h))]),
            ),
        ])
    }

    pub fn lower<M: 'static>(&self, notes: impl IntoIterator<Item = Rect>) -> Task<M> {
        Task::batch([
            set_layer(self.surface, Layer::Bottom),
            set_keyboard_interactivity(self.surface, KeyboardInteractivity::OnDemand),
            self.set_input_region(notes),
        ])
    }

    pub fn destroy<M: 'static>(&self) -> Task<M> {
        destroy(self.surface)
    }

    fn zone(&self, notes: impl IntoIterator<Item = Rect>) -> Vec<Rectangle> {
        input_region(notes, self.geom.size)
            .into_iter()
            .map(|r| Rectangle::new(Point::new(r.x, r.y), Size::new(r.width, r.height)))
            .collect()
    }
}

fn destroy<M: 'static>(surface: SurfaceId) -> Task<M> {
    overlap_notify(surface, false).chain(destroy_layer_surface(surface))
}
