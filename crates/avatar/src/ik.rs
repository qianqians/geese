//! IK 逆向运动学 — FABRIK + CCD 求解器。
//!
//! 用于骨骼链的末端执行器追踪目标位置。

use cgmath::{InnerSpace, Quaternion, Rotation, Rotation3, Vector3};
use crate::animation::SceneNode;

/// IK 目标描述。
#[derive(Clone, Debug)]
pub struct IkTarget {
    /// 末端关节在 SceneNode 数组中的索引
    pub end_effector: usize,
    /// 目标世界坐标
    pub target_position: [f32; 3],
    /// 目标旋转（可选，None 表示不约束旋转）
    pub target_rotation: Option<[f32; 4]>,
    /// IK 权重（0 = 无影响，1 = 完全 IK）
    pub weight: f32,
    /// 最大迭代次数
    pub max_iterations: u32,
    /// 收敛容差（米）
    pub tolerance: f32,
}

impl Default for IkTarget {
    fn default() -> Self {
        Self {
            end_effector: 0,
            target_position: [0.0, 0.0, 0.0],
            target_rotation: None,
            weight: 1.0,
            max_iterations: 20,
            tolerance: 0.001,
        }
    }
}

/// IK 链描述：从根关节到末端关节的骨骼链。
#[derive(Clone, Debug)]
pub struct IkChain {
    /// 链根关节索引（SceneNode 数组）
    pub root: usize,
    /// 链中所有关节索引（从根到末端，含两端）
    pub joints: Vec<usize>,
    /// IK 目标
    pub target: IkTarget,
}

/// FABRIK (Forward And Backward Reaching Inverse Kinematics) 求解器。
///
/// 算法步骤：
/// 1. 后向（Backward）：从末端到根，将每个关节沿"到目标方向"移动
/// 2. 前向（Forward）：从根到末端，保持骨骼长度将关节移回原位方向
///
/// `nodes`: 可变的 SceneNode 数组（将直接写入求解结果）
pub fn solve_fabrik(
    chain: &IkChain,
    nodes: &mut [SceneNode],
) {
    let target = &chain.target;
    if target.weight <= 0.0 {
        return;
    }
    let target_pos = Vector3::new(
        target.target_position[0],
        target.target_position[1],
        target.target_position[2],
    );

    let joint_indices: Vec<usize> = chain.joints.clone();
    if joint_indices.len() < 2 {
        return;
    }

    // 预计算各段骨长
    let bone_lengths: Vec<f32> = {
        let mut lens = Vec::with_capacity(joint_indices.len() - 1);
        for w in joint_indices.windows(2) {
            let a = nodes[w[0]].local_transform.translation;
            let b = nodes[w[1]].local_transform.translation;
            lens.push((b - a).magnitude());
        }
        lens
    };

    let total_length: f32 = bone_lengths.iter().sum();
    let root_pos = nodes[joint_indices[0]].local_transform.translation;
    let dist_to_target = (target_pos - root_pos).magnitude();

    // 收集当前位置
    let mut positions: Vec<Vector3<f32>> = joint_indices
        .iter()
        .map(|&idx| nodes[idx].local_transform.translation)
        .collect();

    // 目标不可达：伸直链
    if dist_to_target > total_length {
        let dir = (target_pos - root_pos).normalize();
        positions[0] = root_pos;
        for i in 1..positions.len() {
            positions[i] = positions[i - 1] + dir * bone_lengths[i - 1];
        }
    } else {
        // FABRIK 迭代
        for _ in 0..target.max_iterations {
            // 检查是否已收敛
            let end_pos = *positions.last().unwrap();
            if (end_pos - target_pos).magnitude() < target.tolerance {
                break;
            }

            // 后向传递（从末端到根）
            let n = positions.len();
            positions[n - 1] = target_pos;
            for i in (0..n - 1).rev() {
                let dir = (positions[i] - positions[i + 1]).normalize();
                positions[i] = positions[i + 1] + dir * bone_lengths[i];
            }

            // 前向传递（从根到末端）
            positions[0] = root_pos;
            for i in 1..n {
                let dir = (positions[i] - positions[i - 1]).normalize();
                positions[i] = positions[i - 1] + dir * bone_lengths[i - 1];
            }
        }
    }

    // 写回节点（按 weight 混合）
    for (k, &idx) in joint_indices.iter().enumerate() {
        let original = nodes[idx].local_transform.translation;
        let solved = positions[k];
        // 按 IK weight 线性插值
        let blended = original * (1.0 - target.weight) + solved * target.weight;
        nodes[idx].local_transform.translation = blended;
    }

    // 如果指定了目标旋转，应用到末端关节
    if let Some(rot) = target.target_rotation {
        let end_idx = joint_indices[joint_indices.len() - 1];
        let target_quat = Quaternion::new(rot[3], rot[0], rot[1], rot[2]);
        let original_rot = nodes[end_idx].local_transform.rotation;
        nodes[end_idx].local_transform.rotation = slerp_quat(original_rot, target_quat, target.weight);
    }
}

