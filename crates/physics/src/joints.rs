//! 关节描述符与句柄类型。
//!
//! 封装 rapier 的 impulse joint，提供与物理场景交互的关节类型。

use rapier3d::prelude as rp;

use crate::handles::SceneId;

/// 关节句柄，用于引用已创建的关节。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JointHandle {
    pub(crate) scene: SceneId,
    pub(crate) inner: rp::ImpulseJointHandle,
}

impl JointHandle {
    pub(crate) fn new(scene: SceneId, inner: rp::ImpulseJointHandle) -> Self {
        Self { scene, inner }
    }
}

/// 关节描述符枚举。
#[derive(Debug, Clone)]
pub enum JointDesc {
    /// 球关节（3 DOF 旋转，如肩/髋关节）
    Spherical(SphericalJointDesc),
    /// 旋转关节（1 DOF 绕轴旋转，如肘/膝关节）
    Revolute(RevoluteJointDesc),
    /// 固定关节（0 DOF）
    Fixed(FixedJointDesc),
}

/// 球关节描述符。
#[derive(Debug, Clone, Copy)]
pub struct SphericalJointDesc {
    /// 局部锚点（body1 坐标系）
    pub local_anchor1: (f32, f32, f32),
    /// 局部锚点（body2 坐标系）
    pub local_anchor2: (f32, f32, f32),
    /// 锥形限制半角（弧度），None 表示无限制
    pub cone_limit: Option<f32>,
}

impl Default for SphericalJointDesc {
    fn default() -> Self {
        Self {
            local_anchor1: (0.0, 0.0, 0.0),
            local_anchor2: (0.0, 0.0, 0.0),
            cone_limit: None,
        }
    }
}

/// 旋转关节描述符。
#[derive(Debug, Clone, Copy)]
pub struct RevoluteJointDesc {
    /// 局部锚点（body1 坐标系）
    pub local_anchor1: (f32, f32, f32),
    /// 局部锚点（body2 坐标系）
    pub local_anchor2: (f32, f32, f32),
    /// 旋转轴（body1 坐标系）
    pub axis: (f32, f32, f32),
    /// 角度范围限制 `(min, max)` 弧度，None 表示无限制
    pub angle_limit: Option<(f32, f32)>,
}

impl Default for RevoluteJointDesc {
    fn default() -> Self {
        Self {
            local_anchor1: (0.0, 0.0, 0.0),
            local_anchor2: (0.0, 0.0, 0.0),
            axis: (0.0, 1.0, 0.0),
            angle_limit: None,
        }
    }
}

/// 固定关节描述符。
#[derive(Debug, Clone, Copy)]
pub struct FixedJointDesc {
    /// 局部锚点（body1 坐标系）
    pub local_anchor1: (f32, f32, f32),
    /// 局部锚点（body2 坐标系）
    pub local_anchor2: (f32, f32, f32),
    /// 局部坐标系（body1），用于对齐两刚体
    pub local_frame1: (f32, f32, f32, f32),
    /// 局部坐标系（body2）
    pub local_frame2: (f32, f32, f32, f32),
}

impl Default for FixedJointDesc {
    fn default() -> Self {
        Self {
            local_anchor1: (0.0, 0.0, 0.0),
            local_anchor2: (0.0, 0.0, 0.0),
            local_frame1: (0.0, 0.0, 0.0, 1.0),
            local_frame2: (0.0, 0.0, 0.0, 1.0),
        }
    }
}
