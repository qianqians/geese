//! Physics crate 基础行为测试：自由落体 / 静态地面碰撞 / 射线 / 销毁循环。

use approx::assert_relative_eq;
use physics::math::{Iso3, Vec3};
use physics::{BodyDesc, PhysicsWorld, ShapeDesc};

const GRAVITY: Vec3 = Vec3::new(0.0, -9.81, 0.0);

#[test]
fn free_fall_after_one_second() {
    let mut world = PhysicsWorld::new();
    let scene_id = world.create_scene(GRAVITY);
    let scene = world.scene_mut(scene_id).expect("scene exists");

    let start_y = 10.0_f32;
    let pos = Iso3::translation(0.0, start_y, 0.0);
    let (body, _col) = scene
        .add_body(
            BodyDesc::dynamic().position(pos).gravity_scale(1.0),
            ShapeDesc::cuboid(0.5, 0.5, 0.5),
        )
        .expect("add body");

    let dt = 1.0_f32 / 60.0;
    for _ in 0..60 {
        scene.step(dt);
    }
    let iso = scene.body_isometry(body).expect("isometry available");
    let dropped = start_y - iso.translation.y;
    // 解析解 0.5 * g * t^2 ≈ 4.905；rapier 半隐式积分会稍有偏差。
    assert_relative_eq!(dropped, 4.905, epsilon = 0.4);
}

#[test]
fn ball_rests_on_static_ground() {
    let mut world = PhysicsWorld::new();
    let scene_id = world.create_scene(GRAVITY);
    let scene = world.scene_mut(scene_id).expect("scene exists");

    // 地面：厚 0.2 的薄盒，y=0 居中。
    let ground_half_y = 0.1_f32;
    scene
        .add_body(
            BodyDesc::fixed().position(Iso3::translation(0.0, 0.0, 0.0)),
            ShapeDesc::cuboid(50.0, ground_half_y, 50.0),
        )
        .expect("add ground");

    let radius = 0.5_f32;
    let drop_h = 5.0_f32;
    let (ball, _) = scene
        .add_body(
            BodyDesc::dynamic().position(Iso3::translation(0.0, drop_h, 0.0)),
            ShapeDesc::ball(radius),
        )
        .expect("add ball");

    let dt = 1.0_f32 / 60.0;
    for _ in 0..240 {
        scene.step(dt);
    }
    let iso = scene.body_isometry(ball).expect("ball isometry");
    let expected_min = ground_half_y + radius - 0.05;
    assert!(
        iso.translation.y >= expected_min,
        "ball y {} should be >= {}",
        iso.translation.y,
        expected_min
    );
    let v = scene.body_linvel(ball).expect("linvel");
    assert!(v.length() < 0.5, "ball should be (almost) at rest, got {:?}", v);
}

#[test]
fn raycast_hits_ground() {
    let mut world = PhysicsWorld::new();
    let scene_id = world.create_scene(GRAVITY);
    let scene = world.scene_mut(scene_id).expect("scene exists");

    let ground_half_y = 0.1_f32;
    scene
        .add_body(
            BodyDesc::fixed().position(Iso3::translation(0.0, 0.0, 0.0)),
            ShapeDesc::cuboid(20.0, ground_half_y, 20.0),
        )
        .expect("add ground");

    // 触发一次 step，让 query_pipeline 完成首次构建。
    scene.step(1.0 / 60.0);

    let origin = Vec3::new(0.0, 5.0, 0.0);
    let dir = Vec3::new(0.0, -1.0, 0.0);
    let hit = scene.cast_ray(origin, dir, 100.0, true).expect("hit");
    let expected_toi = 5.0 - ground_half_y;
    assert_relative_eq!(hit.toi, expected_toi, epsilon = 0.05);
    assert!(hit.normal.1 > 0.5, "normal should point up, got {:?}", hit.normal);
}

#[test]
fn create_destroy_loop_is_stable() {
    let mut world = PhysicsWorld::new();
    for _ in 0..100 {
        let id = world.create_scene(GRAVITY);
        {
            let scene = world.scene_mut(id).expect("scene exists");
            let (body, _) = scene
                .add_body(
                    BodyDesc::dynamic().position(Iso3::translation(0.0, 1.0, 0.0)),
                    ShapeDesc::ball(0.3),
                )
                .expect("add body");
            scene.step(1.0 / 60.0);
            assert!(scene.contains_body(body));
            assert!(scene.remove_body(body));
            assert!(!scene.contains_body(body));
        }
        assert!(world.destroy_scene(id));
        assert!(!world.contains_scene(id));
    }
    assert_eq!(world.scene_count(), 0);
}
