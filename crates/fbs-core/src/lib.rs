//! FBS core: the studio model, independent of any windowing, GPU or capture code.
//!
//! Everything here is plain data plus deterministic state machines, so it can be
//! unit tested on a laptop and reused by the UI, the renderer and the recorder.

#![forbid(unsafe_code)]

pub mod audio;
pub mod director;
pub mod ids;
pub mod project;
pub mod scene;
pub mod source;

pub use director::{Canvas, Director, TransitionKind};
pub use project::Project;
pub use scene::{Scene, SceneGraph};
pub use source::{BlendMode, Rect, Source, SourceKind, Transform, Vec2};