/// CCD (Cyclic Coordinate Descent) 求解器。
///
/// 从末端关节向根关节逐个旋转，使末端执行器朝向目标。
///
/// `nodes`: 可变的 SceneNode 数组
pub fn solve_ccd(
    chain: &IkChain,
    nodes: &mut [SceneNode],
) {
    let target = &chain.target;
    if target.weight <= 0.0 {
        return;
    }
    let target_pos = Vector3::new(
        target.target_position[0],
        target.target_position[1],
        target.target_position[2],
    );

    let joint_indices: Vec<usize> = chain.joints.clone();
    if joint_indices.len() < 2 {
        return;
    }

    let end_effector_idx = *joint_indices.last().unwrap();

    for _ in 0..target.max_iterations {
        // 检查收敛
        let end_pos = nodes[end_effector_idx].local_transform.translation;
        if (end_pos - target_pos).magnitude() < target.tolerance {
            break;
        }

        // 从末端倒数第二个关节开始向根遍历（跳过末端本身）
        for &joint_idx in joint_indices.iter().rev().skip(1) {
            let joint_pos = nodes[joint_idx].local_transform.translation;
            let end_pos = nodes[end_effector_idx].local_transform.translation;

            let to_end = end_pos - joint_pos;
            let to_target = target_pos - joint_pos;

            let to_end_len = to_end.magnitude();
            let to_target_len = to_target.magnitude();

            if to_end_len < 1e-6 || to_target_len < 1e-6 {
                continue;
            }

            let to_end_norm = to_end / to_end_len;
            let to_target_norm = to_target / to_target_len;

            // 计算旋转轴和角度
            let dot = to_end_norm.dot(to_target_norm).clamp(-1.0, 1.0);
            let angle = dot.acos();

            if angle.abs() < 1e-6 {
                continue;
            }

            let axis = to_end_norm.cross(to_target_norm);
            let axis_len = axis.magnitude();
            if axis_len < 1e-6 {
                continue;
            }
            let axis_norm = axis / axis_len;

            let delta_quat = Quaternion::from_axis_angle(axis_norm, cgmath::Rad(angle));
            let old_rot = nodes[joint_idx].local_transform.rotation;
            nodes[joint_idx].local_transform.rotation = delta_quat * old_rot;

            // 旋转后续关节位置
            for &subsequent_idx in joint_indices.iter().filter(|&&j| j != joint_idx) {
                // 仅旋转在 joint_idx 之后的关节
                let rel = nodes[subsequent_idx].local_transform.translation - joint_pos;
                let rotated = delta_quat.rotate_vector(rel);
                nodes[subsequent_idx].local_transform.translation = joint_pos + rotated;
            }
        }
    }

    // 按 weight 混合（CCD 直接修改旋转，此处简化处理）
    // 实际应用中应在每次迭代中混合，但骨架版在迭代后整体混合
    if target.weight < 1.0 {
        // 将旋转插值回原始值
        for &joint_idx in &joint_indices {
            let original_rot = nodes[joint_idx].base_transform.rotation;
            let current_rot = nodes[joint_idx].local_transform.rotation;
            nodes[joint_idx].local_transform.rotation = slerp_quat(original_rot, current_rot, target.weight);
        }
    }

    // 应用目标旋转（若指定）
    if let Some(rot) = target.target_rotation {
        let end_idx = *joint_indices.last().unwrap();
        let target_quat = Quaternion::new(rot[3], rot[0], rot[1], rot[2]);
        let original_rot = nodes[end_idx].local_transform.rotation;
        nodes[end_idx].local_transform.rotation = slerp_quat(original_rot, target_quat, target.weight);
    }
}

