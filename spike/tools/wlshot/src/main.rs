//! THROWAWAY spike tool: screenshot every output via ext-image-copy-capture-v1 (shm).
//! Usage: wlshot <out_dir> [prefix]   -> writes <out_dir>/<prefix><output-name>.png
use std::fs::File;
use std::io::BufWriter;
use std::os::unix::fs::FileExt;
use std::os::fd::AsFd;

use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::ext::image_capture_source::v1::client::{
    ext_image_capture_source_v1, ext_output_image_capture_source_manager_v1 as srcmgr,
};
use wayland_protocols::ext::image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1 as frame, ext_image_copy_capture_manager_v1 as capmgr,
    ext_image_copy_capture_session_v1 as session,
};

#[derive(Default)]
struct St {
    outputs: Vec<(wl_output::WlOutput, String)>,
    shm: Option<wl_shm::WlShm>,
    srcmgr: Option<srcmgr::ExtOutputImageCaptureSourceManagerV1>,
    capmgr: Option<capmgr::ExtImageCopyCaptureManagerV1>,
    size: Option<(u32, u32)>,
    formats: Vec<wl_shm::Format>,
    sess_done: bool,
    ready: Option<bool>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for St {
    fn event(s: &mut Self, r: &wl_registry::WlRegistry, e: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = e {
            match interface.as_str() {
                "wl_output" => {
                    let o = r.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, ());
                    s.outputs.push((o, String::new()));
                }
                "wl_shm" => s.shm = Some(r.bind(name, 1, qh, ())),
                "ext_output_image_capture_source_manager_v1" => s.srcmgr = Some(r.bind(name, 1, qh, ())),
                "ext_image_copy_capture_manager_v1" => s.capmgr = Some(r.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}
impl Dispatch<wl_output::WlOutput, ()> for St {
    fn event(s: &mut Self, o: &wl_output::WlOutput, e: wl_output::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_output::Event::Name { name } = e {
            if let Some(x) = s.outputs.iter_mut().find(|(oo, _)| oo == o) {
                x.1 = name;
            }
        }
    }
}
impl Dispatch<session::ExtImageCopyCaptureSessionV1, ()> for St {
    fn event(s: &mut Self, _: &session::ExtImageCopyCaptureSessionV1, e: session::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match e {
            session::Event::BufferSize { width, height } => s.size = Some((width, height)),
            session::Event::ShmFormat { format } => { if let WEnum::Value(f) = format { s.formats.push(f) } }
            session::Event::Done => s.sess_done = true,
            session::Event::Stopped => eprintln!("session stopped"),
            _ => {}
        }
    }
}
impl Dispatch<frame::ExtImageCopyCaptureFrameV1, ()> for St {
    fn event(s: &mut Self, _: &frame::ExtImageCopyCaptureFrameV1, e: frame::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match e {
            frame::Event::Ready => s.ready = Some(true),
            frame::Event::Failed { reason } => {
                eprintln!("capture failed: {reason:?}");
                s.ready = Some(false)
            }
            _ => {}
        }
    }
}
delegate_noop!(St: ignore wl_shm::WlShm);
delegate_noop!(St: ignore wl_shm_pool::WlShmPool);
delegate_noop!(St: ignore wl_buffer::WlBuffer);
delegate_noop!(St: srcmgr::ExtOutputImageCaptureSourceManagerV1);
delegate_noop!(St: capmgr::ExtImageCopyCaptureManagerV1);
delegate_noop!(St: ext_image_capture_source_v1::ExtImageCaptureSourceV1);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = args.get(1).expect("usage: wlshot <dir> [prefix]").clone();
    let prefix = args.get(2).cloned().unwrap_or_default();
    let conn = Connection::connect_to_env().unwrap();
    let mut q = conn.new_event_queue();
    let qh = q.handle();
    conn.display().get_registry(&qh, ());
    let mut st = St::default();
    q.roundtrip(&mut st).unwrap();
    q.roundtrip(&mut st).unwrap();
    let outputs = st.outputs.clone();
    for (out, name) in outputs {
        st.size = None;
        st.formats.clear();
        st.sess_done = false;
        st.ready = None;
        let src = st.srcmgr.as_ref().unwrap().create_source(&out, &qh, ());
        let sess = st.capmgr.as_ref().unwrap().create_session(&src, if std::env::var_os("PAINT_CURSOR").is_some() { capmgr::Options::PaintCursors } else { capmgr::Options::empty() }, &qh, ());
        while !st.sess_done {
            q.blocking_dispatch(&mut st).unwrap();
        }
        let (w, h) = st.size.unwrap();
        let fmt = [wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888, wl_shm::Format::Xbgr8888, wl_shm::Format::Abgr8888]
            .into_iter()
            .find(|f| st.formats.contains(f))
            .unwrap_or_else(|| panic!("no 32-bit shm format: {:?}", st.formats));
        let stride = w * 4;
        let len = (stride * h) as u64;
        let path = format!("{}/wlshot-{}.buf", std::env::var("XDG_RUNTIME_DIR").unwrap(), std::process::id());
        let file = File::options().read(true).write(true).create(true).truncate(true).open(&path).unwrap();
        std::fs::remove_file(&path).ok();
        file.set_len(len).unwrap();
        let pool = st.shm.as_ref().unwrap().create_pool(file.as_fd(), len as i32, &qh, ());
        let buf = pool.create_buffer(0, w as i32, h as i32, stride as i32, fmt, &qh, ());
        let fr = sess.create_frame(&qh, ());
        fr.attach_buffer(&buf);
        fr.damage_buffer(0, 0, w as i32, h as i32);
        fr.capture();
        while st.ready.is_none() {
            q.blocking_dispatch(&mut st).unwrap();
        }
        if st.ready == Some(true) {
            let mut data = vec![0u8; len as usize];
            file.read_exact_at(&mut data, 0).unwrap();
            // little-endian XRGB8888 = B,G,R,X bytes
            for px in data.chunks_exact_mut(4) {
                if matches!(fmt, wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888) { px.swap(0, 2); }
                px[3] = 255;
            }
            // optional CROP=x,y,w,h (output-local px)
            let (mut w, mut h) = (w, h);
            if let Ok(c) = std::env::var("CROP") {
                let v: Vec<u32> = c.split(',').map(|x| x.parse().unwrap()).collect();
                let (cx, cy, cw, ch) = (v[0], v[1], v[2].min(w - v[0]), v[3].min(h - v[1]));
                let mut out = Vec::with_capacity((cw * ch * 4) as usize);
                for row in cy..cy + ch {
                    let s = ((row * w + cx) * 4) as usize;
                    out.extend_from_slice(&data[s..s + (cw * 4) as usize]);
                }
                data = out;
                (w, h) = (cw, ch);
            }
            let fname = format!("{out_dir}/{prefix}{name}.png");
            let mut enc = png::Encoder::new(BufWriter::new(File::create(&fname).unwrap()), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().unwrap().write_image_data(&data).unwrap();
            println!("{fname} {w}x{h}");
        }
        fr.destroy();
        sess.destroy();
        buf.destroy();
        pool.destroy();
    }
}
