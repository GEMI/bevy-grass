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
        // The texture repeats, so offsets a whole repeat apart are equal.
        let apart = |a: Vec2, b: Vec2| {
            let d = a - b;
            (d - d.round()).length()
        };
        assert!(apart(frame.gust_offset, built_in.gust_offset) < 1e-4);
        assert!(apart(frame.previous_gust_offset, built_in.previous_gust_offset) < 1e-4);
    }
    // The offsets stay wrapped to one repeat of the texture.
    assert!(frame.gust_offset.cmpge(Vec2::ZERO).all() && frame.gust_offset.cmplt(Vec2::ONE).all());

    // A still wind keeps the gusts where they are, and both offsets agree.
    frame.advance(0.5, 0.0);
    assert_eq!(frame.gust_offset, frame.previous_gust_offset);
}
