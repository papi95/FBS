use fbs_core::director::{Canvas, Director, TransitionKind};
use fbs_core::ids::SceneId;
use fbs_core::scene::{Scene, SceneGraph};
use fbs_core::source::{Rect, Source, SourceKind};

fn two_scenes() -> (Director, SceneId, SceneId) {
    let mut graph = SceneGraph::default();
    let a = graph.add(Scene::new("A"));
    let b = graph.add(Scene::new("B"));
    (Director::new(graph, Canvas::HD720), a, b)
}

#[test]
fn cut_is_instant() {
    let (mut d, _a, b) = two_scenes();
    d.set_transition(TransitionKind::Cut, 300);
    d.set_preview(b).unwrap();
    d.take().unwrap();
    assert_eq!(d.program, b);
    assert!(!d.is_live());
}

#[test]
fn fade_completes_in_expected_frames() {
    let (mut d, _a, b) = two_scenes();
    d.set_transition(TransitionKind::Fade, 300);
    d.set_preview(b).unwrap();
    d.take().unwrap();
    assert!(d.is_live());
    d.tick(120);
    assert!(d.progress() > 0.3 && d.progress() < 0.5);
    d.tick(200);
    assert_eq!(d.program, b);
    assert!(!d.is_live());
}

#[test]
fn tbar_holds_partial_mix_then_finishes() {
    let (mut d, _a, b) = two_scenes();
    d.set_transition(TransitionKind::Fade, 1000);
    d.set_preview(b).unwrap();
    d.set_tbar(0.5);
    assert!(d.is_live());
    assert!((d.progress() - 0.5).abs() < 0.01);
    d.set_tbar(1.0);
    assert_eq!(d.program, b);
    assert!(!d.is_live());
}

#[test]
fn take_without_preview_fails() {
    let (mut d, _a, _b) = two_scenes();
    assert!(d.take().is_err());
}

#[test]
fn take_to_current_program_is_a_noop() {
    let (mut d, a, _b) = two_scenes();
    d.set_preview(a).unwrap();
    d.take().unwrap();
    assert_eq!(d.program, a);
    assert!(!d.is_live());
}

#[test]
fn scene_stacking_survives_json() {
    let mut scene = Scene::new("Main");
    let id = scene.add(
        Source::new("Lower Third", SourceKind::Color { rgba: [0, 0, 0, 255] })
            .with_transform(Rect::new(0.0, 600.0, 1280.0, 120.0)),
    );
    scene.add(Source::new(
        "Cam",
        SourceKind::Camera { device: 0, width: 1280, height: 720, fps: 30 },
    ));
    assert!(scene.raise(id));
    let json = serde_json::to_string(&scene).unwrap();
    let back: Scene = serde_json::from_str(&json).unwrap();
    assert_eq!(back.sources.len(), 2);
    assert_eq!(back.sources.last().unwrap().id, id);
}

#[test]
fn solo_overrides_unmuted_strips() {
    let mut mixer = fbs_core::audio::Mixer { strips: Vec::new(), master: 1.0, monitor: 1.0 };
    let a = fbs_core::ids::SourceId::new();
    let b = fbs_core::ids::SourceId::new();
    mixer.add(fbs_core::audio::Strip::new(a, "Mic"));
    let mut strip_b = fbs_core::audio::Strip::new(b, "Music");
    strip_b.solo = true;
    mixer.add(strip_b);
    assert_eq!(mixer.effective_gain(a), 0.0);
    assert!(mixer.effective_gain(b) > 0.0);
}

#[test]
fn project_rejects_missing_scenes() {
    let mut p = fbs_core::Project::blank("Broken");
    p.graph.scenes.clear();
    assert!(p.validate().is_err());
}
