use super::*;

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_trampled_grass_lies_down_stays_put_and_recovers() {
    let mut app = headless_app();
    // Without wind an untouched blade stands nearly straight.
    app.insert_resource(GrassWind {
        strength: 0.0,
        ..GrassWind::CALM
    });
    let at = Vec2::new(4.0, 12.0);
    let interactor = app
        .world_mut()
        .spawn((
            GrassInteractor {
                radius: 1.0,
                strength: 1.0,
                ..Default::default()
            },
            Transform::from_xyz(at.x, 0.5, at.y),
        ))
        .id();
    // Bends of the blades whose roots are within `reach` of the interactor.
    let near = |app: &mut App, reach: f32| -> Vec<(Vec2, Vec2)> {
        gpu_bends(app)
            .into_iter()
            .filter(|(root, _)| root.distance(at) < reach)
            .collect()
    };
    let mut ready = false;
    for _ in 0..600 {
        app.update();
        if near(&mut app, 0.5).len() > 20 {
            ready = true;
            break;
        }
    }
    assert!(
        ready,
        "no grass appeared around the interactor in 600 frames"
    );

    let flattened = near(&mut app, 0.5);
    for (root, bend) in &flattened {
        assert!(
            bend.length() > 0.8,
            "a blade at {root} under the interactor bends only {bend}"
        );
        let away = *root - at;
        if away.length() > 0.2 {
            assert!(
                bend.dot(away.normalize()) > 0.6,
                "a blade at {root} leans {bend}, not away"
            );
        }
    }
    let untouched: Vec<f32> = gpu_bends(&mut app)
        .into_iter()
        .filter(|(root, _)| root.distance(at) > 3.0)
        .map(|(_, bend)| bend.length())
        .collect();
    assert!(
        untouched.iter().all(|&b| b < 0.1),
        "blades far away bend too"
    );

    // The interactor leaves and the camera moves a non-whole number of cells:
    // the trail stays where it was made, a little weaker.
    app.world_mut().despawn(interactor);
    let mut cameras = app.world_mut().query::<(&mut Transform, &Camera3d)>();
    for (mut transform, _) in cameras.iter_mut(app.world_mut()) {
        transform.translation += Vec3::new(6.3, 0.0, -2.9);
    }
    for _ in 0..6 {
        app.update();
    }
    let trail = near(&mut app, 0.5);
    assert!(!trail.is_empty());
    assert!(
        trail.iter().all(|(_, bend)| bend.length() > 0.6),
        "the trail moved with the camera or faded too fast"
    );

    // After 2 s of game time the grass stands again.
    for _ in 0..120 {
        app.update();
    }
    let recovered = near(&mut app, 0.5);
    assert!(!recovered.is_empty());
    assert!(
        recovered.iter().all(|(_, bend)| bend.length() < 0.1),
        "flattened grass did not recover"
    );
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_an_interactor_with_a_height_limit_only_tramples_near_the_ground() {
    let mut app = headless_app();
    app.insert_resource(GrassWind {
        strength: 0.0,
        ..GrassWind::CALM
    });
    let at = Vec2::new(4.0, 12.0);
    // A bird: five metres up, where the grass is a few tens of centimetres.
    let bird = app
        .world_mut()
        .spawn((
            GrassInteractor {
                radius: 1.0,
                max_height: Some(1.0),
                ..Default::default()
            },
            Transform::from_xyz(at.x, 5.0, at.y),
        ))
        .id();
    let bends_near = |app: &mut App| -> Vec<f32> {
        gpu_bends(app)
            .into_iter()
            .filter(|(root, _)| root.distance(at) < 0.5)
            .map(|(_, bend)| bend.length())
            .collect()
    };
    let mut seen = 0;
    for _ in 0..600 {
        app.update();
        seen = bends_near(&mut app).len();
        if seen > 20 {
            break;
        }
    }
    assert!(seen > 20, "no grass appeared under the bird in 600 frames");
    for _ in 0..10 {
        app.update();
    }
    assert!(
        bends_near(&mut app).iter().all(|&bend| bend < 0.1),
        "a bird flying over the grass flattened it"
    );

    // It lands.
    app.world_mut()
        .get_mut::<Transform>(bird)
        .unwrap()
        .translation
        .y = 0.2;
    for _ in 0..10 {
        app.update();
    }
    let landed = bends_near(&mut app);
    assert!(!landed.is_empty());
    assert!(
        landed.iter().all(|&bend| bend > 0.8),
        "a bird on the ground did not flatten the grass"
    );
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_trample_settings_decide_how_far_and_how_long_grass_lies_down() {
    use crate::GrassSettings;

    // A 4 m interactor in still air, and the bends of blades 2.6 to 3.4 m
    // from its centre, which a 2 m cap leaves alone.
    let at = Vec2::new(4.0, 12.0);
    let ring = |app: &mut App| -> Vec<f32> {
        gpu_bends(app)
            .into_iter()
            .filter(|(root, _)| (2.6..3.4).contains(&root.distance(at)))
            .map(|(_, bend)| bend.length())
            .collect()
    };
    let spawn = |app: &mut App| {
        app.insert_resource(GrassWind {
            strength: 0.0,
            ..GrassWind::CALM
        });
        app.world_mut()
            .spawn((
                GrassInteractor {
                    radius: 4.0,
                    strength: 1.0,
                    ..Default::default()
                },
                Transform::from_xyz(at.x, 0.5, at.y),
            ))
            .id()
    };

    let mut capped = headless_app();
    spawn(&mut capped);
    wait_for_grass(&mut capped);
    for _ in 0..10 {
        capped.update();
    }
    let bends = ring(&mut capped);
    assert!(bends.len() > 20, "{} blades in the ring", bends.len());
    assert!(
        bends.iter().all(|&b| b < 0.1),
        "a 4 m interactor reached past the 2 m default"
    );

    let mut wide = headless_app();
    wide.insert_resource(GrassSettings {
        max_interactor_radius: 6.0,
        ..GrassSettings::HIGH
    });
    let interactor = spawn(&mut wide);
    wait_for_grass(&mut wide);
    for _ in 0..10 {
        wide.update();
    }
    let bends = ring(&mut wide);
    assert!(bends.len() > 20, "{} blades in the ring", bends.len());
    assert!(
        bends.iter().filter(|&&b| b > 0.2).count() * 2 > bends.len(),
        "most of the ring should lie down: {bends:?}"
    );
    // The view was told the cap, and the cells of its window.
    let (view, _) = gpu_instances(&mut wide).expect("a grass view");
    assert_eq!(view.limits.y, 6.0);
    assert_eq!(view.trample_step.x, 0.25);

    // Slower recovery: two seconds after the interactor leaves, the grass is
    // still down, where at the default 1.5 s it would have stood up again.
    wide.world_mut().despawn(interactor);
    *wide.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        max_interactor_radius: 6.0,
        trample_recovery: 12.0,
        ..GrassSettings::HIGH
    };
    for _ in 0..120 {
        wide.update();
    }
    let (view, _) = gpu_instances(&mut wide).expect("a grass view");
    assert!((view.trample_step.y - (1.0 / 60.0) / 12.0).abs() < 1e-5);
    let slow = gpu_bends(&mut wide)
        .into_iter()
        .filter(|(root, _)| root.distance(at) < 1.5)
        .map(|(_, bend)| bend.length())
        .collect::<Vec<_>>();
    assert!(!slow.is_empty());
    assert!(
        slow.iter().all(|&b| b > 0.5),
        "grass stood up too soon: {slow:?}"
    );
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_trample_cells_of_another_size_make_another_window() {
    use crate::GrassSettings;

    let mut app = headless_app();
    wait_for_grass(&mut app);
    let (default_view, _) = gpu_instances(&mut app).expect("a grass view");
    assert_eq!(default_view.trample_step.x, 0.25);
    let camera = default_view.camera_position;
    let camera = Vec2::new(camera.x, camera.z);
    // The 64 m window of the default is centred on the camera.
    let origin = Vec2::new(
        default_view.trample_origin.x as f32,
        default_view.trample_origin.y as f32,
    ) * 0.25;
    assert!((origin + Vec2::splat(32.0) - camera).abs().max_element() <= 0.5);

    app.insert_resource(GrassSettings {
        trample_cell_size: 1.0,
        ..GrassSettings::HIGH
    });
    for _ in 0..4 {
        app.update();
    }
    let (view, _) = gpu_instances(&mut app).expect("a grass view");
    assert_eq!(view.trample_step.x, 1.0);
    // 256 cells of a metre: a 256 m window, centred on the camera.
    let origin = Vec2::new(view.trample_origin.x as f32, view.trample_origin.y as f32);
    assert!((origin + Vec2::splat(128.0) - camera).abs().max_element() <= 1.0);
}
