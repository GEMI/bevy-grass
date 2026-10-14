//! bevy_grass::shaders::grass_shadow — depth-only blades for directional
//! shadow maps. Group 1 is intentionally empty so the grass bindings sit at
//! group 2 in every pipeline.

import bevy_render::view::View;
import package::shaders::blade::{blade_ref, blade_vertex, current_eye};

@group(0) @binding(0) var<uniform> view: View;

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> @builtin(position) vec4<f32> {
    var eye = current_eye(view.world_from_view, view.clip_from_view, view.clip_from_world, 1.0);
    // Directional lights are orthographic; no minimum width in shadow maps.
    eye.is_orthographic = true;
    eye.pixel_scale = 0.0;
    let blade = blade_vertex(blade_ref(vertex_index, instance_index), eye);
    return view.clip_from_world * vec4<f32>(blade.world_position, 1.0);
}
