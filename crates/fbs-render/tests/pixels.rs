//! Pixel-level tests. These run on a software Vulkan adapter (llvmpipe) in CI and
//! on the real Intel GPU on the 840 G2, and assert actual rendered colours.

use fbs_core::scene::Scene;
use fbs_core::source::{BlendMode, Rect, Source, SourceKind};
use fbs_render::{Compositor, Gpu, SourceTextures, TransitionPass};

fn setup() -> (Compositor, SourceTextures) {
    let gpu = Gpu::new().expect("a GPU adapter is required to run render tests");
    eprintln!("test adapter: {}", gpu.report.headline());
    (Compositor::new(gpu), SourceTextures::new())
}

fn color_source(name: &str, rgba: [u8; 4], rect: Rect) -> Source {
    Source::new(name, SourceKind::Color { rgba }).with_transform(rect)
}

#[test]
fn full_frame_colour_fills_target() {
    let (mut comp, mut tex) = setup();
    let mut scene = Scene::new("S");
    scene.width = 64;
    scene.height = 64;
    let id = scene.add(color_source("red", [255, 0, 0, 255], Rect::new(0.0, 0.0, 64.0, 64.0)));
    tex.ensure_placeholders(comp.gpu(), &scene.sources);

    let target = comp.make_target(64, 64);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);
    tex.remove(id);

    let px = comp.pixel(&target, 32, 32);
    assert_eq!(px, [255, 0, 0, 255], "centre should be pure red, got {px:?}");
    let edge = comp.pixel(&target, 0, 0);
    assert_eq!(edge, [255, 0, 0, 255], "top-left should be red, got {edge:?}");
}

#[test]
fn layer_is_confined_to_its_rect() {
    let (mut comp, mut tex) = setup();
    let mut scene = Scene::new("S");
    scene.width = 64;
    scene.height = 64;
    scene.add(color_source(
        "block",
        [0, 255, 0, 255],
        Rect::new(0.0, 0.0, 32.0, 32.0),
    ));
    tex.ensure_placeholders(comp.gpu(), &scene.sources);

    let target = comp.make_target(64, 64);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);

    assert_eq!(comp.pixel(&target, 5, 5), [0, 255, 0, 255], "inside the rect");
    assert_eq!(comp.pixel(&target, 48, 48), [0, 0, 0, 255], "outside stays clear");
    // Just past the boundary.
    assert_eq!(comp.pixel(&target, 34, 5), [0, 0, 0, 255], "right edge respected");
    assert_eq!(comp.pixel(&target, 5, 34), [0, 0, 0, 255], "bottom edge respected");
}

#[test]
fn stacking_order_puts_later_source_on_top() {
    let (mut comp, mut tex) = setup();
    let mut scene = Scene::new("S");
    scene.width = 64;
    scene.height = 64;
    scene.add(color_source("under", [255, 0, 0, 255], Rect::new(0.0, 0.0, 64.0, 64.0)));
    scene.add(color_source("over", [0, 0, 255, 255], Rect::new(0.0, 0.0, 32.0, 32.0)));
    tex.ensure_placeholders(comp.gpu(), &scene.sources);

    let target = comp.make_target(64, 64);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);

    assert_eq!(comp.pixel(&target, 10, 10), [0, 0, 255, 255], "blue on top");
    assert_eq!(comp.pixel(&target, 50, 50), [255, 0, 0, 255], "red beneath");
}

#[test]
fn opacity_blends_toward_background() {
    let (mut comp, mut tex) = setup();
    let mut scene = Scene::new("S");
    scene.width = 64;
    scene.height = 64;
    let mut src = color_source("half", [255, 255, 255, 255], Rect::new(0.0, 0.0, 64.0, 64.0));
    src.opacity = 0.5;
    scene.add(src);
    tex.ensure_placeholders(comp.gpu(), &scene.sources);

    let target = comp.make_target(64, 64);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);

    let px = comp.pixel(&target, 32, 32);
    // Black background + 50% white => roughly 128.
    assert!(
        (110..=145).contains(&px[0]),
        "expected mid grey from 50% opacity, got {px:?}"
    );
}

