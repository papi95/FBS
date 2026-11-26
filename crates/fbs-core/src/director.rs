use crate::ids::{SceneId, SourceId};
use crate::scene::SceneGraph;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

impl Canvas {
    pub const HD720: Self = Self { width: 1280, height: 720 };
    pub const HD1080: Self = Self { width: 1920, height: 1080 };

    pub fn aspect(self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    Cut,
    Fade,
    /// Horizontal slide from preview side.
    SlideLeft,
    /// Vertical wipe, vMix style.
    WipeDown,
    /// CastDeck-style stinger driven by a short alpha video.
    Stinger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Running { from: SceneId, to: SceneId },
}

/// Frame budget kept low on purpose: an 840 G2 should not burn cycles on a
/// transition it cannot composite smoothly at 30p.
const DEFAULT_TRANSITION_MS: u32 = 300;

pub struct Director {
    pub graph: SceneGraph,
    pub canvas: Canvas,
    pub program: SceneId,
    pub preview: Option<SceneId>,
    pub fps: u32,
    kind: TransitionKind,
    duration_ms: u32,
    elapsed_ms: u32,
    phase: Phase,
    /// Audio meters per source, 0.0..=1.0 peak, decayed by the mixer tick.
    pub meters: Vec<(SourceId, f32)>,
}

impl Director {
    pub fn new(graph: SceneGraph, canvas: Canvas) -> Self {
        let first = graph.scenes.first().map(|s| s.id).unwrap_or_default();
        Self {
            graph,
            canvas,
            program: first,
            preview: None,
            fps: 30,
            kind: TransitionKind::Fade,
            duration_ms: DEFAULT_TRANSITION_MS,
            elapsed_ms: 0,
            phase: Phase::Idle,
            meters: Vec::new(),
        }
    }

    pub fn transition_kind(&self) -> TransitionKind {
        self.kind
    }

    pub fn set_transition(&mut self, kind: TransitionKind, duration_ms: u32) {
        self.kind = kind;
        self.duration_ms = duration_ms.max(1);
    }

    pub fn progress(&self) -> f32 {
        if self.duration_ms == 0 {
            return 1.0;
        }
        (self.elapsed_ms as f32 / self.duration_ms as f32).clamp(0.0, 1.0)
    }

    pub fn is_live(&self) -> bool {
        matches!(self.phase, Phase::Running { .. })
    }

    pub fn set_preview(&mut self, id: SceneId) -> Result<(), String> {
        if self.graph.position(id).is_none() {
            return Err(format!("scene {:?} does not exist", id));
        }
        self.preview = Some(id);
        Ok(())
    }

    /// Send preview to program using the configured transition.
    pub fn take(&mut self) -> Result<(), String> {
        let Some(target) = self.preview else {
            return Err("no scene on preview".into());
        };
        self.start(target)
    }

    /// Direct cut to a scene, used by hotkeys and the remote control feed.
    pub fn cut_to(&mut self, target: SceneId) -> Result<(), String> {
        if self.graph.position(target).is_none() {
            return Err(format!("scene {:?} does not exist", target));
        }
        self.finish(target);
        Ok(())
    }

    fn start(&mut self, target: SceneId) -> Result<(), String> {
        if self.is_live() {
            return Err("a transition is already running".into());
        }
        if target == self.program {
            return Ok(());
        }
        if self.duration_ms <= 1 || self.kind == TransitionKind::Cut {
            self.finish(target);
            return Ok(());
        }
        self.phase = Phase::Running { from: self.program, to: target };
        self.elapsed_ms = 0;
        Ok(())
    }

    fn finish(&mut self, target: SceneId) {
        self.program = target;
        self.phase = Phase::Idle;
        self.elapsed_ms = 0;
    }

    /// Advance the transition clock. Called once per rendered frame with real
    /// frame delta, so a slow machine does not run transitions in slow motion.
    pub fn tick(&mut self, dt_ms: u32) {
        let Phase::Running { to, .. } = self.phase else {
            return;
        };
        self.elapsed_ms = self.elapsed_ms.saturating_add(dt_ms);
        if self.elapsed_ms >= self.duration_ms {
            self.finish(to);
        }
    }

    /// Manual T-bar: 0.0 is fully on program, 1.0 fully on preview.
    /// Values in between hold a partial mix, which is what vMix operators expect.
    pub fn set_tbar(&mut self, pos: f32) {
        let pos = pos.clamp(0.0, 1.0);
        let running = match self.phase {
            Phase::Running { from, to } => Some((from, to)),
            Phase::Idle => None,
        };
        if let Some((from, to)) = running {
            self.elapsed_ms = (pos * self.duration_ms as f32) as u32;
            if pos >= 1.0 {
                self.finish(to);
            } else if pos <= 0.0 {
                self.finish(from);
            }
            return;
        }
        if let Some(target) = self.preview {
            if pos >= 0.999 {
                self.finish(target);
            } else if pos > 0.0 {
                self.phase = Phase::Running { from: self.program, to: target };
                self.elapsed_ms = (pos * self.duration_ms as f32) as u32;
            }
        }
    }

    pub fn scenes_ordered(&self) -> Vec<(SceneId, &str)> {
        self.graph.scenes.iter().map(|s| (s.id, s.name.as_str())).collect()
    }

    pub fn live_scene(&self) -> Option<&crate::scene::Scene> {
        self.graph.get(self.program)
    }

    pub fn preview_scene(&self) -> Option<&crate::scene::Scene> {
        self.preview.and_then(|id| self.graph.get(id))
    }
}
