//! bevy_grass::shaders::grass_shadow — depth-only blades for directional
//! shadow maps. Group 1 is intentionally empty so the grass bindings sit at
//! group 2 in every pipeline.

import bevy_render::view::View;
import package::shaders::blade::blade_vertex;

@group(0) @binding(0) var<uniform> view: View;

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> @builtin(position) vec4<f32> {
    let forward = -view.world_from_view[2].xyz;
    // Directional lights are orthographic; no minimum width in shadow maps.
    let blade = blade_vertex(
        vertex_index,
        instance_index,
        view.world_position,
        forward,
        true,
        view.clip_from_world,
        0.0,
    );
    return view.clip_from_world * vec4<f32>(blade.world_position, 1.0);
}