#[test]
fn crop_scales_content_within_the_rect() {
    let (mut comp, mut tex) = setup();
    let mut scene = Scene::new("S");
    scene.width = 64;
    scene.height = 64;

    // Source texture: left half red, right half blue.
    let mut pixels = Vec::with_capacity(64 * 64 * 4);
    for _y in 0..64 {
        for x in 0..64 {
            if x < 32 {
                pixels.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                pixels.extend_from_slice(&[0, 0, 255, 255]);
            }
        }
    }

    let mut src = color_source("strip", [0, 0, 0, 255], Rect::new(0.0, 0.0, 64.0, 64.0));
    let id = scene.add(src.clone());
    tex.upload(comp.gpu(), id, 64, 64, &pixels);

    // Crop away the right half of the source: should sample only red.
    src.id = id;
    src.transform.crop = [0.0, 0.0, 0.5, 0.0];
    if let Some(s) = scene.get_mut(id) {
        s.transform.crop = [0.0, 0.0, 0.5, 0.0];
    }

    let target = comp.make_target(64, 64);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);

    let right = comp.pixel(&target, 60, 32);
    assert_eq!(right, [255, 0, 0, 255], "cropped right edge should be red, got {right:?}");
}

#[test]
fn hidden_sources_are_not_drawn() {
    let (mut comp, mut tex) = setup();
    let mut scene = Scene::new("S");
    scene.width = 32;
    scene.height = 32;
    let mut src = color_source("hidden", [255, 0, 0, 255], Rect::new(0.0, 0.0, 32.0, 32.0));
    src.transform.visible = false;
    scene.add(src);
    tex.ensure_placeholders(comp.gpu(), &scene.sources);

    let target = comp.make_target(32, 32);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(comp.pixel(&target, 16, 16), [0, 0, 0, 255]);
}

#[test]
fn multiply_and_screen_modify_the_backdrop() {
    let (mut comp, mut tex) = setup();

    // Backdrop mid grey, then a multiply layer of the same grey => darker.
    let mut scene = Scene::new("S");
    scene.width = 32;
    scene.height = 32;
    scene.add(color_source("bg", [128, 128, 128, 255], Rect::new(0.0, 0.0, 32.0, 32.0)));
    let mut mul = color_source("mul", [128, 128, 128, 255], Rect::new(0.0, 0.0, 32.0, 32.0));
    mul.blend = BlendMode::Multiply;
    scene.add(mul);
    tex.ensure_placeholders(comp.gpu(), &scene.sources);

    let target = comp.make_target(32, 32);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);
    let px = comp.pixel(&target, 16, 16);
    // 128/255 * 128/255 * 255 = 64.
    assert!(
        (50..=80).contains(&px[0]),
        "multiply should darken 128 to about 64, got {px:?}"
    );

    let mut scene2 = Scene::new("S2");
    scene2.width = 32;
    scene2.height = 32;
    scene2.add(color_source("bg", [128, 128, 128, 255], Rect::new(0.0, 0.0, 32.0, 32.0)));
    let mut scr = color_source("scr", [128, 128, 128, 255], Rect::new(0.0, 0.0, 32.0, 32.0));
    scr.blend = BlendMode::Screen;
    scene2.add(scr);
    tex.ensure_placeholders(comp.gpu(), &scene2.sources);

    let target2 = comp.make_target(32, 32);
    comp.render_scene(&target2, &scene2, &tex, [0.0, 0.0, 0.0, 1.0]);
    let px2 = comp.pixel(&target2, 16, 16);
    // Screen of 128 with 128 is about 191.
    assert!(
        (175..=210).contains(&px2[0]),
        "screen should lighten 128 to about 191, got {px2:?}"
    );
}

#[test]
fn rotate_moves_the_corner_away() {
    let (mut comp, mut tex) = setup();
    let mut scene = Scene::new("S");
    scene.width = 64;
    scene.height = 64;
    let mut src = color_source("sq", [255, 0, 0, 255], Rect::new(0.0, 0.0, 40.0, 40.0));
    src.transform.rotation_deg = 45.0;
    src.transform.rect = Rect::new(12.0, 12.0, 40.0, 40.0);
    scene.add(src);
    tex.ensure_placeholders(comp.gpu(), &scene.sources);

    let target = comp.make_target(64, 64);
    comp.render_scene(&target, &scene, &tex, [0.0, 0.0, 0.0, 1.0]);

    // A 45-degree rotated square leaves its original corners empty.
    assert_eq!(comp.pixel(&target, 13, 13), [0, 0, 0, 255], "corner should be empty");
    assert_eq!(comp.pixel(&target, 32, 32), [255, 0, 0, 255], "centre still filled");
}

