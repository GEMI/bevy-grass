use super::*;

fn run(seconds: f64, step: f64) -> GrassClock {
    let mut clock = GrassClock::default();
    let mut elapsed = 0.0;
    while elapsed < seconds {
        elapsed += step;
        clock.advance(elapsed, step);
    }
    clock
}

#[test]
fn time_wraps_into_one_period() {
    let clock = run(3600.0 + 10.0, 0.5);
    assert!((clock.time - 10.0).abs() < 1e-3, "{}", clock.time);
    assert!((clock.previous_time - 9.5).abs() < 1e-3);
    // Just after a wrap the previous frame is just before the end.
    let mut clock = GrassClock::default();
    clock.advance(3600.1, 0.2);
    assert!(clock.previous_time > 3599.0, "{}", clock.previous_time);
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
    let shader = include_str!("../../shaders/common.wesl");
    assert!(
        shader.contains("const TIME_PERIOD: f32 = 3600.0;"),
        "common.wesl's TIME_PERIOD must equal clock::TIME_PERIOD"
    );
    assert_eq!(TIME_PERIOD, 3600.0);
}
