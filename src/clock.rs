//! The grass's clock: time as the shaders need it, without a visible seam
//! when it wraps.
//!
//! A GPU works in `f32`, whose precision falls as a clock grows, so the
//! shaders get a wrapped time and, for the gusts, an offset accumulated here
//! in `f64`. Neither jumps when the clock wraps:
//!
//! - **Sway and flutter** are sines of `time * frequency`. The shader rounds
//!   every frequency to a whole number of cycles per [`TIME_PERIOD`], so the
//!   phase at the end of one period is the phase at the start of the next.
//!   (The rounding is 0.1% of a frequency and unseen.)
//! - **Gusts** scroll the noise texture along the wind. How far it has
//!   scrolled is the sum of each frame's step, kept as a fraction of the
//!   texture (which repeats), so it is exact however long the game runs, and
//!   it stays continuous when the wind's speed or direction changes. Time
//!   multiplied by a speed would jump at the wrap and at every change.

use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::math::{DVec2, Vec2};
use bevy::time::Time;

use crate::api::GrassWind;

/// Seconds after which the shaders' time starts again. The twin of
/// `TIME_PERIOD` in `common.wesl`.
pub(crate) const TIME_PERIOD: f64 = 3600.0;

/// What the shaders are told about time this frame.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct GrassClock {
    /// Seconds since the app started, wrapped to `[0, TIME_PERIOD)`.
    pub time: f32,
    /// The same one frame ago. May be negative just after a wrap, which the
    /// shaders' periodic functions do not mind.
    pub previous_time: f32,
    /// How far the gusts have scrolled, in repeats of the gust texture,
    /// wrapped to `[0, 1)`.
    pub gust_offset: Vec2,
    /// The same one frame ago, wrapped the same way.
    pub previous_gust_offset: Vec2,
    /// `gust_offset` unrounded.
    gust: DVec2,
}

impl GrassClock {
    /// Moves the clock to `elapsed` seconds since the start, `delta` seconds
    /// after the last tick, blowing along `wind`.
    pub(crate) fn advance(&mut self, elapsed: f64, delta: f64, wind: &GrassWind) {
        self.time = elapsed.rem_euclid(TIME_PERIOD) as f32;
        self.previous_time = (elapsed - delta).rem_euclid(TIME_PERIOD) as f32;
        let step = gust_step(wind, delta);
        self.gust = (self.gust + step).rem_euclid(DVec2::ONE);
        self.gust_offset = self.gust.as_vec2();
        self.previous_gust_offset = (self.gust - step).rem_euclid(DVec2::ONE).as_vec2();
    }
}

/// How far the gusts scroll in `delta` seconds, in repeats of the texture:
/// the wind's speed along its direction, over the size of a gust.
fn gust_step(wind: &GrassWind, delta: f64) -> DVec2 {
    let direction = wind.direction.as_dvec2();
    let direction = if direction.length_squared() > 1e-8 {
        direction.normalize()
    } else {
        DVec2::X
    };
    direction * (f64::from(wind.gust_speed) * delta / f64::from(wind.gust_scale.max(0.01)))
}

/// Twin of `periodic` in `common.wesl`: `frequency` (radians per second)
/// rounded to a whole number of cycles per [`TIME_PERIOD`].
#[cfg(test)]
fn periodic(frequency: f32) -> f32 {
    let step = std::f32::consts::TAU / TIME_PERIOD as f32;
    (frequency / step).round() * step
}

/// Ticks the clock once a frame, with the game's time, so that pausing the
/// game pauses the wind.
pub(crate) fn tick(time: Res<Time>, wind: Res<GrassWind>, mut clock: ResMut<GrassClock>) {
    clock.advance(
        time.elapsed().as_secs_f64(),
        time.delta().as_secs_f64(),
        &wind,
    );
}

#[cfg(test)]
mod tests {
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
        let shader = include_str!("shaders/common.wesl");
        assert!(
            shader.contains("const TIME_PERIOD: f32 = 3600.0;"),
            "common.wesl's TIME_PERIOD must equal clock::TIME_PERIOD"
        );
        assert_eq!(TIME_PERIOD, 3600.0);
    }
}
