//! The wind that moves the grass: the settings, the per-frame values the
//! shaders read, and the built-in model that produces them.
//!
//! The grass renderer never reads [`GrassWind`] directly. It reads one
//! [`GrassWindFrame`] a frame, which is the seam between a wind model and the
//! blades:
//!
//! - [`GrassWindPlugin`] (part of [`GrassPlugin`](crate::GrassPlugin) by
//!   default) samples [`GrassWind`] into [`GrassWindFrame`] every frame.
//! - A game with its own wind leaves that plugin out, with
//!   `GrassPlugin { wind: false, ..default() }`, and writes
//!   [`GrassWindFrame`] itself, in [`GrassWindSystems::Sample`]. A weather
//!   library can drive the grass that way without the built-in gust model.
//!
//! The built-in model is self-contained: `wind/clock.rs` owns its scrolling
//! gust offsets, and `shaders/wind.wesl` owns the shader maths. Replacing it with
//! another model means replacing this module and writing the same frame.

mod clock;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::reflect::ReflectResource;
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::{Res, ResMut};
use bevy::math::{Dir2, Vec2};
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;
use bevy::time::Time;

use clock::WindClock;

/// The wind, shared by every field.
///
/// The presets are constants, so a wind is changed like a style, by struct
/// update: `GrassWind { strength: 0.8, ..GrassWind::GUSTY }`.
///
/// `strength` and the gusts decide how far the grass leans; the last four
/// settings shape how it moves, and are 1 in every preset (the wind as it
/// was designed). `GrassWind { sway: 2.0, ..GrassWind::BREEZE }` is a field
/// that shivers more, `flutter: 0.0` one whose tips hold still.
///
/// This is the built-in model's input. The grass renderer reads
/// [`GrassWindFrame`] instead, so a game can replace the model without using
/// this resource.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct GrassWind {
    /// Direction the wind blows toward, in the XZ plane (X, Z). Any length:
    /// it is normalized on use, and (0, 0) blows along +X. To set it from a
    /// `Dir2`, use [`GrassWind::toward`].
    pub direction: Vec2,
    /// Overall strength, 0 (still) to 1 (storm).
    pub strength: f32,
    /// Size of a gust, in metres.
    pub gust_scale: f32,
    /// How fast gusts travel across the field, in metres per second.
    pub gust_speed: f32,
    /// How far each blade sways on its own, as a multiple of the default; 0
    /// (blades move only with the gusts) and up. The sway is what keeps
    /// neighbouring blades from moving in lockstep.
    pub sway: f32,
    /// How fast blades sway, as a multiple of the default (stiff blades
    /// sway faster than soft ones), 0 and up. Slow it for heavy wheat, speed
    /// it up for reeds.
    pub sway_speed: f32,
    /// How much the tips of full-detail blades flutter, as a multiple of the
    /// default, 0 (no flutter) and up. Flutter grows with `strength`.
    pub flutter: f32,
    /// How much stronger the gusts are than the wind between them, as a
    /// multiple of the default, 0 and up. At 0 the wind is steady and the
    /// grass leans the same everywhere; higher, the waves that roll across the
    /// field are deeper.
    pub gust_contrast: f32,
}

impl GrassWind {
    /// The same wind, blowing toward `direction` (X and Z of the world).
    #[must_use]
    pub fn toward(self, direction: Dir2) -> Self {
        Self {
            direction: direction.as_vec2(),
            ..self
        }
    }

    /// Barely moving, a gentle sway.
    pub const CALM: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 0.1,
        gust_scale: 40.0,
        gust_speed: 2.0,
        sway: 1.0,
        sway_speed: 1.0,
        flutter: 1.0,
        gust_contrast: 1.0,
    };
    /// Visible rolling waves. The default.
    pub const BREEZE: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 0.35,
        gust_scale: 25.0,
        gust_speed: 4.0,
        sway: 1.0,
        sway_speed: 1.0,
        flutter: 1.0,
        gust_contrast: 1.0,
    };
    /// Fast, uneven gusts.
    pub const GUSTY: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 0.6,
        gust_scale: 15.0,
        gust_speed: 7.0,
        sway: 1.0,
        sway_speed: 1.0,
        flutter: 1.0,
        gust_contrast: 1.0,
    };
    /// Heavy bending and fast flutter.
    pub const STORM: Self = Self {
        direction: Vec2::new(1.0, 0.3),
        strength: 1.0,
        gust_scale: 8.0,
        gust_speed: 12.0,
        sway: 1.0,
        sway_speed: 1.0,
        flutter: 1.0,
        gust_contrast: 1.0,
    };
}

impl Default for GrassWind {
    fn default() -> Self {
        Self::BREEZE
    }
}

