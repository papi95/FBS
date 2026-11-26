//! Render a project's scenes to PNG files with the real compositor.
//!
//! Usage:
//!   cargo run -p fbs-render --example render_project -- show.fbsproj out/
//!
//! With no arguments it renders a built-in demo scene, which doubles as a
//! smoke test of the whole GPU path on a new machine.

use std::path::PathBuf;

use fbs_core::project::Project;
use fbs_core::scene::Scene;
use fbs_core::source::{BlendMode, Rect, Source, SourceKind};
use fbs_render::{Compositor, Gpu, SourceTextures, TransitionPass};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (project, out_dir) = match args.as_slice() {
        [project_path, dir] => {
            let text = std::fs::read_to_string(project_path).expect("read project");
            let p = Project::from_json(&text).expect("parse project");
            p.validate().expect("valid project");
            (p, PathBuf::from(dir))
        }
        _ => (demo_project(), PathBuf::from("artifacts/frames")),
    };

    std::fs::create_dir_all(&out_dir).expect("create output dir");

    let gpu = Gpu::new().expect("GPU adapter");
    println!("rendering on: {}", gpu.report.headline());

    let mut comp = Compositor::new(gpu);
    let mut textures = SourceTextures::new();
    let canvas = project.canvas;

    // Every scene in the project, rendered as it would go to program.
    for (i, scene) in project.graph.scenes.iter().enumerate() {
        textures.ensure_placeholders(comp.gpu(), &scene.sources);
        let target = comp.make_target(canvas.width, canvas.height);
        comp.render_scene(&target, scene, &textures, [0.0, 0.0, 0.0, 1.0]);
        let path = out_dir.join(format!("scene_{i}_{}.png", slug(&scene.name)));
        write_png(&path, canvas.width, canvas.height, &comp.read_target(&target));
        println!("  wrote {}", path.display());
    }

    // A mid-fade between the first two scenes, to prove transitions render.
    if project.graph.scenes.len() >= 2 {
        let a = &project.graph.scenes[0];
        let b = &project.graph.scenes[1];
        let from = comp.make_target(canvas.width, canvas.height);
        let to = comp.make_target(canvas.width, canvas.height);
        let out = comp.make_target(canvas.width, canvas.height);
        comp.render_scene(&from, a, &textures, [0.0, 0.0, 0.0, 1.0]);
        comp.render_scene(&to, b, &textures, [0.0, 0.0, 0.0, 1.0]);
        comp.render_transition(&out, &from, &to, TransitionPass::Fade, 0.5);
        let path = out_dir.join("transition_midpoint.png");
        write_png(&path, canvas.width, canvas.height, &comp.read_target(&out));
        println!("  wrote {}", path.display());
    }
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

/// Minimal PNG writer (RGBA8, no compression beyond stored deflate blocks) so the
/// render crate keeps zero image dependencies.
fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) {
    let mut raw = Vec::with_capacity((w * h * 4 + h) as usize);
    for y in 0..h {
        raw.push(0);
        let start = (y * w * 4) as usize;
        raw.extend_from_slice(&rgba[start..start + (w * 4) as usize]);
    }

    let mut zlib = vec![0x78, 0x01];
    let mut offset = 0usize;
    while offset < raw.len() {
        let chunk = (raw.len() - offset).min(65535);
        let last = offset + chunk >= raw.len();
        zlib.push(if last { 1 } else { 0 });
        zlib.extend_from_slice(&(chunk as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(chunk as u16)).to_le_bytes());
        zlib.extend_from_slice(&raw[offset..offset + chunk]);
        offset += chunk;
    }
    // Adler-32 checksum of the uncompressed data.
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);

    std::fs::write(path, png).expect("write png");
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *entry = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc = table[((crc ^ byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// A small scene resembling a real layout: camera, lower third, logo bug.
fn demo_project() -> Project {
    let mut project = Project::blank("FBS Demo");
    project.canvas = fbs_core::director::Canvas::HD720;

    let mut main = Scene::new("Studio");
    main.width = 1280;
    main.height = 720;
    main.add(Source::new(
        "Backdrop",
        SourceKind::Color { rgba: [16, 20, 28, 255] },
    ));
    main.add(Source::new(
        "Camera 1",
        SourceKind::Camera { device: 0, width: 1280, height: 720, fps: 30 },
    ));
    // Lower third band.
    main.add(
        Source::new("Lower Third", SourceKind::Color { rgba: [176, 40, 76, 255] })
            .with_transform(Rect::new(64.0, 520.0, 560.0, 92.0)),
    );
    // A translucent scrim above it to show opacity working.
    let mut scrim = Source::new("Scrim", SourceKind::Color { rgba: [8, 10, 14, 255] })
        .with_transform(Rect::new(0.0, 470.0, 1280.0, 250.0));
    scrim.opacity = 0.35;
    main.add(scrim);

    let mut break_card = Scene::new("Break");
    break_card.width = 1280;
    break_card.height = 720;
    break_card.add(Source::new(
        "Break BG",
        SourceKind::Color { rgba: [24, 26, 34, 255] },
    ));
    let mut bar = Source::new("Bar", SourceKind::Color { rgba: [240, 178, 68, 255] })
        .with_transform(Rect::new(0.0, 330.0, 1280.0, 60.0));
    bar.blend = BlendMode::Normal;
    break_card.add(bar);

    project.graph = fbs_core::scene::SceneGraph::default();
    project.graph.add(main);
    project.graph.add(break_card);
    project.program = project.graph.scenes[0].id;
    project
}
