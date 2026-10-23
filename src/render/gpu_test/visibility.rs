use super::*;

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_draws_a_field_only_where_visibility_and_render_layers_allow() {
    use bevy::camera::visibility::RenderLayers;

    let mut app = headless_app();
    // Shaders compile asynchronously; wait until a frame has produced grass.
    let mut drawn = 0;
    for _ in 0..600 {
        drawn = blades_after(&mut app, 1);
        if drawn > 0 {
            break;
        }
    }
    assert!(drawn > 10_000, "the test scene should place many blades");

    let world = app.world_mut();
    let camera = world
        .query_filtered::<Entity, With<Camera3d>>()
        .single(world)
        .unwrap();
    let field = world
        .query_filtered::<Entity, With<GrassField>>()
        .single(world)
        .unwrap();

    // A camera on another layer sees no field on layer 0, as for meshes.
    app.world_mut()
        .entity_mut(camera)
        .insert(RenderLayers::layer(1));
    assert_eq!(blades_after(&mut app, 4), 0, "the camera is on layer 1");

    // The field joins it; the field may be on several layers.
    app.world_mut()
        .entity_mut(field)
        .insert(RenderLayers::from_layers(&[1, 2]));
    assert!(blades_after(&mut app, 4) > 10_000, "they share layer 1");

    // Hidden wins over layers, and showing it again needs no new grass.
    app.world_mut().entity_mut(field).insert(Visibility::Hidden);
    assert_eq!(blades_after(&mut app, 4), 0, "the field is hidden");
    app.world_mut()
        .entity_mut(field)
        .insert(Visibility::Inherited);
    assert!(blades_after(&mut app, 4) > 10_000, "the field is back");

    // A hidden parent hides the field (a streamed-out level, say).
    let level = app.world_mut().spawn(Visibility::Visible).id();
    app.world_mut().entity_mut(field).insert(ChildOf(level));
    assert!(blades_after(&mut app, 4) > 10_000, "the level is visible");
    app.world_mut().entity_mut(level).insert(Visibility::Hidden);
    assert_eq!(blades_after(&mut app, 4), 0, "the level is hidden");
    app.world_mut()
        .entity_mut(level)
        .insert(Visibility::Visible);
    assert!(blades_after(&mut app, 4) > 10_000, "the level is back");

    // Without a layer of its own the camera is on layer 0 again, where the
    // field no longer is.
    app.world_mut().entity_mut(camera).remove::<RenderLayers>();
    assert_eq!(
        blades_after(&mut app, 4),
        0,
        "the camera is back on layer 0"
    );
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_camera_components_reach_the_render_world_and_leave_it() {
    use bevy::render::sync_world::MainEntity;

    use crate::api::{GrassCamera, NoGrass};

    let mut app = headless_app();
    let camera = app
        .world_mut()
        .query_filtered::<Entity, With<Camera3d>>()
        .single(app.world())
        .unwrap();
    let in_render_world = |app: &mut App| {
        let world = app.sub_app_mut(RenderApp).world_mut();
        world
            .query::<(&MainEntity, Has<NoGrass>, Option<&GrassCamera>)>()
            .iter(world)
            .find(|(main, ..)| main.id() == camera)
            .map(|(_, no_grass, grass_camera)| (no_grass, grass_camera.copied()))
            .expect("the camera has a render entity")
    };
    app.update();
    assert_eq!(in_render_world(&mut app), (false, None));

    let grass_camera = GrassCamera {
        radius: Some(12.0),
        ..default()
    };
    app.world_mut()
        .entity_mut(camera)
        .insert((NoGrass, grass_camera));
    app.update();
    assert_eq!(in_render_world(&mut app), (true, Some(grass_camera)));

    app.world_mut()
        .entity_mut(camera)
        .remove::<(NoGrass, GrassCamera)>();
    app.update();
    assert_eq!(in_render_world(&mut app), (false, None));
}
