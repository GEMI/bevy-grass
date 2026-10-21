//! bevy_grass::shaders::grass_prepass — grass in Bevy's prepass and in its
//! deferred G-buffer: depth always; normals, motion vectors and the G-buffer
//! when the view asks for them. Group 0 is Bevy's prepass view group, 1 is
//! its empty group and 2 is the grass group, as in the other grass draws.
//!
//! Motion vectors are exact: the blade is built a second time as it stood
//! last frame, with last frame's bend, clock and camera.

import bevy_pbr::render::mesh_view_bindings::view;
import package::shaders::blade::{blade_ref, blade_vertex, current_eye, previous_eye, fields, grass_view};

@if(MOTION_VECTOR_PREPASS)
import bevy_pbr::prepass::bindings::previous_view_uniforms;

@if(DEFERRED_PREPASS)
import package::shaders::look::{blade_look, look_terms, unpack_look, blade_pbr_input, transmitted};
@if(DEFERRED_PREPASS)
import bevy_pbr::deferred::functions::deferred_gbuffer_from_pbr_input;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    @location(1) world_normal: vec3<f32>,
    @if(MOTION_VECTOR_PREPASS)
    @location(2) previous_world_position: vec4<f32>,
    /// The blade's `Look`, packed by `look.wesl`.
    @if(DEFERRED_PREPASS)
    @location(3) look_color: vec4<f32>,
    @if(DEFERRED_PREPASS)
    @location(4) look_terms: vec4<f32>,
}

@if(PREPASS_FRAGMENT)
struct FragmentOutput {
    @if(NORMAL_PREPASS)
    @location(0) normal: vec4<f32>,
    @if(MOTION_VECTOR_PREPASS)
    @location(1) motion_vector: vec2<f32>,
    @if(DEFERRED_PREPASS)
    @location(2) deferred: vec4<u32>,
    @if(DEFERRED_PREPASS)
    @location(3) deferred_lighting_pass_id: u32,
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let blade_at = blade_ref(vertex_index, instance_index);
    let blade = blade_vertex(
        blade_at,
        current_eye(view.world_from_view, view.clip_from_view, view.clip_from_world, view.viewport.z),
    );
    var out: VertexOutput;
    out.world_position = vec4<f32>(blade.world_position, 1.0);
    out.position = view.clip_from_world * out.world_position;

@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS)
    out.world_normal = blade.normal;

@if(MOTION_VECTOR_PREPASS) {
    let previous = blade_vertex(
        blade_at,
        previous_eye(
            previous_view_uniforms.view_from_world,
            previous_view_uniforms.clip_from_view,
            previous_view_uniforms.clip_from_world,
            view.viewport.z,
        ),
    );
    out.previous_world_position = vec4<f32>(previous.world_position, 1.0);
}

@if(DEFERRED_PREPASS) {
    let look = blade_look(blade_at.instance, fields[blade_at.instance.field], blade.t);
    out.look_color = look.color;
    out.look_terms = look_terms(look);
}
    return out;
}

@if(PREPASS_FRAGMENT)
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var out: FragmentOutput;

@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS) {
    // Blades are two-sided: the back face's normal is flipped.
    let normal = normalize(in.world_normal) * select(-1.0, 1.0, is_front);
@if(NORMAL_PREPASS)
    out.normal = vec4<f32>(normal * 0.5 + vec3<f32>(0.5), 1.0);

@if(DEFERRED_PREPASS) {
    // Translucency, as near the forward pass as a G-buffer allows: a face
    // turned away from the sun is stored as its sunward side, with its colour
    // dimmed and tinted by the light that passes through it. The forward pass
    // keeps such a face's ambient light whole; here it is dimmed too.
    let sun = grass_view.sun;
    let backlit = sun.w > 0.5 && dot(normal, sun.xyz) < 0.0;
    let lit_side = select(normal, -normal, backlit);
    var look = unpack_look(in.look_color, in.look_terms);
    look.color = vec4<f32>(look.color.rgb * select(vec3<f32>(1.0), transmitted(look.color), backlit), look.color.a);
    let pbr_input = blade_pbr_input(
        look,
        lit_side,
        in.position,
        in.world_position,
        view.clip_from_view[3].w == 1.0,
    );
    out.deferred = deferred_gbuffer_from_pbr_input(pbr_input);
    // Bevy's own lighting pass, the one `StandardMaterial` uses.
    out.deferred_lighting_pass_id = 1u;
}
}

@if(MOTION_VECTOR_PREPASS) {
    let clip = view.unjittered_clip_from_world * in.world_position;
    let previous_clip = previous_view_uniforms.unjittered_clip_from_world * in.previous_world_position;
    // As Bevy's prepass: a UV-space offset, y down.
    out.motion_vector = (clip.xy / clip.w - previous_clip.xy / previous_clip.w) * vec2<f32>(0.5, -0.5);
}
    return out;
}
