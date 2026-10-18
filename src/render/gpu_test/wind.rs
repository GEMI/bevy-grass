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
    let frame = app
        .sub_app(RenderApp)
        .world()
        .resource::<ExtractedGrass>()
        .frame;
    assert_eq!((frame.sway_speed, frame.flutter), (0.5, 0.0));
    assert_eq!((frame.sway, frame.gust_contrast), (1.0, 1.0));
}
