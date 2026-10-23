//! bevy_grass::shaders::bevy_bridge — the one grass shader that imports
//! Bevy's (`bevy_pbr`, `bevy_render`), used by every grass draw: the main
//! pass, the prepass and deferred G-buffer, and the shadow pass.
//!
//! Bevy's shader modules change between releases: `PbrInput`'s fields, the
//! view's, the lighting entry points. A change there compiles with
//! `cargo build` and fails only when the pipeline is created, drawing no
//! grass. Here, the rest of the grass shaders see Bevy through a few small
//! functions in grass's own terms, so a Bevy port edits this file alone.
//! (`far_ground.wesl` is the exception: it is a `StandardMaterial`
//! extension, a Bevy material by nature, like `FarGrassMaterial` in Rust.)
//!
//! Which view this is depends on the pass: Bevy's mesh view in the main pass
//! and the prepasses, the light's view in the shadow pass
//! (`GRASS_SHADOW_PASS`, set by `GrassShadowPipeline`).

@if(GRASS_SHADOW_PASS)
import bevy_render::view::View;
@if(!GRASS_SHADOW_PASS)
import bevy_pbr::render::mesh_view_bindings::view;
@if(!GRASS_SHADOW_PASS)
import bevy_pbr::render::{
    pbr_types::{PbrInput, pbr_input_new, STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT},
    pbr_functions::calculate_view,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
};
@if(!PREPASS_PIPELINE && !GRASS_SHADOW_PASS)
import bevy_pbr::render::{
    mesh_view_bindings::lights,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};
@if(MOTION_VECTOR_PREPASS)
import bevy_pbr::prepass::bindings::previous_view_uniforms;
@if(DEFERRED_PREPASS)
import bevy_pbr::deferred::functions::deferred_gbuffer_from_pbr_input;

/// The directional light's view, in the shadow pass. Group 1 is empty so
/// the grass group sits at 2 in every grass pipeline.
@if(GRASS_SHADOW_PASS)
@group(0) @binding(0) var<uniform> view: View;

// --- The view ------------------------------------------------------------

fn view_world_from_view() -> mat4x4<f32> {
    return view.world_from_view;
}

fn view_clip_from_view() -> mat4x4<f32> {
    return view.clip_from_view;
}

fn view_clip_from_world() -> mat4x4<f32> {
    return view.clip_from_world;
}

/// The view's width in pixels.
fn view_viewport_width() -> f32 {
    return view.viewport.z;
}

fn view_is_orthographic() -> bool {
    return view.clip_from_view[3].w == 1.0;
}

// --- The surface Bevy lights -----------------------------------------------

/// A blade's surface, in grass's terms: what `grass_pbr_input` hands to
/// Bevy's lighting.
struct GrassSurface {
    base_color: vec3<f32>,
    perceptual_roughness: f32,
    reflectance: f32,
    /// Ambient occlusion, 0 to 1.
    occlusion: f32,
    /// The normal of the side being lit, world space.
    normal: vec3<f32>,
    frag_coord: vec4<f32>,
    world_position: vec4<f32>,
    is_orthographic: bool,
}

/// `surface` as Bevy's `PbrInput`: a matte, fogged, shadow-receiving
/// dielectric.
@if(!GRASS_SHADOW_PASS)
fn grass_pbr_input(surface: GrassSurface) -> PbrInput {
    var pbr_input = pbr_input_new();
    pbr_input.material.base_color = vec4<f32>(surface.base_color, 1.0);
    pbr_input.material.perceptual_roughness = surface.perceptual_roughness;
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(surface.reflectance);
    // Bevy fogs only materials that ask for it; without this grass stood out,
    // unfogged, in front of everything the mist had faded.
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    pbr_input.diffuse_occlusion = vec3<f32>(surface.occlusion);
    pbr_input.world_normal = surface.normal;
    pbr_input.N = surface.normal;
    pbr_input.frag_coord = surface.frag_coord;
    pbr_input.world_position = surface.world_position;
    pbr_input.is_orthographic = surface.is_orthographic;
    pbr_input.V = calculate_view(surface.world_position, surface.is_orthographic);
    pbr_input.flags = MESH_FLAGS_SHADOW_RECEIVER_BIT;
    return pbr_input;
}

// --- The main pass -------------------------------------------------------

/// Whether a surface facing `normal` faces away from the first directional
/// light, the one grass treats as the sun. False without one.
@if(!PREPASS_PIPELINE && !GRASS_SHADOW_PASS)
fn faces_away_from_sun(normal: vec3<f32>) -> bool {
    return lights.n_directional_lights > 0u
        && dot(normal, lights.directional_lights[0].direction_to_light) < 0.0;
}

/// The ambient light, exposed: what a white, unoccluded diffuse surface
/// receives from it.
@if(!PREPASS_PIPELINE && !GRASS_SHADOW_PASS)
fn ambient_light() -> vec3<f32> {
    return view.exposure * lights.ambient_color.rgb;
}

/// Every light, shadow and environment map Bevy has for this view, applied
/// to `pbr_input`.
@if(!PREPASS_PIPELINE && !GRASS_SHADOW_PASS)
fn grass_lit(pbr_input: PbrInput) -> vec4<f32> {
    return apply_pbr_lighting(pbr_input);
}

/// Fog, tonemapping and the rest of Bevy's view settings, applied to a lit
/// `color`.
@if(!PREPASS_PIPELINE && !GRASS_SHADOW_PASS)
fn grass_post(pbr_input: PbrInput, color: vec4<f32>) -> vec4<f32> {
    return main_pass_post_lighting_processing(pbr_input, color);
}

// --- The prepass and the G-buffer ------------------------------------------

/// `normal` as Bevy's normal prepass stores it.
fn prepass_normal(normal: vec3<f32>) -> vec4<f32> {
    return vec4<f32>(normal * 0.5 + vec3<f32>(0.5), 1.0);
}

@if(MOTION_VECTOR_PREPASS)
fn previous_view_from_world() -> mat4x4<f32> {
    return previous_view_uniforms.view_from_world;
}

@if(MOTION_VECTOR_PREPASS)
fn previous_clip_from_view() -> mat4x4<f32> {
    return previous_view_uniforms.clip_from_view;
}

@if(MOTION_VECTOR_PREPASS)
fn previous_clip_from_world() -> mat4x4<f32> {
    return previous_view_uniforms.clip_from_world;
}

/// The motion vector of a point at `world_position` now and at
/// `previous_world_position` a frame ago, as Bevy's prepass stores it: a
/// UV-space offset, y down, without the jitter.
@if(MOTION_VECTOR_PREPASS)
fn prepass_motion_vector(world_position: vec4<f32>, previous_world_position: vec4<f32>) -> vec2<f32> {
    let clip = view.unjittered_clip_from_world * world_position;
    let previous_clip = previous_view_uniforms.unjittered_clip_from_world * previous_world_position;
    return (clip.xy / clip.w - previous_clip.xy / previous_clip.w) * vec2<f32>(0.5, -0.5);
}

/// `pbr_input` packed into Bevy's G-buffer.
@if(DEFERRED_PREPASS)
fn grass_gbuffer(pbr_input: PbrInput) -> vec4<u32> {
    return deferred_gbuffer_from_pbr_input(pbr_input);
}

/// The deferred lighting pass that lights the G-buffer grass writes: Bevy's
/// own, the one `StandardMaterial` uses.
const DEFERRED_LIGHTING_PASS_ID: u32 = 1u;
