use cgmath::{
    Point3, /* , Matrix4, Vector3, InnerSpace, EuclideanSpace, Rad, Deg, PerspectiveFov */
};
use math::AABB;
use render::{ModelMesh, RenderObject};

bitflags::bitflags! {
    /// 场景对象脏标记——网络增量同步使用。
    /// 新创建的对象默认全脏（DirtyFlags::all()），由 Scene 每帧 collect 后清零。
    #[derive(Clone, Debug)]
    pub struct DirtyFlags: u8 {
        /// 位置 / 缩放 / 旋转变化
        const TRANSFORM = 0b001;
        /// Mesh 引用发生变化（切换模型）
        const MESH     = 0b010;
    }
}

// 场景对象 trait
#[derive(Clone)]
pub struct SceneObject {
    pub entity_id: String,
    pub node: usize,
    pub local_aabb: AABB,
    pub aabb: AABB,
    pub center: Point3<f32>,
    pub mesh: ModelMesh,
    pub model_matrix: [[f32; 4]; 4],
    pub normal_matrix: [[f32; 4]; 4],
    pub joint_matrices: Vec<[[f32; 4]; 4]>,
    /// 脏标记：新创建对象全脏，collect_dirty 后清零。
    pub dirty: DirtyFlags,
    /// 如果此对象由 Prefab 实例化而来，记录来源 Prefab UUID。
    /// 用于支持 "刷新 Prefab 实例" 功能。
    pub prefab_source: Option<String>,
}

impl RenderObject for SceneObject {
    fn entity_id(&self) -> &str {
        &self.entity_id
    }

    fn mesh(&self) -> &ModelMesh {
        &self.mesh
    }

    fn model_matrix(&self) -> [[f32; 4]; 4] {
        self.model_matrix
    }

    fn normal_matrix(&self) -> [[f32; 4]; 4] {
        self.normal_matrix
    }

    fn joint_matrices(&self) -> &[[[f32; 4]; 4]] {
        &self.joint_matrices
    }
}
