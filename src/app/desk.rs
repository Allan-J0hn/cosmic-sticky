// SPDX-License-Identifier: GPL-3.0-only

use cosmic::app::Task;
use cosmic::cctk::sctk::reexports::client::protocol::wl_output::WlOutput;
use cosmic::iced::core::window::Id as SurfaceId;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer, destroy_layer_surface, get_layer_surface, set_input_zone,
};
use cosmic::iced::{Limits, Point, Rectangle, Size};

use cosmic_sticky::model::geometry::{OutputGeom, Rect, input_region};

const NAMESPACE: &str = "cosmic-sticky";
const MAX_SURFACE_EDGE: f32 = 16384.0;

/// Transparent layer surface covering one output; every note on that output is drawn inside it.
pub struct Desk {
    pub surface: SurfaceId,
    pub geom: OutputGeom,
}

impl Desk {
    pub fn new(geom: OutputGeom) -> Self {
        Self {
            surface: SurfaceId::unique(),
            geom,
        }
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
    }

    pub fn set_input_region<M: 'static>(&self, notes: impl IntoIterator<Item = Rect>) -> Task<M> {
        set_input_zone(self.surface, Some(self.zone(notes)))
    }

    pub fn destroy<M: 'static>(&self) -> Task<M> {
        destroy_layer_surface(self.surface)
    }

    fn zone(&self, notes: impl IntoIterator<Item = Rect>) -> Vec<Rectangle> {
        input_region(notes, self.geom.size)
            .into_iter()
            .map(|r| Rectangle::new(Point::new(r.x, r.y), Size::new(r.width, r.height)))
            .collect()
    }
}
