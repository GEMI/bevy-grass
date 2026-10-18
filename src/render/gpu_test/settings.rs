use super::*;

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_every_setting_takes_effect_at_runtime() {
    use crate::{GrassSettings, GrassShadows};

    let mut app = headless_app();
    let mut start = (ViewUniformData::default(), 0);
    for _ in 0..600 {
        start = view_after(&mut app, 1);
        if start.1 > 0 {
            break;
        }
    }
    let (view, all) = start;
    assert!(all > 10_000, "the test scene should place many blades");
    let high = GrassSettings::HIGH;
    assert_eq!(view.bucket_capacity.x, high.instance_capacity);
    assert_eq!(view.radius, DistanceLod::for_radius(high.radius).radius);

    // Density scale: half the density keeps about half the blades.
    app.world_mut()
        .resource_mut::<GrassSettings>()
        .density_scale = 0.5;
    let (view, half) = view_after(&mut app, 4);
    assert_eq!(view.density_scale, 0.5);
    assert!(
        (all as f32 * 0.4..all as f32 * 0.6).contains(&(half as f32)),
        "{half} blades at half density, {all} at full"
    );

    // Radius: a shorter reach draws fewer blades.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        radius: 20.0,
        ..high
    };
    let (view, near) = view_after(&mut app, 4);
    assert_eq!(view.radius, DistanceLod::for_radius(20.0).radius);
    assert!(
        near < all / 2,
        "{near} blades within 20 m, {all} within 100 m"
    );

    // Capacity: the buffers are made again at the new size, and the view
    // never draws past it.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        instance_capacity: 4_000,
        ..high
    };
    let (view, small) = view_after(&mut app, 6);
    assert_eq!(view.bucket_capacity.x, 4_000, "the buffers were not remade");
    assert!(
        small > 0 && small <= 4_000 + 2_000 + 2_000,
        "{small} blades"
    );
    *app.world_mut().resource_mut::<GrassSettings>() = high;
    let (view, again) = view_after(&mut app, 6);
    assert_eq!(view.bucket_capacity.x, high.instance_capacity);
    assert_eq!(
        again, all,
        "the same grass comes back with the same settings"
    );

    // Shadows are read every frame too (no light here, so only the setting
    // itself can be checked, through the extracted copy).
    app.world_mut().resource_mut::<GrassSettings>().shadows = GrassShadows::Full;
    app.update();
    app.update();
    let extracted = app
        .sub_app(RenderApp)
        .world()
        .resource::<crate::render::resources::ExtractedGrass>();
    assert_eq!(extracted.settings.shadows, GrassShadows::Full);

    // Nonsense is clamped, not trusted: a NaN radius falls back to the
    // default and a negative density draws nothing.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        radius: f32::NAN,
        ..high
    };
    let (_, nan) = view_after(&mut app, 4);
    assert_eq!(nan, all, "a NaN radius must act like the default");
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        density_scale: -3.0,
        ..high
    };
    let (_, none) = view_after(&mut app, 4);
    assert_eq!(none, 0, "a negative density scale must clamp to no grass");
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_two_cameras_draw_with_their_own_settings() {
    use crate::GrassSettings;
    use crate::api::GrassCamera;

    let mut app = headless_app();
    let main = app
        .world_mut()
        .query_filtered::<Entity, With<Camera3d>>()
        .single(app.world())
        .unwrap();
    // A minimap looking at the same grass from the same place.
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            320,
            180,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let minimap = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderTarget::Image(target.into()),
            Camera {
                order: 1,
                ..default()
            },
            Transform::from_xyz(-5.0, 1.7, 30.0).looking_at(Vec3::new(5.0, 0.0, 0.0), Vec3::Y),
            GrassCamera {
                radius: Some(20.0),
                density_scale: Some(0.5),
                min_width_px: Some(3.0),
                ..default()
            },
        ))
        .id();

    let view = |app: &mut App, camera| {
        for _ in 0..4 {
            app.update();
        }
        let (uniform, instances) = gpu_instances_of(app, camera)?;
        Some((uniform, instances.len()))
    };
    // Wait for both views to produce grass.
    let mut seen = None;
    for _ in 0..600 {
        let (a, b) = (view(&mut app, main), view(&mut app, minimap));
        if let (Some(a), Some(b)) = (a, b)
            && a.1 > 0
            && b.1 > 0
        {
            seen = Some((a, b));
            break;
        }
    }
    let ((main_view, main_blades), (map_view, map_blades)) =
        seen.expect("both cameras should draw grass");

    // Each draws with its own radius, density and minimum width.
    let high = GrassSettings::HIGH;
    assert_eq!(
        main_view.radius,
        DistanceLod::for_radius(high.radius).radius
    );
    assert_eq!(map_view.radius, DistanceLod::for_radius(20.0).radius);
    assert_eq!(
        (main_view.density_scale, map_view.density_scale),
        (1.0, 0.5)
    );
    assert_eq!((main_view.min_width_px, map_view.min_width_px), (1.0, 3.0));
    assert!(
        map_blades < main_blades / 4,
        "the minimap draws {map_blades} blades, the main camera {main_blades}"
    );

    // The camera that names nothing follows the resource as it changes; the
    // one that named a setting keeps it.
    {
        let mut settings = app.world_mut().resource_mut::<GrassSettings>();
        settings.min_width_px = 2.0;
        settings.radius = 60.0;
    }
    let (main_view, _) = view(&mut app, main).unwrap();
    let (map_view, _) = view(&mut app, minimap).unwrap();
    assert_eq!(main_view.min_width_px, 2.0);
    assert_eq!(main_view.radius, DistanceLod::for_radius(60.0).radius);
    assert_eq!(map_view.min_width_px, 3.0, "the override wins");
    assert_eq!(map_view.radius, DistanceLod::for_radius(20.0).radius);

    // Removing the component hands the camera back to the resource.
    app.world_mut().entity_mut(minimap).remove::<GrassCamera>();
    let (map_view, _) = view(&mut app, minimap).unwrap();
    assert_eq!(map_view.min_width_px, 2.0);
    assert_eq!(map_view.radius, DistanceLod::for_radius(60.0).radius);
    assert_eq!(map_view.density_scale, 1.0);
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_distance_and_budget_settings_change_what_is_drawn() {
    use crate::GrassSettings;

    let mut app = headless_app();
    let mut start = (ViewUniformData::default(), 0);
    for _ in 0..600 {
        start = view_after(&mut app, 1);
        if start.1 > 0 {
            break;
        }
    }
    let (_, all) = start;
    assert!(all > 10_000, "the test scene should place many blades");
    let lods = blades_per_lod(&mut app);
    let high = GrassSettings::HIGH;

    // The first level of detail starting sooner leaves fewer full-detail
    // blades, and the uniform carries the new distance.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        lod1_start: 0.05,
        ..high
    };
    let (view, _) = view_after(&mut app, 4);
    assert_eq!(view.lod1_start, high.radius * 0.05);
    let sooner = blades_per_lod(&mut app);
    assert!(
        sooner[0] * 2 < lods[0],
        "{} full-detail blades with LOD 1 at 5 m, {} at 20 m",
        sooner[0],
        lods[0]
    );

    // Keeping all of the mid-range blades draws more of them.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        mid_range_density: 1.0,
        ..high
    };
    let (view, denser) = view_after(&mut app, 4);
    assert_eq!(view.lod_density.z, 1.0);
    assert!(
        denser as f32 > all as f32 * 1.2,
        "{denser} blades with the whole mid-range, {all} with half"
    );

    // No thinning at all by distance: nothing is lost before the fade.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        lod2_density: 1.0,
        fade_density: 1.0,
        mid_range_density: 1.0,
        ..high
    };
    let (_, unthinned) = view_after(&mut app, 4);
    assert!(unthinned > denser, "{unthinned} against {denser}");

    // A narrower budget of chunk slices makes buffers of that size and drops
    // the grass that does not fit.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        max_chunk_slices: 32,
        ..high
    };
    let (view, limited) = view_after(&mut app, 6);
    assert_eq!(view.max_visible_chunks, 32, "the buffers were not remade");
    assert!(limited > 0 && limited < all, "{limited} of {all} blades");
    *app.world_mut().resource_mut::<GrassSettings>() = high;
    let (view, again) = view_after(&mut app, 6);
    assert_eq!(view.max_visible_chunks, high.max_chunk_slices);
    assert_eq!(
        again, all,
        "the same grass comes back with the same settings"
    );

    // How far blades turn toward the camera, and how wide thin ones grow, are
    // in the view's limits.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        turn_to_camera: 1.0,
        max_widening: 3.0,
        ..high
    };
    let (view, _) = view_after(&mut app, 4);
    assert_eq!((view.limits.x, view.limits.z), (3.0, 1.0));

    // Distances given out of order are put in order.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        lod1_start: 0.6,
        lod2_start: 0.3,
        fade_start: 0.1,
        ..high
    };
    let (view, _) = view_after(&mut app, 4);
    assert_eq!(view.lod1_start, high.radius * 0.6);
    assert_eq!(
        view.lod2_start,
        high.radius * 0.6,
        "LOD 2 cannot precede LOD 1"
    );
    assert_eq!(view.lod_density.w, 0.6, "nor the fade precede LOD 2");
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_an_orthographic_camera_stores_blades_over_the_extent_the_settings_give() {
    use bevy::camera::{OrthographicProjection, Projection, ScalingMode};

    use crate::GrassSettings;

    // A map camera 600 m above a field 0.5 m high: further down than the
    // 256 m a default view stores, so only a bigger extent places the grass.
    let mut app = headless_app();
    app.insert_resource(GrassSettings {
        orthographic_extent: 1_024.0,
        ..GrassSettings::HIGH
    });
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            320,
            180,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let map = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderTarget::Image(target.into()),
            Camera {
                order: 1,
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: 60.0,
                },
                far: 2_000.0,
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(3.0, 600.0, -4.0).looking_to(Vec3::NEG_Y, Vec3::NEG_Z),
        ))
        .id();

    let mut seen = None;
    for _ in 0..600 {
        app.update();
        if let Some((view, words)) = gpu_words_of(&mut app, Some(map))
            && !words.is_empty()
        {
            seen = Some((view, words));
            break;
        }
    }
    let (view, words) = seen.expect("the map camera should draw grass");
    assert_eq!(view.instance_origin.w, 1_024.0);
    assert_eq!(view.limits.w, 1_024.0, "the vertical range follows");
    // Every blade is where the field is: 0.5 m up, within a step or two of
    // the 16-bit grid (3 cm here).
    for word in &words {
        let y = ((word[1] & 0xffff) as f32 / 65535.0 - 0.5) * 2.0 * view.limits.w
            + view.instance_origin.y;
        assert!((y - 0.5).abs() < 0.1, "a blade at {y} m");
    }
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_the_rows_of_a_blade_are_a_setting_and_the_meshes_follow_it() {
    use crate::GrassSettings;

    let mut app = headless_app();
    let mut start = (ViewUniformData::default(), 0);
    for _ in 0..600 {
        start = view_after(&mut app, 1);
        if start.1 > 0 {
            break;
        }
    }
    let (_, all) = start;
    // A strip of `rows` rows is `2 * rows - 1` triangles: 13, 5 and 1.
    assert_eq!(draw_index_counts(&mut app), [39, 15, 3]);

    // Fewer rows: the meshes are built again, and the same blades are drawn
    // with them.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        blade_rows: [4, 2, 1],
        ..GrassSettings::HIGH
    };
    let (_, same) = view_after(&mut app, 4);
    assert_eq!(draw_index_counts(&mut app), [21, 9, 3]);
    assert_eq!(same, all, "the rows change the mesh, not the placement");

    // More rows than the default.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        blade_rows: [16, 8, 2],
        ..GrassSettings::HIGH
    };
    view_after(&mut app, 4);
    assert_eq!(draw_index_counts(&mut app), [93, 45, 9]);

    // And back.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings::HIGH;
    view_after(&mut app, 4);
    assert_eq!(draw_index_counts(&mut app), [39, 15, 3]);
}