/// 将 IK 求解结果写回骨骼 Transform（应用 weight 后）。
///
/// `chain`: IK 链
/// `nodes`: 场景节点数组（已就地修改）
///
/// 此函数主要用于 FABRIK 后的 world_transform 重算。
pub fn apply_ik(
    chain: &IkChain,
    nodes: &mut [SceneNode],
) {
    // 从根到末端重算世界矩阵
    for &idx in &chain.joints {
        let parent_world = nodes[idx]
            .parent
            .map(|p| nodes[p].world_transform)
            .unwrap_or_else(|| cgmath::Matrix4::from_scale(1.0));
        nodes[idx].world_transform = parent_world * nodes[idx].local_transform.matrix();
    }
}

/// 四元数球面线性插值（简化实现，不处理对径点）。
fn slerp_quat(a: Quaternion<f32>, b: Quaternion<f32>, t: f32) -> Quaternion<f32> {
    let mut dot = a.v.x * b.v.x + a.v.y * b.v.y + a.v.z * b.v.z + a.s * b.s;

    let b2 = if dot < 0.0 {
        dot = -dot;
        Quaternion::new(-b.s, -b.v.x, -b.v.y, -b.v.z)
    } else {
        b
    };

    if dot > 0.9995 {
        // 非常接近，退化为线性插值
        let result = Quaternion::new(
            a.s + t * (b2.s - a.s),
            a.v.x + t * (b2.v.x - a.v.x),
            a.v.y + t * (b2.v.y - a.v.y),
            a.v.z + t * (b2.v.z - a.v.z),
        );
        return result.normalize();
    }

    let theta_0 = dot.acos();
    let theta = theta_0 * t;
    let sin_theta = theta.sin();
    let sin_theta_0 = theta_0.sin();

    let s0 = ((1.0 - t) * theta_0).sin() / sin_theta_0;
    let s1 = sin_theta / sin_theta_0;

    Quaternion::new(
        s0 * a.s + s1 * b2.s,
        s0 * a.v.x + s1 * b2.v.x,
        s0 * a.v.y + s1 * b2.v.y,
        s0 * a.v.z + s1 * b2.v.z,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::Transform;
    use cgmath::Matrix4;

    fn make_node(id: usize, parent: Option<usize>, pos: Vector3<f32>) -> SceneNode {
        let mut t = Transform::default();
        t.translation = pos;
        let mut n = SceneNode::new(id, parent, t);
        n.world_transform = Matrix4::from_translation(pos);
        n
    }

    #[test]
    fn fabrik_reaches_target() {
        // 3 关节链：根(0) -> 中(1) -> 末端(2)，每段长 1.0
        let mut nodes = vec![
            make_node(0, None, Vector3::new(0.0, 0.0, 0.0)),
            make_node(1, Some(0), Vector3::new(1.0, 0.0, 0.0)),
            make_node(2, Some(1), Vector3::new(2.0, 0.0, 0.0)),
        ];

        let chain = IkChain {
            root: 0,
            joints: vec![0, 1, 2],
            target: IkTarget {
                end_effector: 2,
                target_position: [0.0, 1.5, 0.0],
                weight: 1.0,
                max_iterations: 50,
                tolerance: 0.01,
                target_rotation: None,
            },
        };

        solve_fabrik(&chain, &mut nodes);

        let end_pos = nodes[2].local_transform.translation;
        let target = Vector3::new(0.0, 1.5, 0.0);
        let dist = (end_pos - target).magnitude();
        assert!(dist < 0.1, "FABRIK end effector too far from target: dist={dist}");
    }

    #[test]
    fn fabrik_stretches_when_unreachable() {
        let mut nodes = vec![
            make_node(0, None, Vector3::new(0.0, 0.0, 0.0)),
            make_node(1, Some(0), Vector3::new(1.0, 0.0, 0.0)),
            make_node(2, Some(1), Vector3::new(2.0, 0.0, 0.0)),
        ];

        let chain = IkChain {
            root: 0,
            joints: vec![0, 1, 2],
            target: IkTarget {
                end_effector: 2,
                target_position: [10.0, 0.0, 0.0], // 远超出链长
                weight: 1.0,
                max_iterations: 20,
                tolerance: 0.001,
                target_rotation: None,
            },
        };

        solve_fabrik(&chain, &mut nodes);

        // 根不动
        assert!((nodes[0].local_transform.translation - Vector3::new(0.0, 0.0, 0.0)).magnitude() < 1e-4);
        // 链伸直（总长 2.0）
        let end = nodes[2].local_transform.translation;
        assert!((end - Vector3::new(2.0, 0.0, 0.0)).magnitude() < 0.01);
    }

    #[test]
    fn ccd_moves_towards_target() {
        let mut nodes = vec![
            make_node(0, None, Vector3::new(0.0, 0.0, 0.0)),
            make_node(1, Some(0), Vector3::new(1.0, 0.0, 0.0)),
            make_node(2, Some(1), Vector3::new(2.0, 0.0, 0.0)),
        ];

        let original_end = nodes[2].local_transform.translation;

        let chain = IkChain {
            root: 0,
            joints: vec![0, 1, 2],
            target: IkTarget {
                end_effector: 2,
                target_position: [0.0, 1.5, 0.0],
                weight: 1.0,
                max_iterations: 30,
                tolerance: 0.05,
                target_rotation: None,
            },
        };

        solve_ccd(&chain, &mut nodes);

        // CCD 应使末端更接近目标
        let new_end = nodes[2].local_transform.translation;
        let target = Vector3::new(0.0, 1.5, 0.0);
        let old_dist = (original_end - target).magnitude();
        let new_dist = (new_end - target).magnitude();
        assert!(new_dist < old_dist, "CCD did not move end effector closer: old={old_dist}, new={new_dist}");
    }

    #[test]
    fn zero_weight_does_nothing() {
        let mut nodes = vec![
            make_node(0, None, Vector3::new(0.0, 0.0, 0.0)),
            make_node(1, Some(0), Vector3::new(1.0, 0.0, 0.0)),
            make_node(2, Some(1), Vector3::new(2.0, 0.0, 0.0)),
        ];

        let original: Vec<Vector3<f32>> = nodes.iter().map(|n| n.local_transform.translation).collect();

        let chain = IkChain {
            root: 0,
            joints: vec![0, 1, 2],
            target: IkTarget {
                end_effector: 2,
                target_position: [0.0, 5.0, 0.0],
                weight: 0.0,
                max_iterations: 20,
                tolerance: 0.001,
                target_rotation: None,
            },
        };

        solve_fabrik(&chain, &mut nodes);
        for (i, n) in nodes.iter().enumerate() {
            assert!((n.local_transform.translation - original[i]).magnitude() < 1e-4);
        }
    }
}
