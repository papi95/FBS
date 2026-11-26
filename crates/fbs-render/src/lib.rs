//! GPU compositor for FBS.
//!
//! Colour policy: every texture is `Rgba8Unorm` and the shader does no gamma
//! conversion. Video frames arrive already gamma-encoded, so compositing in that
//! space is what broadcasters expect (and it keeps blend maths exact, which makes
//! the pixel tests meaningful).

#![forbid(unsafe_code)]

mod compositor;
mod gpu;
mod sources;

pub use compositor::{BlendModeGpu, Compositor, RenderTarget};
pub use gpu::{AdapterReport, Gpu};
pub use sources::SourceTextures;

/// How a transition is applied between two bus renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionPass {
    Cut,
    Fade,
    SlideLeft,
    WipeDown,
    /// Falls back to a fade until alpha-video stingers are wired in.
    Stinger,
}

impl TransitionPass {
    pub fn from_core(kind: fbs_core::TransitionKind) -> Self {
        match kind {
            fbs_core::TransitionKind::Cut => Self::Cut,
            fbs_core::TransitionKind::Fade => Self::Fade,
            fbs_core::TransitionKind::SlideLeft => Self::SlideLeft,
            fbs_core::TransitionKind::WipeDown => Self::WipeDown,
            fbs_core::TransitionKind::Stinger => Self::Stinger,
        }
    }

    fn shader_id(self) -> f32 {
        match self {
            Self::Cut | Self::Fade => 0.0,
            Self::SlideLeft => 1.0,
            Self::WipeDown => 2.0,
            Self::Stinger => 3.0,
        }
    }
}
