//! Texture cache for sources.
//!
//! Capture frames arrive as raw RGBA; this module turns them into GPU textures
//! and samples a 1x1 placeholder for sources that have no frames yet (camera,
//! desktop) so the compositor always has something valid to draw.

use std::collections::HashMap;

use fbs_core::ids::SourceId;
use fbs_core::source::SourceKind;

use crate::gpu::Gpu;

pub struct SourceTextures {
    entries: HashMap<SourceId, Entry>,
}

struct Entry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    /// Bumped whenever new pixels arrive, so the bind group can be rebuilt.
    generation: u64,
}

impl SourceTextures {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn get(&self, id: SourceId) -> Option<&wgpu::TextureView> {
        self.entries.get(&id).map(|e| &e.view)
    }

    pub fn generation(&self, id: SourceId) -> u64 {
        self.entries.get(&id).map(|e| e.generation).unwrap_or(0)
    }

    pub fn size(&self, id: SourceId) -> Option<(u32, u32)> {
        self.entries.get(&id).map(|e| (e.width, e.height))
    }

    pub fn has(&self, id: SourceId) -> bool {
        self.entries.contains_key(&id)
    }

    /// Replace (or create) a source's texture.
    pub fn upload(&mut self, gpu: &Gpu, id: SourceId, w: u32, h: u32, pixels: &[u8]) {
        let texture = gpu.texture_from_rgba(w, h, pixels);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let generation = self
            .entries
            .get(&id)
            .map(|e| e.generation + 1)
            .unwrap_or(1);
        self.entries.insert(
            id,
            Entry { texture, view, width: w, height: h, generation },
        );
    }

    /// Solid colour texture, used for colour sources and as a placeholder.
    pub fn solid(&mut self, gpu: &Gpu, id: SourceId, rgba: [u8; 4]) {
        self.upload(gpu, id, 1, 1, &rgba);
    }

    /// Make sure every source in the list has a texture, so a scene referencing
    /// a camera that has not produced a frame yet still renders something.
    pub fn ensure_placeholders(&mut self, gpu: &Gpu, sources: &[fbs_core::source::Source]) {
        for src in sources {
            if self.has(src.id) {
                continue;
            }
            match &src.kind {
                SourceKind::Color { rgba } => self.solid(gpu, src.id, *rgba),
                // Capture sources: dark slate until real frames arrive.
                _ => self.solid(gpu, src.id, [26, 30, 38, 255]),
            }
        }
    }

    pub fn remove(&mut self, id: SourceId) {
        self.entries.remove(&id);
    }

    /// Drop textures for sources that no longer exist, keeping VRAM bounded.
    pub fn retain_only(&mut self, live: &[SourceId]) {
        self.entries.retain(|id, _| live.contains(id));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Handy in tests: read back a texture as RGBA8.
    pub fn read_pixels(&self, gpu: &Gpu, id: SourceId) -> Option<Vec<u8>> {
        let entry = self.entries.get(&id)?;
        crate::compositor::read_texture(gpu, &entry.texture, entry.width, entry.height)
    }
}

impl Default for SourceTextures {
    fn default() -> Self {
        Self::new()
    }
}
