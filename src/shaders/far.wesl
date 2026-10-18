//! bevy_grass::shaders::far — the colour of grass seen from too far away to
//! draw blades, for terrain materials. Beyond the grass radius there are no
//! blades; `grass_far_color` tints the ground toward the grass's colour as
//! the blades thin out, with the same gusts rolling across it, so there is no
//! seam where the blades end.
//!
//! `FarGrassMaterial` (Rust) is a ready-made terrain material that calls it.
//! A custom terrain shader can import it:
//!
//! ```wgsl
//! import bevy_grass::shaders::far::{FarGrass, grass_far_color};
//! ```

import package::shaders::wind::{Wind, wind_gust};

/// Mirrors `FarGrassUniform` in Rust.
struct FarGrass {
    /// rgb: the grass's colour from afar, linear; w: how far the ground
    /// takes it on (0–1).
    color: vec4<f32>,
    /// x: distance where the tint starts; y: where it is full; z: how much
    /// gusts brighten it; w: 1 if `area` applies.
    fade: vec4<f32>,
    /// The rectangle with grass, world XZ: min x, min z, max x, max z. The
    /// tint fades out over `fade.y * 0.02` metres at its edge.
    area: vec4<f32>,
    /// The wind, kept equal to `GrassWindFrame` by `FarGrassPlugin`.
    wind: Wind,
}

/// The ground's colour `ground` at `world_position`, tinted toward the far
/// grass seen from `eye`, with the gusts where `far.wind.gust_offset` has
/// scrolled them (kept equal to the blades' by `FarGrassPlugin`). `gusts` is
/// the grass's gust texture.
fn grass_far_color(
    world_position: vec3<f32>,
    eye: vec3<f32>,
    ground: vec3<f32>,
    far: FarGrass,
    gusts: texture_2d<f32>,
    gust_sampler: sampler,
) -> vec3<f32> {
    var amount = smoothstep(far.fade.x, far.fade.y, distance(world_position, eye)) * far.color.w;
    if far.fade.w > 0.5 {
        let soft = max(far.fade.y * 0.02, 0.01);
        let inside = min(world_position.xz - far.area.xy, far.area.zw - world_position.xz);
        amount *= smoothstep(0.0, soft, min(inside.x, inside.y));
    }
    // The gust texture the blades bend by, scrolled the same way: where a
    // gust presses the grass down its paler, sunlit side shows.
    let gust = wind_gust(far.wind, gusts, gust_sampler, world_position.xz);
    let sheen = 1.0 + far.fade.z * far.wind.strength * (1.4 * gust * gust - 0.35);
    return mix(ground, far.color.rgb * sheen, amount);
}
