//! bevy_grass::shaders::wind — the model that bends the grass.
//!
//! The grass renderer reads one `Wind` uniform per frame. The built-in
//! `GrassWindPlugin` fills it from `GrassWind`; a game with another wind
//! model fills it itself and leaves the built-in plugin out.
//!
//! There are no bindings here: the gust texture is passed in, so the blade
//! generation pass and the draw passes can use their own bindings.

import package::shaders::common::{TAU, periodic};

/// One frame of wind. Mirrors `WindUniform` in `src/render/resources.rs`.
struct Wind {
    /// Direction the wind blows toward, in world XZ. Normalized on use.
    direction: vec2<f32>,
    /// Overall strength, 0 (still) and up.
    strength: f32,
    /// Size of a gust, in metres.
    gust_scale: f32,
    /// How far the gusts have scrolled, in repeats of the gust texture.
    gust_offset: vec2<f32>,
    /// The same a frame ago, for motion vectors.
    previous_gust_offset: vec2<f32>,
    /// How far blades sway on their own, how fast, and how much tips flutter.
    sway: f32,
    sway_speed: f32,
    flutter: f32,
    /// How much stronger gusts are than the wind between them.
    gust_contrast: f32,
}

/// The direction the wind blows toward, normalized; +X for a zero direction.
fn wind_direction(wind: Wind) -> vec2<f32> {
    let raw = wind.direction;
    return select(vec2<f32>(1.0, 0.0), normalize(raw), dot(raw, raw) > 1e-8);
}

/// How far the gust offsets run before they wrap, in repeats of the gust
/// texture. Twin of `wind::clock::GUST_WRAP`.
const GUST_WRAP: f32 = 256.0;

/// The second layer of gusts: the texture turned 36.8 degrees and 1.17 times
/// as large. One texture repeat is `gust_scale` metres, so one layer alone
/// tiles the ground in a grid of that size, which a camera high above shows
/// as squares. A turned, larger copy of it repeats on another grid, and the
/// sum does not repeat. Its entries are whole multiples of `1 / GUST_WRAP`,
/// so when the offset wraps the layer moves by whole repeats and does not
/// jump; the two grids meet again only `GUST_WRAP` repeats away.
const GUST_TURN = mat2x2<f32>(175.0, 131.0, -131.0, 175.0) * (1.0 / GUST_WRAP);

/// The gusts at world XZ `at`: two layers of the scrolling gust texture,
/// 0 to 1.
fn wind_gust(wind: Wind, gusts: texture_2d<f32>, gust_sampler: sampler, at: vec2<f32>) -> f32 {
    let uv = at / max(wind.gust_scale, 0.01) - wind.gust_offset;
    let along = textureSampleLevel(gusts, gust_sampler, uv, 0.0).r;
    let turned = textureSampleLevel(gusts, gust_sampler, GUST_TURN * uv, 0.0).r;
    // The mean of two independent samples varies 1/sqrt(2) as much as one;
    // scaled back, the gusts are as deep as a single layer's.
    return clamp(0.5 + (along + turned - 1.0) * 0.70710678, 0.0, 1.0);
}

/// Tip offset from the wind, as a fraction of blade height, in world XZ.
///
/// Gusts, the scrolling noise texture sampled at the root, give the waves
/// that roll across a field; sway, a per-blade sine, keeps neighbours from
/// moving in lockstep. Flutter is added per vertex by `wind_flutter`.
fn wind_bend(
    wind: Wind,
    root: vec2<f32>,
    seed: f32,
    stiffness: f32,
    time: f32,
    previous: bool,
    gusts: texture_2d<f32>,
    gust_sampler: sampler,
) -> vec2<f32> {
    var current = wind;
    current.gust_offset = select(wind.gust_offset, wind.previous_gust_offset, previous);
    let direction = wind_direction(wind);
    let strength = wind.strength;
    let gust = wind_gust(current, gusts, gust_sampler, root);
    let softness = 1.0 - 0.75 * stiffness;
    let frequency = periodic(mix(1.1, 2.4, stiffness) * (0.85 + 0.3 * seed) * wind.sway_speed);
    let sway = sin(time * frequency + seed * TAU);
    let across = vec2<f32>(-direction.y, direction.x);
    let push = strength * (0.15 + 1.4 * wind.gust_contrast * gust * gust);
    let wobble = (direction * 0.5 + across * 0.5) * sway * (0.04 + 0.14 * strength) * wind.sway;
    return clamp((direction * push + wobble) * softness, vec2<f32>(-1.0), vec2<f32>(1.0));
}

/// The tip shiver of a full-detail blade, in metres. `t` is 0 at the root and
/// 1 at the tip; `height` is the blade's height.
fn wind_flutter(wind: Wind, seed: f32, t: f32, height: f32, time: f32) -> f32 {
    let shiver = sin(time * periodic(11.0) + seed * 37.0 + t * 4.0);
    return shiver * 0.012 * wind.strength * wind.flutter * t * t * height;
}
