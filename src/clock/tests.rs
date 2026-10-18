use super::*;

fn run(seconds: f64, step: f64, wind: &GrassWind) -> GrassClock {
    let mut clock = GrassClock::default();
    let mut elapsed = 0.0;
    while elapsed < seconds {
        elapsed += step;
        clock.advance(elapsed, step, wind);
    }
    clock
}

#[test]
fn time_wraps_into_one_period() {
    let clock = run(3600.0 + 10.0, 0.5, &GrassWind::BREEZE);
    assert!((clock.time - 10.0).abs() < 1e-3, "{}", clock.time);
    assert!((clock.previous_time - 9.5).abs() < 1e-3);
    // Just after a wrap the previous frame is just before the end.
    let mut clock = GrassClock::default();
    clock.advance(3600.1, 0.2, &GrassWind::BREEZE);
    assert!(clock.previous_time > 3599.0, "{}", clock.previous_time);
}

#[test]
fn the_gusts_do_not_jump_when_the_time_wraps() {
    // The scroll between two frames is the same just before the wrap as
    // just after, and equals one frame's step.
    let wind = GrassWind::GUSTY;
    let step = gust_step(&wind, 1.0 / 60.0);
    let mut clock = GrassClock::default();
    for frame in 0..(3600 * 60 + 120) {
        let elapsed = f64::from(frame) / 60.0;
        clock.advance(elapsed, 1.0 / 60.0, &wind);
        if (3599.9..3600.1).contains(&elapsed) {
            // The texture repeats, so only differences modulo 1 count.
            let moved = (clock.gust_offset - clock.previous_gust_offset).as_dvec2();
            let moved = (moved + DVec2::splat(0.5)).rem_euclid(DVec2::ONE) - DVec2::splat(0.5);
            assert!(
                (moved - step).length() < 1e-5,
                "{elapsed}: {moved} vs {step}"
            );
        }
    }
}

#[test]
fn changing_the_wind_changes_the_pace_not_the_place() {
    // Doubling the speed doubles the step from then on, and the gusts do
    // not jump back to where they would be had the wind always been so.
    let slow = GrassWind {
        gust_speed: 2.0,
        ..GrassWind::BREEZE
    };
    let fast = GrassWind {
        gust_speed: 4.0,
        ..GrassWind::BREEZE
    };
    let mut clock = run(100.0, 0.25, &slow);
    let before = clock.gust_offset;
    clock.advance(100.25, 0.25, &fast);
    let moved = (clock.gust_offset - before).length();
    let expected = gust_step(&fast, 0.25).length() as f32;
    assert!((moved - expected).abs() < 1e-5, "{moved} vs {expected}");
}

#[test]
fn a_still_clock_does_not_scroll_and_a_zero_direction_blows_along_x() {
    let still = GrassWind {
        gust_speed: 0.0,
        ..GrassWind::BREEZE
    };
    assert_eq!(run(50.0, 0.1, &still).gust_offset, Vec2::ZERO);
    let none = GrassWind {
        direction: Vec2::ZERO,
        ..GrassWind::BREEZE
    };
    let step = gust_step(&none, 1.0);
    assert!(step.x > 0.0 && step.y == 0.0);
}

#[test]
fn sway_and_flutter_are_continuous_where_the_time_wraps() {
    // Every frequency the shaders use: sway from 1.1 × 0.85 to 2.4 × 1.15
    // radians a second, and flutter at 11.
    let period = TIME_PERIOD;
    let frequencies = (0..=200)
        .map(|i| 0.935 + (2.76 - 0.935) * i as f32 / 200.0)
        .chain([11.0]);
    for frequency in frequencies {
        let f = f64::from(periodic(frequency));
        assert!(
            (f - f64::from(frequency)).abs() / f64::from(frequency) < 0.001,
            "{frequency} became {f}"
        );
        // The phase at the end of a period equals the phase at its start,
        // for any blade's offset.
        let end = (f * period).rem_euclid(std::f64::consts::TAU);
        let wrapped = end.min(std::f64::consts::TAU - end);
        assert!(
            wrapped < 0.01,
            "{frequency}: {wrapped} rad apart at the wrap"
        );
    }
    // Without the rounding, the jump is large.
    let jump = (2.0_f64 * period).rem_euclid(std::f64::consts::TAU);
    assert!(jump.min(std::f64::consts::TAU - jump) > 0.1);
}

#[test]
fn the_shader_and_the_clock_agree_on_the_period() {
    let shader = include_str!("../shaders/common.wesl");
    assert!(
        shader.contains("const TIME_PERIOD: f32 = 3600.0;"),
        "common.wesl's TIME_PERIOD must equal clock::TIME_PERIOD"
    );
    assert_eq!(TIME_PERIOD, 3600.0);
}
