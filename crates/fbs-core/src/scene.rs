use crate::ids::{SceneId, SourceId};
use crate::source::Source;
use serde::{Deserialize, Serialize};

/// One composition. Sources are stored back-to-front; index 0 draws first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<Source>,
    /// Optional fixed canvas size for vertical/short-form output.
    pub width: u32,
    pub height: u32,
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: SceneId::new(),
            name: name.into(),
            sources: Vec::new(),
            width: 1920,
            height: 1080,
        }
    }

    pub fn add(&mut self, source: Source) -> SourceId {
        let id = source.id;
        self.sources.push(source);
        id
    }

    pub fn get(&self, id: SourceId) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: SourceId) -> Option<&mut Source> {
        self.sources.iter_mut().find(|s| s.id == id)
    }

    pub fn remove(&mut self, id: SourceId) -> Option<Source> {
        let pos = self.sources.iter().position(|s| s.id == id)?;
        Some(self.sources.remove(pos))
    }

    /// Bring a source one step forward in the stack.
    pub fn raise(&mut self, id: SourceId) -> bool {
        let Some(pos) = self.sources.iter().position(|s| s.id == id) else {
            return false;
        };
        if pos + 1 >= self.sources.len() {
            return false;
        }
        self.sources.swap(pos, pos + 1);
        true
    }

    pub fn lower(&mut self, id: SourceId) -> bool {
        let Some(pos) = self.sources.iter().position(|s| s.id == id) else {
            return false;
        };
        if pos == 0 {
            return false;
        }
        self.sources.swap(pos, pos - 1);
        true
    }

    /// Draw order, skipping hidden sources and any muted-by-solo strip.
    pub fn draw_list(&self) -> impl Iterator<Item = &Source> {
        self.sources.iter().filter(|s| s.transform.visible)
    }
}

/// All scenes plus the hotkey-friendly ordering.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneGraph {
    pub scenes: Vec<Scene>,
}

impl SceneGraph {
    pub fn add(&mut self, scene: Scene) -> SceneId {
        let id = scene.id;
        self.scenes.push(scene);
        id
    }

    pub fn get(&self, id: SceneId) -> Option<&Scene> {
        self.scenes.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: SceneId) -> Option<&mut Scene> {
        self.scenes.iter_mut().find(|s| s.id == id)
    }

    pub fn position(&self, id: SceneId) -> Option<usize> {
        self.scenes.iter().position(|s| s.id == id)
    }

    /// Studio default: a starting scene so the app is never empty.
    pub fn starter() -> Self {
        let mut graph = Self::default();
        let mut scene = Scene::new("Main");
        scene.add(Source::new(
            "Background",
            crate::source::SourceKind::Color { rgba: [16, 18, 24, 255] },
        ));
        graph.add(scene);
        graph
    }
}
