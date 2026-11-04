//! THROWAWAY feasibility spike for cosmic-sticky. Not app code.
//!
//! Tests, on the real cosmic-comp session:
//!   a) layer-shell note on Background/Bottom layer, behind windows
//!   b) pointer input: drag header to move, drag corner to resize
//!   c) keyboard input for text entry, with different keyboard-interactivity modes
//!   d) several notes, on every output
//!   e) architecture: one layer surface per note (`--arch per-note`) vs
//!      one full-output transparent surface per monitor with an input region (`--arch single`)
//!
//! Every relevant event is logged to stderr as `[ms] EVENT ...` so tests can be
//! verified from the log.
//!
//! Usage: notes-spike [--arch per-note|single] [--layer background|bottom|top]
//!                    [--kb none|ondemand|exclusive] [--raise-on-edit] [--notes N]

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Instant;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::core::window::Id as SurfaceId;
use cosmic::iced::event::wayland::{Event as WaylandEvent, OutputEvent};
use cosmic::iced::event::{self, Event, PlatformSpecific};
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer, get_layer_surface, set_input_zone,
    set_keyboard_interactivity, set_layer, set_margin, set_size,
};
use cosmic::iced::{Color, Length, Limits, Point, Rectangle, Size, Subscription, keyboard, mouse, window};
use cosmic::iced::widget::{pin, stack, text_editor};
use cosmic::widget::{self, container, mouse_area, text};
use cosmic::{Element, executor};
use cosmic::cctk::sctk::reexports::client::protocol::wl_output::WlOutput;

