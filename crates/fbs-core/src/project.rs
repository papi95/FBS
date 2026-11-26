use crate::audio::Mixer;
use crate::director::{Canvas, Director, TransitionKind};
use crate::ids::SceneId;
use crate::scene::SceneGraph;
use serde::{Deserialize, Serialize};

pub const PROJECT_VERSION: u32 = 1;
pub const PROJECT_EXTENSION: &str = "fbsproj";

/// On-disk project. Versioned from day one so early laptop builds stay readable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub title: String,
    pub canvas: Canvas,
    pub fps: u32,
    pub graph: SceneGraph,
    pub mixer: Mixer,
    pub program: SceneId,
    pub preview: Option<SceneId>,
    pub transition: TransitionKind,
    pub transition_ms: u32,
    /// Recording/output preferences that should travel with the project.
    pub output: OutputSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSettings {
    pub record_dir: String,
    pub bitrate_kbps: u32,
    /// 0 = "Auto (Quick Sync)". A 5200U handles 720p30/1080p30 H.264 well.
    pub keyframe_secs: u32,
    pub hardware_encoder: String,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            record_dir: "recordings".into(),
            bitrate_kbps: 6000,
            keyframe_secs: 2,
            hardware_encoder: "auto".into(),
        }
    }
}

impl Project {
    pub fn blank(title: impl Into<String>) -> Self {
        let graph = SceneGraph::starter();
        let program = graph.scenes[0].id;
        Self {
            version: PROJECT_VERSION,
            title: title.into(),
            canvas: Canvas::HD720,
            fps: 30,
            graph,
            mixer: Mixer { strips: Vec::new(), master: 0.9, monitor: 1.0 },
            program,
            preview: None,
            transition: TransitionKind::Fade,
            transition_ms: 300,
            output: OutputSettings::default(),
        }
    }

    pub fn director(&self) -> Director {
        let mut d = Director::new(self.graph.clone(), self.canvas);
        d.program = self.program;
        d.preview = self.preview;
        d.fps = self.fps;
        d.set_transition(self.transition, self.transition_ms);
        d
    }

    /// Rebuild the project from a live director (used before autosave).
    pub fn from_director(&mut self, d: &Director) {
        self.program = d.program;
        self.preview = d.preview;
        self.canvas = d.canvas;
        self.fps = d.fps;
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut project: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if project.version > PROJECT_VERSION {
            return Err(format!(
                "project was written by a newer FBS (v{}, this build reads v{})",
                project.version, PROJECT_VERSION
            ));
        }
        project.version = PROJECT_VERSION;
        Ok(project)
    }

    /// Cheap integrity check to run on load so a bad file does not crash the show.
    pub fn validate(&self) -> Result<(), String> {
        if self.graph.scenes.is_empty() {
            return Err("project has no scenes".into());
        }
        if self.graph.get(self.program).is_none() {
            return Err("program scene is missing".into());
        }
        if let Some(p) = self.preview {
            if self.graph.get(p).is_none() {
                return Err("preview scene is missing".into());
            }
        }
        if !(1..=240).contains(&self.fps) {
            return Err(format!("unsupported frame rate {}", self.fps));
        }
        Ok(())
    }
}

/// Bounded undo for scene edits. Ten steps is plenty for live work and keeps
/// memory small on an 8 GB machine.
#[derive(Debug)]
pub struct UndoStack {
    states: Vec<SceneGraph>,
    capacity: usize,
}

impl UndoStack {
    pub fn new(capacity: usize) -> Self {
        Self { states: Vec::new(), capacity: capacity.max(1) }
    }

    pub fn push(&mut self, graph: &SceneGraph) {
        if self.states.len() == self.capacity {
            self.states.remove(0);
        }
        self.states.push(graph.clone());
    }

    pub fn undo(&mut self, current: &SceneGraph) -> Option<SceneGraph> {
        let previous = self.states.pop()?;
        self.states.push(current.clone());
        Some(previous)
    }

    pub fn can_undo(&self) -> bool {
        !self.states.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_scenes_and_mixer() {
        let mut p = Project::blank("Test");
        p.mixer.add(crate::audio::Strip::new(crate::ids::SourceId::new(), "Mic"));
        let json = p.to_json().unwrap();
        let back = Project::from_json(&json).unwrap();
        back.validate().unwrap();
        assert_eq!(back.graph.scenes.len(), p.graph.scenes.len());
        assert_eq!(back.mixer.strips.len(), 1);
    }

    #[test]
    fn rejects_newer_project_version() {
        let mut p = Project::blank("Future");
        p.version = PROJECT_VERSION + 1;
        let json = serde_json::to_string(&p).unwrap();
        assert!(Project::from_json(&json).is_err());
    }
}
