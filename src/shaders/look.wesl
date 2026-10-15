//! bevy_grass::shaders::look — a blade's colour and material, shared by the
//! forward pass and the deferred G-buffer, so the two look the same.
//!
//! The optional terms are compiled in only when some field uses them, so a
//! game that uses none pays nothing for them:
//!
//! * `GRASS_GROUND_TINT`: roots blend toward the ground's colour.
//! * `GRASS_SHEEN`: rough specular on the upper half of the blade.
//! * `GRASS_TOON`: flat toon bands (forward only).
//! * `GRASS_SHAPE`: heads, in their own colour.
//! * `GRASS_VARIATION`: colour patches, dry tips and dead blades.

import bevy_pbr::render::{
    pbr_types::{PbrInput, pbr_input_new, STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT},
    pbr_functions::calculate_view,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
};
import package::shaders::common::{Field, Instance, instance_random, straw_of};
import package::shaders::blade::{noise_texture, noise_sampler};

/// Base colour and translucency of a blade at height `t` (0 root, 1 tip).
fn blade_color(instance: Instance, field: Field, t: f32) -> vec4<f32> {
    // Dark roots to bright tips. Brightness varies mostly per clump and a
    // little per blade, and one clump in five is drier, toward straw: patches
    // read as natural where per-blade noise reads as static.
    let clump = instance.clump;
    let shade = mix(0.82, 1.12, clump) * (0.94 + 0.12 * instance.seed);
    let green = mix(field.root_color.rgb, field.tip_color.rgb, pow(t, 0.7)) * shade;
    let straw = vec3<f32>(dot(green, vec3<f32>(0.3, 0.59, 0.11))) * vec3<f32>(1.25, 1.1, 0.6);
    var color = mix(green, straw, 0.4 * step(0.8, fract(clump * 5.3)));
@if(GRASS_VARIATION) {
    // Patches several metres across, where the blades' colour drifts toward
    // the patch colour: the noise, turned so its repeat is not along the
    // world's axes.
    let at = instance.root.xz / (field.variation.x * 8.0);
    let uv = vec2<f32>(at.x * 0.83 - at.y * 0.56, at.x * 0.56 + at.y * 0.83);
    let noise = textureSampleLevel(noise_texture, noise_sampler, uv, 0.0).r;
    let in_patch = field.patches.w * smoothstep(0.3, 0.7, noise);
    color *= mix(vec3<f32>(1.0), field.patches.rgb, in_patch);
    // Dry tips: the upper part of the blade, some blades more than others.
    let dry = field.variation.y * smoothstep(0.45, 1.0, t) * (0.4 + 1.2 * instance_random(instance, 0x7d1u));
    color = mix(color, straw_of(color), min(dry, 1.0));
}
@if(GRASS_SHAPE) {
    let head = field.form.w;
    color = select(color, field.head.rgb, head > 0.0 && t >= 1.0 - 0.5 * head - 1e-4);
}
@if(GRASS_VARIATION) {
    // Dead blades, head and all.
    let dead = instance_random(instance, 0x7d2u) < field.variation.z;
    color = select(color, straw_of(color) * 0.85, dead);
}
@if(GRASS_GROUND_TINT)
    // The lowest third of the blade blends toward the ground's colour, so the
    // field sits in the terrain instead of on it.
    color = mix(color, field.ground.rgb, field.ground.w * (1.0 - smoothstep(0.0, 0.35, t)));
    return vec4<f32>(color, field.tip_color.w);
}

/// Fake ambient occlusion at height `t`: the surrounding blades shade the
/// roots.
fn blade_occlusion(field: Field, t: f32) -> f32 {
    return mix(1.0 - field.motion.w, 1.0, smoothstep(0.0, 0.6, t));
}

/// The style's optional terms: x specular, y toon bands (0 for physically
/// based shading).
fn blade_style(field: Field) -> vec2<f32> {
    return field.look.xy;
}

/// How much of a blade's sheen shows at height `t`, for a style whose
/// specular is `specular`: only the upper half catches the light. Computed
/// per vertex: passing the height and the style to the fragment instead cost
/// 20–30% more of the sheen's time.
fn blade_sheen(specular: f32, t: f32) -> f32 {
    return specular * smoothstep(0.4, 0.8, t);
}

/// The blade's surface for Bevy's lighting. `normal` is the side being lit;
/// `sheen` comes from `blade_sheen` (0 without `GRASS_SHEEN`).
fn blade_pbr_input(
    color: vec3<f32>,
    occlusion: f32,
    sheen: f32,
    normal: vec3<f32>,
    frag_coord: vec4<f32>,
    world_position: vec4<f32>,
    is_orthographic: bool,
) -> PbrInput {
    var pbr_input = pbr_input_new();
    pbr_input.material.base_color = vec4<f32>(color, 1.0);
    // With sheen, the upper blade is glossier and reflects more, so wind
    // waves show as a moving sheen. Without, the whole blade is a matte leaf.
    pbr_input.material.perceptual_roughness = 0.7 - 0.25 * sheen;
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(0.25 + 0.45 * sheen);
    // Bevy fogs only materials that ask for it; without this grass stood out,
    // unfogged, in front of everything the mist had faded.
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    pbr_input.diffuse_occlusion = vec3<f32>(occlusion);
    pbr_input.world_normal = normal;
    pbr_input.N = normal;
    pbr_input.frag_coord = frag_coord;
    pbr_input.world_position = world_position;
    pbr_input.is_orthographic = is_orthographic;
    pbr_input.V = calculate_view(world_position, is_orthographic);
    pbr_input.flags = MESH_FLAGS_SHADOW_RECEIVER_BIT;
    return pbr_input;
}

/// How much of the direct light a backlit face gets: the light that passed
/// through the blade, dimmed by its translucency and tinted by its colour.
fn transmitted(color: vec4<f32>) -> vec3<f32> {
    return color.a * min(color.rgb * 2.0, vec3<f32>(1.0));
}

/// Flat toon bands: the light a surface receives, relative to its colour,
/// rounded to `bands` levels.
fn toon(lit: vec3<f32>, base: vec3<f32>, bands: f32) -> vec3<f32> {
    let weights = vec3<f32>(0.2126, 0.7152, 0.0722);
    let level = dot(lit, weights) / max(dot(base, weights), 1e-4);
    return base * (floor(level * bands + 0.5) / bands);
}