static START: OnceLock<Instant> = OnceLock::new();
macro_rules! log {
    ($($t:tt)*) => {{
        let ms = START.get_or_init(Instant::now).elapsed().as_millis();
        eprintln!("[{ms:>7}] {}", format!($($t)*));
    }};
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Arch {
    PerNote,
    Single,
}

#[derive(Debug, Clone)]
struct Opts {
    arch: Arch,
    layer: Layer,
    kb: KeyboardInteractivity,
    raise_on_edit: bool,
    notes_per_output: usize,
    origin: (f32, f32),
}

fn parse_opts() -> Opts {
    let mut o = Opts {
        arch: Arch::PerNote,
        layer: Layer::Bottom,
        kb: KeyboardInteractivity::OnDemand,
        raise_on_edit: false,
        notes_per_output: 2,
        origin: (200.0, 200.0),
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let v = args.get(i + 1).cloned().unwrap_or_default();
        match args[i].as_str() {
            "--arch" => {
                o.arch = if v == "single" { Arch::Single } else { Arch::PerNote };
                i += 1;
            }
            "--layer" => {
                o.layer = match v.as_str() {
                    "background" => Layer::Background,
                    "top" => Layer::Top,
                    _ => Layer::Bottom,
                };
                i += 1;
            }
            "--kb" => {
                o.kb = match v.as_str() {
                    "none" => KeyboardInteractivity::None,
                    "exclusive" => KeyboardInteractivity::Exclusive,
                    _ => KeyboardInteractivity::OnDemand,
                };
                i += 1;
            }
            "--raise-on-edit" => o.raise_on_edit = true,
            "--at" => {
                let (x, y) = v.split_once(',').expect("--at X,Y");
                o.origin = (x.parse().unwrap(), y.parse().unwrap());
                i += 1;
            }
            "--notes" => {
                o.notes_per_output = v.parse().unwrap_or(2);
                i += 1;
            }
            other => panic!("unknown arg {other}"),
        }
        i += 1;
    }
    o
}

fn main() -> cosmic::iced::Result {
    START.get_or_init(Instant::now);
    let opts = parse_opts();
    log!("START opts={opts:?} pid={}", std::process::id());
    cosmic::app::run::<Spike>(
        Settings::default().no_main_window(true).exit_on_close(false),
        opts,
    )
}

#[derive(Debug, Clone, Copy)]
enum DragKind {
    Move,
    Resize,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    kind: DragKind,
    /// Cursor position in output-logical coords at drag start.
    start_cursor: Point,
    start_rect: Rectangle,
    /// Surface-local cursor at press (per-note move uses "keep grab point under cursor").
    grab_local: Point,
    last_sent: Option<Instant>,
}

struct Note {
    id: usize,
    /// Surface showing this note (per-note arch: its own; single arch: the output surface).
    surface: SurfaceId,
    output_name: String,
    output_size: Size,
    rect: Rectangle,
    title: String,
    body: text_editor::Content,
    color: Color,
    drag: Option<Drag>,
}

struct OutputSurf {
    name: String,
    output: WlOutput,
    size: Size,
}

struct Spike {
    core: Core,
    opts: Opts,
    notes: Vec<Note>,
    /// single arch: surface -> output info
    outputs: HashMap<SurfaceId, OutputSurf>,
    /// last cursor position per surface, in surface-local coords
    cursor: HashMap<SurfaceId, Point>,
    raised: HashMap<SurfaceId, bool>,
    kb_released: std::collections::HashSet<SurfaceId>,
    next_id: usize,
}

#[derive(Debug, Clone)]
enum Msg {
    Output(OutputEvent, WlOutput),
    CursorMoved(SurfaceId, Point),
    Released(SurfaceId),
    StartDrag(usize, DragKind),
    Focus(SurfaceId, bool),
    Key(SurfaceId, String),
    Title(usize, String),
    Body(usize, text_editor::Action),
    Edit(usize),
    Entered(SurfaceId),
}

const COLORS: [Color; 4] = [
    Color::from_rgb(1.0, 0.92, 0.35),
    Color::from_rgb(0.55, 0.85, 1.0),
    Color::from_rgb(1.0, 0.6, 0.75),
    Color::from_rgb(0.6, 0.95, 0.6),
];

impl Spike {
    fn note_idx(&self, id: usize) -> Option<usize> {
        self.notes.iter().position(|n| n.id == id)
    }

    /// Margin of the per-note surface (= note's position on its output).
    fn per_note_settings(&self, n: &Note, output: &WlOutput) -> SctkLayerSurfaceSettings {
        SctkLayerSurfaceSettings {
            id: n.surface,
            layer: self.opts.layer,
            keyboard_interactivity: self.opts.kb,
            input_zone: None,
            anchor: Anchor::TOP | Anchor::LEFT,
            output: IcedOutput::Output(output.clone()),
            namespace: "cosmic-sticky-spike".into(),
            margin: IcedMargin {
                top: n.rect.y as i32,
                left: n.rect.x as i32,
                right: 0,
                bottom: 0,
            },
            size: Some((Some(n.rect.width as u32), Some(n.rect.height as u32))),
            exclusive_zone: -1,
            size_limits: Limits::NONE
                .min_width(1.0)
                .min_height(1.0)
                .max_width(8192.0)
                .max_height(8192.0),
        }
    }

    fn input_zone_for(&self, surface: SurfaceId) -> Vec<Rectangle> {
        self.notes
            .iter()
            .filter(|n| n.surface == surface)
            .map(|n| n.rect)
            .collect()
    }

    /// Cursor position in output-logical coordinates for a note's surface.
    fn global_cursor(&self, n: &Note, local: Point) -> Point {
        match self.opts.arch {
            Arch::PerNote => Point::new(n.rect.x + local.x, n.rect.y + local.y),
            Arch::Single => local,
        }
    }

    fn note_view<'a>(&'a self, n: &'a Note) -> Element<'a, Msg> {
        let id = n.id;
        let header = mouse_area(
            container(text(format!("#{} {} ({},{}) {}x{}", n.id, n.output_name, n.rect.x, n.rect.y, n.rect.width, n.rect.height)).size(12))
                .padding(6)
                .width(Length::Fill),
        )
        .on_press(Msg::StartDrag(id, DragKind::Move))
        .interaction(mouse::Interaction::Grab);

        let title = widget::text_input("Title", &n.title)
            .on_input(move |s| Msg::Title(id, s))
            .on_focus(Msg::Edit(id));

        let body = text_editor(&n.body)
            .on_action(move |a| Msg::Body(id, a))
            .height(Length::Fill);

        let grip = mouse_area(
            container(text("◢").size(16))
                .width(Length::Fixed(20.0))
                .height(Length::Fixed(20.0)),
        )
        .on_press(Msg::StartDrag(id, DragKind::Resize))
        .interaction(mouse::Interaction::ResizingDiagonallyDown);

        let color = n.color;
        let content = widget::column::with_children(vec![
            header.into(),
            title.into(),
            mouse_area(body).on_press(Msg::Edit(id)).into(),
            widget::row::with_children(vec![
                widget::space::horizontal().into(),
                grip.into(),
            ])
            .into(),
        ])
        .spacing(4)
        .padding(6);

        container(content)
            .width(Length::Fixed(n.rect.width))
            .height(Length::Fixed(n.rect.height))
            .class(cosmic::theme::Container::custom(move |_| {
                cosmic::iced::widget::container::Style {
                    background: Some(color.into()),
                    text_color: Some(Color::BLACK),
                    border: cosmic::iced::Border {
                        radius: 8.0.into(),
                        width: 1.0,
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                    },
                    ..Default::default()
                }
            }))
            .into()
    }
}

impl cosmic::Application for Spike {
    type Executor = executor::Default;
    type Flags = Opts;
    type Message = Msg;
    const APP_ID: &'static str = "dev.stickies.Spike";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, opts: Opts) -> (Self, Task<Msg>) {
        // Without this, libcosmic requests compositor blur behind every layer surface
        // when the theme has "frosted system interface" on -> the whole output is blurred
        // behind a full-output transparent surface.
        if std::env::var_os("SPIKE_BLUR").is_none() {
            let mut f = core.auto_blur();
            f.remove(cosmic::core::Auto::System);
            f.remove(cosmic::core::Auto::Window);
            core.set_auto_blur(f);
        }
        (
            Spike {
                core,
                opts,
                notes: Vec::new(),
                outputs: HashMap::new(),
                cursor: HashMap::new(),
                raised: HashMap::new(),
                kb_released: Default::default(),
                next_id: 1,
            },
            Task::none(),
        )
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: Color::BLACK,
            icon_color: Color::BLACK,
        })
    }

    fn subscription(&self) -> Subscription<Msg> {
        event::listen_with(|ev, status, wid| match ev {
            Event::PlatformSpecific(PlatformSpecific::Wayland(WaylandEvent::Output(e, o))) => {
                Some(Msg::Output(e, o))
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Msg::CursorMoved(wid, position))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Msg::Released(wid))
            }
            Event::Mouse(mouse::Event::ButtonPressed(b)) => {
                log!("POINTER_PRESS surface={wid:?} button={b:?}");
                None
            }
            Event::Mouse(mouse::Event::CursorEntered) => {
                log!("POINTER_ENTER surface={wid:?}");
                Some(Msg::Entered(wid))
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                log!("POINTER_LEAVE surface={wid:?}");
                None
            }
            Event::Window(window::Event::Focused) => Some(Msg::Focus(wid, true)),
            Event::Window(window::Event::Unfocused) => Some(Msg::Focus(wid, false)),
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                let _ = status;
                Some(Msg::Key(wid, format!("{key:?}")))
            }
            _ => None,
        })
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Output(OutputEvent::Created(info), output) => {
                let Some(info) = info else {
                    log!("OUTPUT created without info");
                    return Task::none();
                };
                let name = info.name.clone().unwrap_or_default();
                let lsize = info
                    .logical_size
                    .map(|(w, h)| Size::new(w as f32, h as f32))
                    .unwrap_or(Size::new(1920.0, 1080.0));
                log!("OUTPUT created name={name} logical_size={lsize:?} pos={:?}", info.logical_position);
                let mut tasks = Vec::new();
                let single_surface = SurfaceId::unique();
                for k in 0..self.opts.notes_per_output {
                    let id = self.next_id;
                    self.next_id += 1;
                    let surface = match self.opts.arch {
                        Arch::PerNote => SurfaceId::unique(),
                        Arch::Single => single_surface,
                    };
                    let note = Note {
                        id,
                        surface,
                        output_name: name.clone(),
                        output_size: lsize,
                        rect: Rectangle::new(
                            Point::new(self.opts.origin.0 + 320.0 * k as f32, self.opts.origin.1 + 40.0 * k as f32),
                            Size::new(260.0, 240.0),
                        ),
                        title: format!("Note {id}"),
                        body: text_editor::Content::with_text("Click here and type."),
                        color: COLORS[(id - 1) % COLORS.len()],
                        drag: None,
                    };
                    if self.opts.arch == Arch::PerNote {
                        let s = self.per_note_settings(&note, &output);
                        log!("CREATE per-note surface={surface:?} note={id} rect={:?}", note.rect);
                        tasks.push(get_layer_surface(s));
                    }
                    self.notes.push(note);
                }
                if self.opts.arch == Arch::Single {
                    self.outputs.insert(
                        single_surface,
                        OutputSurf { name: name.clone(), output: output.clone(), size: lsize },
                    );
                    let zone = self.input_zone_for(single_surface);
                    log!("CREATE single surface={single_surface:?} output={name} input_zone={zone:?}");
                    tasks.push(get_layer_surface(SctkLayerSurfaceSettings {
                        id: single_surface,
                        layer: self.opts.layer,
                        keyboard_interactivity: self.opts.kb,
                        input_zone: Some(zone),
                        anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                        output: IcedOutput::Output(output),
                        namespace: "cosmic-sticky-spike".into(),
                        margin: IcedMargin::default(),
                        size: Some((None, None)),
                        exclusive_zone: -1,
                        size_limits: Limits::NONE
                            .min_width(1.0)
                            .min_height(1.0)
                            .max_width(8192.0)
                            .max_height(8192.0),
                    }));
                }
                Task::batch(tasks)
            }
            Msg::Output(e, _) => {
                log!("OUTPUT event {e:?}");
                Task::none()
            }
            Msg::CursorMoved(sid, p) => {
                self.cursor.insert(sid, p);
                let arch = self.opts.arch;
                let Some(idx) = self.notes.iter().position(|n| n.surface == sid && n.drag.is_some())
                else {
                    return Task::none();
                };
                let g = self.global_cursor(&self.notes[idx], p);
                let n = &mut self.notes[idx];
                let d = n.drag.unwrap();
                let dx = g.x - d.start_cursor.x;
                let dy = g.y - d.start_cursor.y;
                match d.kind {
                    DragKind::Move if arch == Arch::PerNote => {
                        // Pointer coords are relative to where the compositor *currently* has
                        // the surface, which lags our set_margin requests. Events already in
                        // flight after a request are relative to the old position, so skip
                        // motion for a short window after each request, then apply
                        // "keep the grab point under the cursor".
                        let throttle = std::env::var("SPIKE_THROTTLE_MS")
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(20u128);
                        if let Some(t) = d.last_sent {
                            if t.elapsed().as_millis() < throttle {
                                return Task::none();
                            }
                        }
                        let tx = (n.rect.x + p.x - d.grab_local.x)
                            .clamp(0.0, (n.output_size.width - n.rect.width).max(0.0))
                            .round();
                        let ty = (n.rect.y + p.y - d.grab_local.y)
                            .clamp(0.0, (n.output_size.height - n.rect.height).max(0.0))
                            .round();
                        if (tx, ty) == (n.rect.x, n.rect.y) {
                            return Task::none();
                        }
                        n.rect.x = tx;
                        n.rect.y = ty;
                        n.drag.as_mut().unwrap().last_sent = Some(Instant::now());
                        log!("DRAG_MOVE note={} local={p:?} -> pos=({}, {})", n.id, n.rect.x, n.rect.y);
                        set_margin(sid, n.rect.y as i32, 0, 0, n.rect.x as i32)
                    }
                    DragKind::Move => {
                        n.rect.x = (d.start_rect.x + dx)
                            .clamp(0.0, (n.output_size.width - n.rect.width).max(0.0))
                            .round();
                        n.rect.y = (d.start_rect.y + dy)
                            .clamp(0.0, (n.output_size.height - n.rect.height).max(0.0))
                            .round();
                        log!("DRAG_MOVE note={} local={p:?} -> pos=({}, {})", n.id, n.rect.x, n.rect.y);
                        match arch {
                            Arch::PerNote => {
                                set_margin(sid, n.rect.y as i32, 0, 0, n.rect.x as i32)
                            }
                            Arch::Single => Task::none(),
                        }
                    }
                    DragKind::Resize => {
                        n.rect.width = (d.start_rect.width + dx).max(160.0).round();
                        n.rect.height = (d.start_rect.height + dy).max(120.0).round();
                        log!("DRAG_RESIZE note={} -> size=({}, {})", n.id, n.rect.width, n.rect.height);
                        match arch {
                            Arch::PerNote => set_size(
                                sid,
                                Some(n.rect.width as u32),
                                Some(n.rect.height as u32),
                            ),
                            Arch::Single => Task::none(),
                        }
                    }
                }
            }
            Msg::StartDrag(id, kind) => {
                let Some(idx) = self.note_idx(id) else { return Task::none() };
                let sid = self.notes[idx].surface;
                let local = self.cursor.get(&sid).copied().unwrap_or_default();
                let g = self.global_cursor(&self.notes[idx], local);
                let n = &mut self.notes[idx];
                n.drag = Some(Drag { kind, start_cursor: g, start_rect: n.rect, grab_local: local, last_sent: None });
                log!("DRAG_START note={id} kind={kind:?} cursor={g:?} rect={:?}", n.rect);
                Task::none()
            }
            Msg::Released(sid) => {
                let mut tasks = Vec::new();
                for n in self.notes.iter_mut().filter(|n| n.surface == sid) {
                    if let Some(d) = n.drag.take() {
                        log!("DRAG_END note={} kind={:?} rect={:?}", n.id, d.kind, n.rect);
                    }
                }
                if self.opts.arch == Arch::Single {
                    let zone = self.input_zone_for(sid);
                    log!("INPUT_ZONE surface={sid:?} {zone:?}");
                    tasks.push(set_input_zone(sid, Some(zone)));
                }
                Task::batch(tasks)
            }
            Msg::Focus(sid, focused) => {
                log!("KEYBOARD_FOCUS surface={sid:?} focused={focused}");
                if !focused && self.opts.raise_on_edit && self.raised.remove(&sid).is_some() {
                    log!("LOWER surface={sid:?} back to {:?}", self.opts.layer);
                    return Task::batch([
                        set_layer(sid, self.opts.layer),
                        set_keyboard_interactivity(sid, self.opts.kb),
                    ]);
                }
                Task::none()
            }
            Msg::Entered(sid) => {
                if self.kb_released.remove(&sid) {
                    log!("KB_RESTORE surface={sid:?} -> {:?}", self.opts.kb);
                    return set_keyboard_interactivity(sid, self.opts.kb);
                }
                Task::none()
            }
            Msg::Key(sid, k) => {
                log!("KEY surface={sid:?} key={k}");
                if !self.opts.raise_on_edit && k.contains("Escape") {
                    self.kb_released.insert(sid);
                    log!("KB_RELEASE surface={sid:?} -> None");
                    return set_keyboard_interactivity(sid, KeyboardInteractivity::None);
                }
                if self.opts.raise_on_edit && k.contains("Escape") && self.raised.remove(&sid).is_some() {
                    log!("LOWER (escape) surface={sid:?}");
                    return Task::batch([
                        set_layer(sid, self.opts.layer),
                        set_keyboard_interactivity(sid, self.opts.kb),
                    ]);
                }
                Task::none()
            }
            Msg::Title(id, s) => {
                if let Some(i) = self.note_idx(id) {
                    log!("TEXT title note={id} = {s:?}");
                    self.notes[i].title = s;
                }
                Task::none()
            }
            Msg::Body(id, a) => {
                if let Some(i) = self.note_idx(id) {
                    let is_edit = a.is_edit();
                    self.notes[i].body.perform(a);
                    if is_edit {
                        log!("TEXT body note={id} = {:?}", self.notes[i].body.text());
                    }
                }
                Task::none()
            }
            Msg::Edit(id) => {
                let Some(i) = self.note_idx(id) else { return Task::none() };
                let sid = self.notes[i].surface;
                log!("EDIT_INTENT note={id}");
                if self.opts.raise_on_edit && !self.raised.contains_key(&sid) {
                    self.raised.insert(sid, true);
                    log!("RAISE surface={sid:?} to Top + Exclusive");
                    return Task::batch([
                        set_layer(sid, Layer::Top),
                        set_keyboard_interactivity(sid, KeyboardInteractivity::Exclusive),
                    ]);
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        text("unused").into()
    }

    fn view_window(&self, sid: SurfaceId) -> Element<'_, Msg> {
        match self.opts.arch {
            Arch::PerNote => match self.notes.iter().find(|n| n.surface == sid) {
                Some(n) => self.note_view(n),
                None => text("?").into(),
            },
            Arch::Single => {
                let layers: Vec<Element<'_, Msg>> = self
                    .notes
                    .iter()
                    .filter(|n| n.surface == sid)
                    .map(|n| pin(self.note_view(n)).x(n.rect.x).y(n.rect.y).into())
                    .collect();
                let size = self.outputs.get(&sid).map(|o| o.size);
                let _ = size;
                stack(layers).width(Length::Fill).height(Length::Fill).into()
            }
        }
    }
}

#[allow(dead_code)]
fn _unused(o: &OutputSurf) -> (&str, &WlOutput) {
    (&o.name, &o.output)
}
