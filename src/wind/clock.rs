//! The built-in wind's gust scroll: how far the gust texture has moved,
//! without a jump when the wind changes speed or direction.
//!
//! The shaders sample a repeating noise texture scrolled along the wind. How
//! far it has scrolled is the sum of each frame's step, wrapped every
//! [`GUST_WRAP`] repeats of the texture, so it is exact however long the game
//! runs, and stays continuous when the wind's speed or direction changes.
//! Time multiplied by a speed would jump at every change.

use bevy::ecs::resource::Resource;
use bevy::math::{DVec2, Vec2};

use super::{GrassWind, GrassWindFrame};

/// How far the gust offsets run before they wrap, in repeats of the gust
/// texture. The WESL twin is `GUST_WRAP` in `wind.wesl`.
///
/// The shaders turn a second layer of gusts by a matrix in steps of 1/256,
/// so a wrap must move it by whole repeats: a multiple of 256. A wrap at 1
/// would make that layer jump each time the gusts crossed a repeat. A larger
/// one costs precision: an f32 below 256 resolves 1/30 000 of a repeat, under a
/// hundredth of a texel of the 256-texel texture.
pub(super) const GUST_WRAP: f64 = 256.0;

/// What the built-in wind is told about the gusts this frame.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub(super) struct WindClock {
    /// How far the gusts have scrolled, in repeats of the gust texture,
    /// wrapped to `[0, GUST_WRAP)`.
    gust_offset: Vec2,
    /// The same one frame ago, wrapped the same way.
    previous_gust_offset: Vec2,
    /// `gust_offset` unrounded.
    gust: DVec2,
}

impl WindClock {
    /// Moves the gusts `delta` seconds along `wind`.
    pub(super) fn advance(&mut self, delta: f64, wind: &GrassWind) {
        let step = gust_step(wind.direction, wind.gust_speed, wind.gust_scale, delta);
        self.gust = (self.gust + step).rem_euclid(DVec2::splat(GUST_WRAP));
        self.gust_offset = self.gust.as_vec2();
        self.previous_gust_offset = (self.gust - step)
            .rem_euclid(DVec2::splat(GUST_WRAP))
            .as_vec2();
    }

    /// This frame's shader values.
    pub(super) fn frame(&self, wind: &GrassWind) -> GrassWindFrame {
        GrassWindFrame {
            gust_offset: self.gust_offset,
            previous_gust_offset: self.previous_gust_offset,
            ..(*wind).into()
        }
    }
}

/// How far gusts of `gust_scale` metres, moving at `gust_speed` metres per
/// second toward `direction`, scroll in `delta` seconds, in repeats of the
/// texture. Shared by the built-in clock and [`GrassWindFrame::advance`].
pub(super) fn gust_step(direction: Vec2, gust_speed: f32, gust_scale: f32, delta: f64) -> DVec2 {
    let direction = direction.as_dvec2();
    let direction = if direction.length_squared() > 1e-8 {
        direction.normalize()
    } else {
        DVec2::X
    };
    direction * (f64::from(gust_speed) * delta / f64::from(gust_scale.max(0.01)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seconds: f64, step: f64, wind: &GrassWind) -> WindClock {
        let mut clock = WindClock::default();
        let mut elapsed = 0.0;
        while elapsed < seconds {
            elapsed += step;
            clock.advance(step, wind);
        }
        clock
    }

    #[test]
    fn the_gusts_do_not_jump_when_the_time_wraps() {
        // The scroll between two frames is the same from the start to the
        // end of the clock, and equals one frame's step.
        let wind = GrassWind::GUSTY;
        let step = gust_step(wind.direction, wind.gust_speed, wind.gust_scale, 1.0 / 60.0);
        let mut clock = WindClock::default();
        for _ in 0..(3600 * 60 + 120) {
            clock.advance(1.0 / 60.0, &wind);
            // Only differences modulo the wrap count. An f32 below the wrap
            // resolves 3e-5 of a repeat.
            let half = DVec2::splat(GUST_WRAP / 2.0);
            let moved = (clock.gust_offset - clock.previous_gust_offset).as_dvec2();
            let moved = (moved + half).rem_euclid(DVec2::splat(GUST_WRAP)) - half;
            assert!((moved - step).length() < 1e-4, "{moved} vs {step}");
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
        clock.advance(0.25, &fast);
        let moved = (clock.gust_offset - before).length();
        let expected =
            gust_step(fast.direction, fast.gust_speed, fast.gust_scale, 0.25).length() as f32;
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
        let step = gust_step(none.direction, none.gust_speed, none.gust_scale, 1.0);
        assert!(step.x > 0.0 && step.y == 0.0);
    }

    #[test]
    fn the_frame_carries_the_wind_and_the_scroll() {
        let wind = GrassWind {
            sway: 2.0,
            ..GrassWind::STORM
        };
        let clock = run(1.0, 0.25, &wind);
        let frame = clock.frame(&wind);
        assert_eq!(frame.direction, wind.direction);
        assert_eq!(frame.strength, wind.strength);
        assert_eq!(frame.gust_scale, wind.gust_scale);
        assert_eq!(frame.sway, 2.0);
        assert_eq!(frame.gust_offset, clock.gust_offset);
        assert_eq!(frame.previous_gust_offset, clock.previous_gust_offset);
    }
}
