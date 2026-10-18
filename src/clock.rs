//! The grass's clock: time as the shaders need it, without a visible seam
//! when it wraps.
//!
//! A GPU works in `f32`, whose precision falls as a clock grows, so the
//! shaders get a wrapped time. The wrap is invisible for the wind:
//!
//! - **Sway and flutter** are sines of `time * frequency`. The shader rounds
//!   every frequency to a whole number of cycles per [`TIME_PERIOD`], so the
//!   phase at the end of one period is the phase at the start of the next.
//!   (The rounding is 0.1% of a frequency and unseen.)
//! - **Gusts** scroll the noise texture along the wind. That offset belongs to
//!   the built-in wind model (`wind/clock.rs`), not to this clock, so another
//!   wind model can keep its own time.

use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::time::Time;

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
}

impl GrassClock {
    /// Moves the clock to `elapsed` seconds since the start, `delta` seconds
    /// after the last tick.
    pub(crate) fn advance(&mut self, elapsed: f64, delta: f64) {
        self.time = elapsed.rem_euclid(TIME_PERIOD) as f32;
        self.previous_time = (elapsed - delta).rem_euclid(TIME_PERIOD) as f32;
    }
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
pub(crate) fn tick(time: Res<Time>, mut clock: ResMut<GrassClock>) {
    clock.advance(time.elapsed().as_secs_f64(), time.delta().as_secs_f64());
}

#[cfg(test)]
mod tests;
