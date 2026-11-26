use fbs_core::director::{Canvas, Director, TransitionKind};
use fbs_core::scene::Scene;
use fbs_core::source::{Rect, Source, SourceKind};

fn main() {
    // Demo: build two scenes, fade one to program, then report the result.
    let mut graph = fbs_core::scene::SceneGraph::default();

    let mut main = Scene::new("Main");
    main.add(Source::new(
        "Camera 1",
        SourceKind::Camera { device: 0, width: 1280, height: 720, fps: 30 },
    ));
    main.add(
        Source::new("Lower Third", SourceKind::Color { rgba: [20, 60, 120, 255] })
            .with_transform(Rect::new(80.0, 520.0, 620.0, 90.0)),
    );
    graph.add(main);
    graph.add(Scene::new("Break Card"));

    let mut d = Director::new(graph, Canvas::HD720);
    d.set_transition(TransitionKind::Fade, 300);

    let break_card = d.graph.scenes[1].id;
    d.set_preview(break_card).unwrap();
    d.take().unwrap();

    let mut t = 0u32;
    while d.is_live() {
        d.tick(33);
        t += 33;
        if d.is_live() {
            println!("t={t}ms transition {:.0}%", d.progress() * 100.0);
        }
    }
    println!("on program: {:?}", d.live_scene().map(|s| s.name.clone()));
}
