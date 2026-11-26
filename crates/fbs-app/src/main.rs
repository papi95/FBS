//! FBS control room.
//!
//! Milestone 2 draws labelled placeholders where capture frames will land. The
//! engine underneath is the real one, so scene switching, T-bar timing and the
//! mixer behave the same once video arrives.

mod style;
mod theme;

use eframe::egui;
use fbs_core::audio::Mixer;
use fbs_core::director::{Director, TransitionKind};
use fbs_core::ids::SceneId;
use fbs_core::project::Project;
use fbs_core::scene::Scene;
use fbs_core::source::{Rect, Source, SourceKind};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([1024.0, 680.0])
            .with_title("FBS — Free Broadcast Studio"),
        ..Default::default()
    };
    eframe::run_native("FBS", options, Box::new(|cc| Ok(Box::new(Studio::new(cc)))))
}

pub(crate) struct Studio {
    pub(crate) project: Project,
    pub(crate) director: Director,
    pub(crate) mixer: Mixer,
    pub(crate) tbar: f32,
    pub(crate) transition: TransitionKind,
    pub(crate) transition_ms: u32,
    pub(crate) status: String,
    pub(crate) active_scene: Option<SceneId>,
    last_frame: std::time::Instant,
    pub(crate) recording: bool,
}

impl Studio {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let project = Project::blank("Untitled Show");
        let director = project.director();
        let mut mixer = project.mixer.clone();
        // Broadcast default: a mic and a music/backing strip already on the desk.
        mixer.add(fbs_core::audio::Strip::new(
            fbs_core::ids::SourceId::new(),
            "Mic 1",
        ));
        let mut music = fbs_core::audio::Strip::new(fbs_core::ids::SourceId::new(), "Music");
        music.set_gain_db(-12.0);
        mixer.add(music);
        Self {
            project,
            director,
            mixer,
            tbar: 0.0,
            transition: TransitionKind::Fade,
            transition_ms: 300,
            status: "Ready — Space takes, Esc cuts".into(),
            active_scene: None,
            last_frame: std::time::Instant::now(),
            recording: false,
        }
    }

    pub(crate) fn take(&mut self) {
        match self.director.take() {
            Ok(()) => {
                self.status = if self.director.is_live() {
                    format!("Take — {:.0}%", self.director.progress() * 100.0)
                } else {
                    "Cut".into()
                };
                self.project.from_director(&self.director);
            }
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn cut(&mut self, id: SceneId) {
        match self.director.cut_to(id) {
            Ok(()) => {
                self.active_scene = Some(id);
                self.tbar = 0.0;
                self.status = "Cut".into();
            }
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn set_preview(&mut self, id: SceneId) {
        match self.director.set_preview(id) {
            Ok(()) => self.active_scene = Some(id),
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn new_scene(&mut self) {
        let mut scene = Scene::new(format!("Scene {}", self.director.graph.scenes.len() + 1));
        scene.add(Source::new(
            "Background",
            SourceKind::Color { rgba: [22, 24, 32, 255] },
        ));
        scene.add(Source::new(
            "Camera",
            SourceKind::Camera { device: 0, width: 1280, height: 720, fps: 30 },
        ));
        scene.add(
            Source::new("Lower Third", SourceKind::Color { rgba: [180, 40, 78, 255] })
                .with_transform(Rect::new(64.0, 560.0, 520.0, 84.0)),
        );
        let id = self.director.graph.add(scene);
        self.project.graph = self.director.graph.clone();
        self.active_scene = Some(id);
        self.status = "Scene added".into();
    }

    pub(crate) fn save_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("FBS project", &[fbs_core::project::PROJECT_EXTENSION])
            .set_file_name(format!(
                "{}.{}",
                self.project.title,
                fbs_core::project::PROJECT_EXTENSION
            ))
            .save_file()
        else {
            return;
        };
        self.project.from_director(&self.director);
        self.project.graph = self.director.graph.clone();
        let result = self
            .project
            .to_json()
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(&path, json).map_err(|e| e.to_string()));
        self.status = match result {
            Ok(()) => format!("Saved {}", path.display()),
            Err(e) => format!("Save failed: {e}"),
        };
    }

    pub(crate) fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("FBS project", &[fbs_core::project::PROJECT_EXTENSION])
            .pick_file()
        else {
            return;
        };
        let loaded = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| Project::from_json(&text))
            .and_then(|p| p.validate().map(|_| p));
        match loaded {
            Ok(p) => {
                self.director = p.director();
                self.mixer = p.mixer.clone();
                self.tbar = 0.0;
                self.project = p;
                self.status = "Project loaded".into();
            }
            Err(e) => self.status = format!("Load failed: {e}"),
        }
    }

    fn tick(&mut self) {
        let now = std::time::Instant::now();
        let dt = (now - self.last_frame).as_millis().min(250) as u32;
        self.last_frame = now;
        if dt == 0 {
            return;
        }

        self.director.tick(dt);
        self.mixer.tick();

        if self.director.is_live() {
            self.tbar = self.director.progress();
            self.status = format!("Take — {:.0}%", self.director.progress() * 100.0);
        } else if self.tbar >= 0.999 {
            self.tbar = 0.0;
        }

        // Meters move on synthetic levels until capture is wired in milestone 4.
        let elapsed = now.elapsed().as_secs_f32();
        let feeds: Vec<(fbs_core::ids::SourceId, f32)> = self
            .mixer
            .strips
            .iter()
            .enumerate()
            .map(|(i, strip)| {
                let phase = (elapsed * 2.0 + i as f32 * 1.7).sin() * 0.5 + 0.5;
                let level = if strip.muted { 0.0 } else { phase * strip.gain * 0.75 };
                (strip.source, level)
            })
            .collect();
        for (source, level) in feeds {
            self.mixer.report_peak(source, level);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.take();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let program = self.director.program;
            self.cut(program);
        }
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick();
        self.handle_keys(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        egui::TopBottomPanel::top("top").show(ctx, |ui| style::top_bar(ui, self));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| style::status_bar(ui, self));
        egui::SidePanel::left("scene_panel")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| style::scene_list(ui, self));
        egui::SidePanel::right("mixer_panel")
            .resizable(true)
            .default_width(270.0)
            .show(ctx, |ui| style::mixer_panel(ui, self));
        egui::CentralPanel::default().show(ctx, |ui| style::monitors(ui, self));
    }
}