/// One frame of wind: the values the grass shaders move blades by.
///
/// This is the seam between the wind and the grass. It is plain data, not
/// the GPU layout: the renderer copies it into its own uniform each frame, so
/// the shaders can change without changing this type. The built-in
/// [`GrassWindPlugin`] fills it from [`GrassWind`] every frame; a game using
/// another wind model turns that plugin off (`GrassPlugin { wind: false,
/// ..default() }`) and writes this resource itself.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// # #[derive(Resource)] struct Weather { gust: f32, direction: Vec2 }
/// fn sample_weather(
///     weather: Res<Weather>,
///     mut wind: ResMut<GrassWindFrame>,
/// ) {
///     *wind = GrassWindFrame {
///         direction: weather.direction,
///         strength: weather.gust,
///         ..GrassWind::BREEZE.into()
///     };
/// }
/// ```
///
/// A custom source updates this in [`GrassWindSystems::Sample`], once a
/// frame, so the far-grass material and the blades see the same wind.
///
/// `gust_offset` and `previous_gust_offset` are how far the gust texture has
/// scrolled, in repeats, now and a frame ago. Motion vectors compare the two,
/// so a custom source must advance them (or leave both at zero for a fixed
/// gust pattern). The built-in model accumulates them in `wind/clock.rs`,
/// which keeps them continuous when the wind changes speed or direction.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct GrassWindFrame {
    /// Direction the wind blows toward, in world XZ. Any length: it is
    /// normalized on use, and (0, 0) blows along +X.
    pub direction: Vec2,
    /// Overall strength, 0 (still) and up.
    pub strength: f32,
    /// Size of a gust, in metres.
    pub gust_scale: f32,
    /// How far the gusts have scrolled, in repeats of the gust texture,
    /// wrapped to `[0, 1)`.
    pub gust_offset: Vec2,
    /// The same one frame ago, for motion vectors.
    pub previous_gust_offset: Vec2,
    /// How far each blade sways on its own, 0 and up.
    pub sway: f32,
    /// How fast blades sway, 0 and up.
    pub sway_speed: f32,
    /// How much the tips of full-detail blades flutter, 0 and up.
    pub flutter: f32,
    /// How much stronger the gusts are than the wind between them, 0 and up.
    pub gust_contrast: f32,
}

impl From<GrassWind> for GrassWindFrame {
    /// The wind as a frame, at rest: the gusts have not scrolled yet.
    fn from(wind: GrassWind) -> Self {
        Self {
            direction: wind.direction,
            strength: wind.strength,
            gust_scale: wind.gust_scale,
            gust_offset: Vec2::ZERO,
            previous_gust_offset: Vec2::ZERO,
            sway: wind.sway,
            sway_speed: wind.sway_speed,
            flutter: wind.flutter,
            gust_contrast: wind.gust_contrast,
        }
    }
}

impl Default for GrassWindFrame {
    fn default() -> Self {
        GrassWind::default().into()
    }
}

/// The wind systems, in `Update`.
///
/// [`Sample`](Self::Sample) is where the wind becomes a [`GrassWindFrame`]:
/// the built-in [`GrassWindPlugin`] puts its own system there, and a custom
/// wind plugin should put its system there too, so the far-grass material and
/// the blades read the same frame.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GrassWindSystems {
    /// Samples this frame's wind into [`GrassWindFrame`].
    Sample,
}

/// Adds the built-in wind: [`GrassWind`], its scrolling gusts, and a system
/// that writes [`GrassWindFrame`] every frame. Part of
/// [`GrassPlugin`](crate::GrassPlugin) unless it is built with `wind: false`.
///
/// Leave it out to replace the model:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(GrassPlugins.set(GrassPlugin {
///         wind: false,
///         ..default()
///     }))
///     // Your own wind: write `GrassWindFrame` in `GrassWindSystems::Sample`.
///     .add_systems(Update, sample_weather.in_set(GrassWindSystems::Sample))
///     .run();
/// # fn sample_weather() {}
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct GrassWindPlugin;

impl Plugin for GrassWindPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrassWind>()
            .init_resource::<GrassWindFrame>()
            .init_resource::<WindClock>()
            .add_systems(Update, sample_wind.in_set(GrassWindSystems::Sample));
    }
}

/// Turns the wind settings and the gust clock into this frame's shader values.
fn sample_wind(
    time: Res<Time>,
    wind: Res<GrassWind>,
    mut clock: ResMut<WindClock>,
    mut frame: ResMut<GrassWindFrame>,
) {
    clock.advance(time.delta().as_secs_f64(), &wind);
    *frame = clock.frame(&wind);
}

#[cfg(test)]
mod tests;
