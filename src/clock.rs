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
mod tests;
