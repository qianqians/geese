use crate::{Material, MaterialLibrary, ModelMesh};

pub trait RenderObject {
    fn entity_id(&self) -> &str;
    fn mesh(&self) -> &ModelMesh;
}

pub struct RenderCommand<'a> {
    pub entity_id: &'a str,
    pub mesh: &'a ModelMesh,
    pub material: &'a Material,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub draw_calls: usize,
    pub vertices: usize,
    pub indices: usize,
    pub missing_materials: usize,
}

pub struct RenderQueue<'a> {
    pub commands: Vec<RenderCommand<'a>>,
    pub stats: RenderStats,
}

pub struct SceneRenderer {
    default_material: Material,
}

impl SceneRenderer {
    pub fn new(default_material: Material) -> Self {
        Self { default_material }
    }

    pub fn build_queue<'a, T, I>(
        &'a self,
        materials: &'a MaterialLibrary,
        objects: I,
    ) -> RenderQueue<'a>
    where
        T: RenderObject + 'a,
        I: IntoIterator<Item = &'a T>,
    {
        let mut commands = Vec::new();
        let mut stats = RenderStats::default();

        for object in objects {
            let mesh = object.mesh();
            let material = mesh
                .material
                .and_then(|handle| materials.material(handle))
                .unwrap_or_else(|| {
                    stats.missing_materials += 1;
                    &self.default_material
                });

            stats.draw_calls += 1;
            stats.vertices += mesh.vertices.len();
            stats.indices += mesh.indices.len();

            commands.push(RenderCommand {
                entity_id: object.entity_id(),
                mesh,
                material,
            });
        }

        RenderQueue { commands, stats }
    }
}

impl Default for SceneRenderer {
    fn default() -> Self {
        Self::new(Material::default())
    }
}
