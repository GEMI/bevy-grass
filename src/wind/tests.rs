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
