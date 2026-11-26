pub use crate::ids::SourceId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Pixel rectangle in canvas space, origin top-left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn center(self) -> Vec2 {
        Vec2::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn right(self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }
}

/// Per-source placement. `crop` is in source-space normalised units (0.0..1.0),
/// which keeps the same crop valid when the capture resolution changes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub rect: Rect,
    pub rotation_deg: f32,
    pub crop: [f32; 4],
    pub visible: bool,
    pub locked: bool,
}

impl Transform {
    pub fn fit(width: f32, height: f32) -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, width, height),
            rotation_deg: 0.0,
            crop: [0.0; 4],
            visible: true,
            locked: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
}

/// What feeds a source. Capture variants are declared here as configuration;
/// the platform layer in `fbs-capture` turns them into live frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    Color { rgba: [u8; 4] },
    Image { path: String },
    /// Full-screen or single-window desktop capture with cursor toggle.
    DesktopCapture { monitor: usize, cursor: bool },
    /// Webcam by device index; resolution is negotiated by the backend.
    Camera { device: usize, width: u32, height: u32, fps: u32 },
    /// System loopback or a named microphone.
    AudioInput { device: String, system_output: bool },
    /// Lower third / caption driven by the title engine.
    Text { text: String, font: String, size_pt: f32, rgb: [u8; 3] },
    /// Animated browser overlay rendered offscreen (scoreboard, ticker, alert).
    Browser { url: String, width: u32, height: u32 },
    /// A second scene rendered into this source (vMix-style nested scene).
    Scene { scene: crate::ids::SceneId },
    /// Local media file with loop and trim settings.
    Media { path: String, looped: bool, muted: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    pub name: String,
    pub kind: SourceKind,
    pub transform: Transform,
    pub blend: BlendMode,
    pub opacity: f32,
    /// Mixer settings: 0.0..=1.0 fader, dB shown in UI.
    pub volume: f32,
    pub muted: bool,
    pub solo: bool,
    /// Audio-follows-video used by the director when a scene is taken live.
    pub follows_video: bool,
    /// Offset in milliseconds for lip-sync correction of capture sources.
    pub sync_offset_ms: i32,
}

impl Source {
    pub fn new(name: impl Into<String>, kind: SourceKind) -> Self {
        Self {
            id: SourceId::new(),
            name: name.into(),
            kind,
            transform: Transform::fit(1920.0, 1080.0),
            blend: BlendMode::Normal,
            opacity: 1.0,
            volume: 1.0,
            muted: false,
            solo: false,
            follows_video: true,
            sync_offset_ms: 0,
        }
    }

    pub fn with_transform(mut self, rect: Rect) -> Self {
        self.transform.rect = rect;
        self
    }

    pub fn is_audio_capable(&self) -> bool {
        matches!(
            self.kind,
            SourceKind::AudioInput { .. } | SourceKind::Media { .. } | SourceKind::Camera { .. }
        )
    }
}
