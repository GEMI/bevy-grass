use super::*;

#[test]
fn frustum_planes_contain_what_the_camera_sees() {
    use bevy::camera::{CameraProjection, PerspectiveProjection};
    use bevy::transform::components::Transform;
    let projection = PerspectiveProjection {
        fov: 1.0,
        aspect_ratio: 16.0 / 9.0,
        near: 0.1,
        ..Default::default()
    }
    .get_clip_from_view();
    let view = Transform::from_xyz(0.0, 2.0, 10.0)
        .looking_at(Vec3::ZERO, Vec3::Y)
        .to_matrix()
        .inverse();
    let planes = frustum_planes(&(projection * view));
    let inside = |p: Vec3| {
        planes
            .iter()
            .all(|plane| plane.truncate().dot(p) + plane.w >= 0.0)
    };
    assert!(inside(Vec3::ZERO));
    assert!(inside(Vec3::new(0.0, 0.0, -500.0)));
    assert!(!inside(Vec3::new(0.0, 0.0, 20.0)), "behind the camera");
    assert!(!inside(Vec3::new(100.0, 0.0, 0.0)), "far to the side");
}