#[test]
fn fade_transition_midpoint_is_average() {
    let (mut comp, mut tex) = setup();

    let mut from_scene = Scene::new("A");
    from_scene.width = 32;
    from_scene.height = 32;
    from_scene.add(color_source("red", [255, 0, 0, 255], Rect::new(0.0, 0.0, 32.0, 32.0)));

    let mut to_scene = Scene::new("B");
    to_scene.width = 32;
    to_scene.height = 32;
    to_scene.add(color_source("blue", [0, 0, 255, 255], Rect::new(0.0, 0.0, 32.0, 32.0)));

    tex.ensure_placeholders(comp.gpu(), &from_scene.sources);
    tex.ensure_placeholders(comp.gpu(), &to_scene.sources);

    let from = comp.make_target(32, 32);
    let to = comp.make_target(32, 32);
    let out = comp.make_target(32, 32);

    comp.render_scene(&from, &from_scene, &tex, [0.0, 0.0, 0.0, 1.0]);
    comp.render_scene(&to, &to_scene, &tex, [0.0, 0.0, 0.0, 1.0]);
    comp.render_transition(&out, &from, &to, TransitionPass::Fade, 0.5);

    let px = comp.pixel(&out, 16, 16);
    assert!(
        (110..=145).contains(&px[0]) && (110..=145).contains(&px[2]),
        "50% fade should be purple, got {px:?}"
    );
}

#[test]
fn cut_transition_shows_only_the_incoming_scene() {
    let (mut comp, mut tex) = setup();
    let mut a = Scene::new("A");
    a.width = 16;
    a.height = 16;
    a.add(color_source("red", [255, 0, 0, 255], Rect::new(0.0, 0.0, 16.0, 16.0)));
    let mut b = Scene::new("B");
    b.width = 16;
    b.height = 16;
    b.add(color_source("blue", [0, 0, 255, 255], Rect::new(0.0, 0.0, 16.0, 16.0)));
    tex.ensure_placeholders(comp.gpu(), &a.sources);
    tex.ensure_placeholders(comp.gpu(), &b.sources);

    let from = comp.make_target(16, 16);
    let to = comp.make_target(16, 16);
    let out = comp.make_target(16, 16);
    comp.render_scene(&from, &a, &tex, [0.0, 0.0, 0.0, 1.0]);
    comp.render_scene(&to, &b, &tex, [0.0, 0.0, 0.0, 1.0]);
    comp.render_transition(&out, &from, &to, TransitionPass::Cut, 0.1);

    assert_eq!(comp.pixel(&out, 8, 8), [0, 0, 255, 255]);
}

#[test]
fn slide_transition_puts_incoming_on_the_left() {
    let (mut comp, mut tex) = setup();
    let mut a = Scene::new("A");
    a.width = 32;
    a.height = 16;
    a.add(color_source("red", [255, 0, 0, 255], Rect::new(0.0, 0.0, 32.0, 16.0)));
    let mut b = Scene::new("B");
    b.width = 32;
    b.height = 16;
    b.add(color_source("blue", [0, 0, 255, 255], Rect::new(0.0, 0.0, 32.0, 16.0)));
    tex.ensure_placeholders(comp.gpu(), &a.sources);
    tex.ensure_placeholders(comp.gpu(), &b.sources);

    let from = comp.make_target(32, 16);
    let to = comp.make_target(32, 16);
    let out = comp.make_target(32, 16);
    comp.render_scene(&from, &a, &tex, [0.0, 0.0, 0.0, 1.0]);
    comp.render_scene(&to, &b, &tex, [0.0, 0.0, 0.0, 1.0]);
    comp.render_transition(&out, &from, &to, TransitionPass::SlideLeft, 0.5);

    assert_eq!(comp.pixel(&out, 24, 8), [0, 0, 255, 255], "right half is incoming");
    assert_eq!(comp.pixel(&out, 8, 8), [255, 0, 0, 255], "left half is outgoing");
}

#[test]
fn adapter_report_is_populated() {
    let gpu = Gpu::new().expect("adapter");
    assert!(!gpu.report.name.is_empty());
    assert!(gpu.report.max_texture_dim >= 2048);
}
