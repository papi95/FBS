use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Stable identity for a source across saves, undo and the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceId(pub u64);

impl SourceId {
    pub fn new() -> Self {
        Self(next())
    }
}

impl Default for SourceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable identity for a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SceneId(pub u64);

impl SceneId {
    pub fn new() -> Self {
        Self(next())
    }
}

impl Default for SceneId {
    fn default() -> Self {
        Self::new()
    }
}
