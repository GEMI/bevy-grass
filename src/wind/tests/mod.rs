use super::*;

#[test]
fn a_frame_is_the_wind_without_its_speed_or_a_scroll() {
    let frame = GrassWindFrame::from(GrassWind::STORM);
    assert_eq!(frame.direction, GrassWind::STORM.direction);
    assert_eq!(frame.strength, GrassWind::STORM.strength);
    assert_eq!(frame.gust_scale, GrassWind::STORM.gust_scale);
    assert_eq!(frame.sway, GrassWind::STORM.sway);
    assert_eq!(frame.sway_speed, GrassWind::STORM.sway_speed);
    assert_eq!(frame.flutter, GrassWind::STORM.flutter);
    assert_eq!(frame.gust_contrast, GrassWind::STORM.gust_contrast);
    // Speed only decides how the built-in model scrolls the gusts; it is not
    // something a shader reads.
    assert_eq!(frame.gust_offset, Vec2::ZERO);
    assert_eq!(frame.previous_gust_offset, Vec2::ZERO);
    assert_eq!(
        GrassWindFrame::default(),
        GrassWindFrame::from(GrassWind::BREEZE)
    );
}

#[test]
fn advancing_a_frame_scrolls_the_gusts_as_the_built_in_wind_does() {
    // A custom source that calls `advance` with the built-in wind's speed
    // gets the built-in wind's gusts.
    let wind = GrassWind::GUSTY;
    let mut clock = WindClock::default();
    let mut frame = GrassWindFrame::from(wind);
    for _ in 0..600 {
        clock.advance(1.0 / 60.0, &wind);
        frame.advance(1.0 / 60.0, wind.gust_speed);
        let built_in = clock.frame(&wind);
        // Offsets a whole wrap apart are equal.
        let apart = |a: Vec2, b: Vec2| {
            let d = (a - b) / 256.0;
            (d - d.round()).length() * 256.0
        };
        assert!(apart(frame.gust_offset, built_in.gust_offset) < 1e-4);
        assert!(apart(frame.previous_gust_offset, built_in.previous_gust_offset) < 1e-4);
    }
    // The offsets run past one repeat of the texture, which the shaders'
    // turned layer of gusts needs, and stay wrapped to 256.
    assert!(frame.gust_offset.x > 1.0, "{}", frame.gust_offset);
    assert!(
        frame.gust_offset.cmpge(Vec2::ZERO).all()
            && frame.gust_offset.cmplt(Vec2::splat(256.0)).all()
    );

    // A still wind keeps the gusts where they are, and both offsets agree.
    frame.advance(0.5, 0.0);
    assert_eq!(frame.gust_offset, frame.previous_gust_offset);
}

#[test]
fn the_plugin_samples_the_wind_into_the_frame() {
    use std::time::Duration;

    use bevy::app::App;

    let wind = GrassWind {
        gust_speed: 2.0,
        ..GrassWind::BREEZE
    };
    let mut app = App::new();
    app.add_plugins(GrassWindPlugin).insert_resource(wind);
    // Insert a clock with a known delta instead of relying on `TimePlugin`:
    // its first update only primes the clock and reports a zero delta.
    let mut time: Time = Time::default();
    time.advance_by(Duration::from_secs_f64(0.5));
    app.insert_resource(time);

    app.update();
    let frame = *app.world().resource::<GrassWindFrame>();
    let expected = clock::gust_step(wind.direction, wind.gust_speed, wind.gust_scale, 0.5);
    // The plugin's system carried the wind into the frame and scrolled the
    // gusts by one step.
    assert_eq!(frame.direction, wind.direction);
    assert_eq!(frame.strength, wind.strength);
    assert_eq!(frame.gust_scale, wind.gust_scale);
    assert_eq!(frame.sway, wind.sway);
    assert_eq!(frame.gust_contrast, wind.gust_contrast);
    assert!(
        (frame.gust_offset.as_dvec2() - expected).length() < 1e-5,
        "{} vs {expected}",
        frame.gust_offset
    );

    // Another update scrolls again and keeps the previous offset.
    let first = frame.gust_offset;
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f64(0.5));
    app.update();
    let frame = *app.world().resource::<GrassWindFrame>();
    assert!((frame.previous_gust_offset - first).length() < 1e-5);
}

#[test]
fn toward_normalizes_direction_and_preserves_the_rest_of_the_wind() {
    let original = GrassWind {
        direction: Vec2::new(3.0, -4.0),
        strength: 0.7,
        gust_scale: 12.0,
        gust_speed: 3.0,
        sway: 1.5,
        sway_speed: 0.8,
        flutter: 0.6,
        gust_contrast: 1.2,
    };
    let direction = Dir2::from_angle(0.7);
    let mut expected = original;
    expected.direction = direction.as_vec2();
    let result = original.toward(direction);
    assert_eq!(result, expected);
    assert!((result.direction.length() - 1.0).abs() <= 1e-6);
}

#[test]
fn every_preset_stays_in_valid_ranges() {
    for wind in [
        GrassWind::CALM,
        GrassWind::BREEZE,
        GrassWind::GUSTY,
        GrassWind::STORM,
    ] {
        assert!((0.0..=1.0).contains(&wind.strength), "{wind:?}");
        assert!(wind.gust_scale >= 0.0, "{wind:?}");
        assert!(wind.gust_speed >= 0.0, "{wind:?}");
        assert!(wind.sway >= 0.0, "{wind:?}");
        assert!(wind.sway_speed >= 0.0, "{wind:?}");
        assert!(wind.flutter >= 0.0, "{wind:?}");
        assert!(wind.gust_contrast >= 0.0, "{wind:?}");
        assert!(wind.direction.is_finite(), "{wind:?}");
    }
}
