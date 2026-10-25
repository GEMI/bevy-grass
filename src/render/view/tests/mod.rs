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

#[test]
fn a_degenerate_matrix_falls_back_to_the_far_plane() {
    let planes = frustum_planes(&Mat4::ZERO);
    for plane in planes {
        assert_eq!(plane, Vec4::W);
    }
}

#[test]
fn an_infinite_far_plane_only_degenerates_that_plane() {
    // A reversed-depth infinite projection: row 2 is [0, 0, 0, near], so its
    // truncation has no length and the far plane falls back to Vec4::W.
    let near = 0.1;
    let projection = Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, -1.0),
        Vec4::new(0.0, 0.0, near, 0.0),
    );
    let planes = frustum_planes(&projection);
    assert_eq!(planes[5], Vec4::W, "the far plane");
    // The finite planes still contain a point down the view's -Z axis.
    for (i, plane) in planes.iter().enumerate().filter(|(i, _)| *i != 5) {
        assert_ne!(*plane, Vec4::W, "plane {i} is degenerate");
        let p = Vec3::new(0.0, 0.0, -1000.0);
        assert!(
            plane.truncate().dot(p) + plane.w >= 0.0,
            "plane {i} excludes {p}"
        );
    }
}

#[test]
fn view_uniform_from_parts_builds_a_perspective_view() {
    let clip_from_view = Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.0, -1.0),
        Vec4::new(0.0, 0.0, 0.1, 0.0),
    );
    let parts = ViewParts {
        world_from_view: Mat4::IDENTITY,
        clip_from_view,
        clip_from_world: None,
        viewport: UVec4::new(0, 0, 1920, 1080),
        position: Vec3::ZERO,
        forward: Vec3::NEG_Z,
    };
    let settings = GrassSettings::HIGH;
    let capacities = [600_000, 300_000, 300_000];
    let data = view_uniform_from_parts(&parts, &settings, 0.0, &capacities, None);

    // A perspective view uses the distance table's fixed LOD.
    assert_eq!(data.fixed_lod, DISTANCE_LOD);
    assert_eq!(data.bucket_lod, UVec4::new(0, 1, 2, 0));
    // The camera sits at the origin and looks down -Z.
    assert_eq!(data.camera_position, Vec4::ZERO);
    let pixel_scale = 2.0 / (clip_from_view.x_axis.x * 1920.0);
    assert_eq!(data.camera_forward, Vec4::new(0.0, 0.0, -1.0, pixel_scale));
    // The extent is the radius plus 16 m; the origin snaps to its step.
    let extent = settings.radius + 16.0;
    assert_eq!(data.instance_origin, Vec4::new(0.0, 0.0, 0.0, extent));
    // The height range is at least 256 m.
    assert_eq!(data.limits.w, extent.max(MIN_HEIGHT_RANGE));
    // Buckets follow the capacities.
    assert_eq!(data.bucket_base, UVec4::new(0, 600_000, 900_000, 0));
    assert_eq!(
        data.bucket_capacity,
        UVec4::new(600_000, 300_000, 300_000, 0)
    );
    // The distance table is scaled to the radius.
    assert_eq!(data.radius, settings.radius);
    assert_eq!(data.lod1_start, settings.radius * settings.lod1_start);
    assert_eq!(data.lod2_start, settings.radius * settings.lod2_start);
    assert_eq!(data.lod_band, settings.lod_blend);
    assert_eq!(
        data.mid_range_start,
        settings.radius * settings.mid_range_start
    );
    assert_eq!(
        data.lod_density,
        Vec4::new(
            settings.lod2_density,
            settings.fade_density,
            settings.mid_range_density,
            settings.fade_start
        )
    );
    // The rest comes straight from the settings.
    assert_eq!(data.density_scale, settings.density_scale);
    assert_eq!(data.max_visible_chunks, settings.max_chunk_slices);
    assert_eq!(data.min_width_px, settings.min_width_px);
    assert_eq!(data.limits.x, settings.max_widening);
    assert_eq!(data.limits.y, settings.max_interactor_radius);
    assert_eq!(data.limits.z, settings.turn_to_camera);
    // A fresh view has no interactors, one layer and no sun.
    assert_eq!(data.interactor_count, 0);
    assert_eq!(data.layers, 1);
    assert_eq!(data.sun, Vec4::ZERO);
    assert_eq!(data.trample_origin, IVec4::ZERO);
    assert_eq!(data.trample_step, Vec4::ZERO);
    // The frustum planes are computed from the projection.
    assert_eq!(data.planes.len(), 6);
}

#[test]
fn view_uniform_from_parts_builds_an_orthographic_view() {
    // A plain orthographic projection: w_axis.w == 1.0 marks it as such.
    let clip_from_view = Mat4::from_cols(
        Vec4::new(0.01, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 0.01, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 0.001, 0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    );
    let parts = ViewParts {
        world_from_view: Mat4::IDENTITY,
        clip_from_view,
        clip_from_world: None,
        viewport: UVec4::new(0, 0, 1920, 1080),
        position: Vec3::ZERO,
        forward: Vec3::NEG_Z,
    };
    let settings = GrassSettings::HIGH;
    let data = view_uniform_from_parts(&parts, &settings, 0.0, &[600_000, 300_000, 300_000], None);
    // Orthographic views use the settings' orthographic extent and LOD.
    assert_eq!(data.fixed_lod, settings.orthographic_lod);
    assert_eq!(
        data.bucket_lod,
        UVec4::new(settings.orthographic_lod, 1, 2, 0)
    );
    assert_eq!(data.instance_origin.w, settings.orthographic_extent);
    assert_eq!(
        data.limits.w,
        settings.orthographic_extent.max(MIN_HEIGHT_RANGE)
    );
    // The camera position's w is 1 for orthographic views.
    assert_eq!(data.camera_position.w, 1.0);
}

#[test]
fn a_shadow_light_fills_the_shadow_offset() {
    let parts = ViewParts {
        world_from_view: Mat4::IDENTITY,
        clip_from_view: Mat4::IDENTITY,
        clip_from_world: Some(Mat4::IDENTITY),
        viewport: UVec4::new(0, 0, 1920, 1080),
        position: Vec3::ZERO,
        forward: Vec3::NEG_Z,
    };
    // A light at a normal angle: down = max(0.5, 0.2) = 0.5.
    let data = view_uniform_from_parts(
        &parts,
        &GrassSettings::HIGH,
        0.0,
        &[600_000, 300_000, 300_000],
        Some(Vec3::new(0.5, -0.5, 0.25)),
    );
    assert_eq!(data.shadow_offset, Vec4::new(1.0, 0.0, 0.5, 1.0));
    // A light almost overhead clamps `down` to 0.2.
    let data = view_uniform_from_parts(
        &parts,
        &GrassSettings::HIGH,
        0.0,
        &[600_000, 300_000, 300_000],
        Some(Vec3::new(0.1, -0.01, 0.2)),
    );
    assert_eq!(data.shadow_offset, Vec4::new(0.5, 0.0, 1.0, 1.0));
}
