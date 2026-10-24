use super::*;

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_the_wind_can_be_shaped_beyond_its_strength_and_gusts() {
    use crate::render::resources::ExtractedGrass;

    let mut app = headless_app();
    // A steady wind: no gusts to differ, no sway to tell blades apart.
    app.insert_resource(GrassWind {
        sway: 0.0,
        gust_contrast: 0.0,
        ..GrassWind::BREEZE
    });
    wait_for_grass(&mut app);
    for _ in 0..10 {
        app.update();
    }
    let steady = bend_spread(&mut app);
    assert!(
        steady < 0.003,
        "{steady}: blades bend alike without gusts and sway"
    );

    // Sway alone tells blades apart, and more of it more so.
    app.insert_resource(GrassWind {
        sway: 1.0,
        gust_contrast: 0.0,
        ..GrassWind::BREEZE
    });
    for _ in 0..4 {
        app.update();
    }
    let swaying = bend_spread(&mut app);
    assert!(swaying > 0.01, "{swaying}");
    app.insert_resource(GrassWind {
        sway: 3.0,
        gust_contrast: 0.0,
        ..GrassWind::BREEZE
    });
    for _ in 0..4 {
        app.update();
    }
    let swaying_more = bend_spread(&mut app);
    assert!(
        swaying_more > swaying * 2.0,
        "{swaying_more} against {swaying}"
    );

    // Gusts alone make the waves across the field, and a higher contrast
    // makes them deeper.
    app.insert_resource(GrassWind {
        sway: 0.0,
        gust_contrast: 1.0,
        ..GrassWind::BREEZE
    });
    for _ in 0..4 {
        app.update();
    }
    let gusting = bend_spread(&mut app);
    assert!(gusting > 0.02, "{gusting}");
    app.insert_resource(GrassWind {
        sway: 0.0,
        gust_contrast: 3.0,
        ..GrassWind::BREEZE
    });
    for _ in 0..4 {
        app.update();
    }
    let gusting_more = bend_spread(&mut app);
    assert!(
        gusting_more > gusting * 1.5,
        "{gusting_more} against {gusting}"
    );

    // The speed of the sway and the flutter of the tips are in the frame the
    // shaders read, clamped at zero.
    app.insert_resource(GrassWind {
        sway_speed: 0.5,
        flutter: -3.0,
        ..GrassWind::BREEZE
    });
    for _ in 0..3 {
        app.update();
    }
    let wind = app
        .sub_app(RenderApp)
        .world()
        .resource::<ExtractedGrass>()
        .wind;
    assert_eq!((wind.sway_speed, wind.flutter), (0.5, 0.0));
    assert_eq!((wind.sway, wind.gust_contrast), (1.0, 1.0));
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_a_field_moves_by_its_own_wind_scale() {
    use crate::api::{GrassField, GrassFieldOverrides};

    let mut app = headless_app();
    // A steady storm, so every blade bends alike, and far.
    app.insert_resource(GrassWind {
        sway: 0.0,
        gust_contrast: 0.0,
        ..GrassWind::STORM
    });
    wait_for_grass(&mut app);
    let mean_bend = |app: &mut App| {
        for _ in 0..4 {
            app.update();
        }
        let bends = gpu_bends(app);
        assert!(
            bends.len() > 10_000,
            "the test scene should place many blades"
        );
        bends.iter().map(|(_, bend)| bend.length()).sum::<f32>() / bends.len() as f32
    };
    let full = mean_bend(&mut app);
    assert!(full > 0.08, "{full}: the storm bends the blades");

    let field = app
        .world_mut()
        .query_filtered::<Entity, With<GrassField>>()
        .single(app.world())
        .unwrap();
    for scale in [0.5f32, 0.0, 2.0] {
        app.world_mut()
            .entity_mut(field)
            .insert(GrassFieldOverrides {
                wind_scale: scale,
                ..default()
            });
        let scaled = mean_bend(&mut app);
        // Bends are stored in 12 bits, steps of 1/2048.
        assert!(
            (scaled - full * scale).abs() < 0.002,
            "wind scale {scale}: {scaled} against {full} at 1"
        );
    }
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_the_gusts_do_not_repeat_every_gust_scale() {
    // One repeat of the gust texture is `gust_scale` metres. With one layer
    // of gusts, blades that far apart bent alike, and a camera high above
    // saw the field in squares of that size.
    const SCALE: f32 = 6.0;
    const CELLS_PER_SCALE: i32 = 12;
    let mut app = headless_app();
    // No sway, so a blade's bend is the gusts at its root.
    app.insert_resource(GrassWind {
        gust_scale: SCALE,
        sway: 0.0,
        ..GrassWind::BREEZE
    });
    wait_for_grass(&mut app);
    for _ in 0..4 {
        app.update();
    }
    // The mean bend of the blades in each cell of a grid.
    let mut cells = HashMap::<(i32, i32), (f32, u32)>::new();
    for (root, bend) in gpu_bends(&mut app) {
        let cell = (root * CELLS_PER_SCALE as f32 / SCALE).floor().as_ivec2();
        let entry = cells.entry((cell.x, cell.y)).or_default();
        entry.0 += bend.length();
        entry.1 += 1;
    }
    let mean = |cell: &(i32, i32)| {
        cells
            .get(cell)
            .filter(|(_, count)| *count >= 4)
            .map(|(sum, count)| sum / *count as f32)
    };
    // Cells one gust scale apart along X and along Z.
    let pairs: Vec<(f32, f32)> = cells
        .keys()
        .flat_map(|&(x, z)| {
            [(x + CELLS_PER_SCALE, z), (x, z + CELLS_PER_SCALE)].map(|other| ((x, z), other))
        })
        .filter_map(|(cell, other)| Some((mean(&cell)?, mean(&other)?)))
        .collect();
    assert!(pairs.len() > 200, "{} pairs of cells", pairs.len());
    let n = pairs.len() as f32;
    let (ma, mb) = pairs
        .iter()
        .fold((0.0, 0.0), |(a, b), (x, y)| (a + x / n, b + y / n));
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (a, b) in &pairs {
        cov += (a - ma) * (b - mb);
        va += (a - ma) * (a - ma);
        vb += (b - mb) * (b - mb);
    }
    let correlation = cov / (va * vb).sqrt();
    assert!(
        correlation < 0.75,
        "{correlation}: blades a gust scale apart bend alike"
    );
}
