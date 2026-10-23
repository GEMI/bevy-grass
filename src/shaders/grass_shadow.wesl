//! bevy_grass::shaders::grass_shadow — depth-only blades for directional
//! shadow maps. The light's view is group 0 (declared in `bevy_bridge` under
//! `GRASS_SHADOW_PASS`); group 1 is intentionally empty so the grass bindings
//! sit at group 2 in every pipeline.

import package::shaders::bevy_bridge::{view_world_from_view, view_clip_from_view, view_clip_from_world};
import package::shaders::blade::{blade_ref, blade_vertex, current_eye};

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> @builtin(position) vec4<f32> {
    var eye = current_eye(view_world_from_view(), view_clip_from_view(), view_clip_from_world(), 1.0);
    // Directional lights are orthographic; no minimum width in shadow maps.
    eye.is_orthographic = true;
    eye.pixel_scale = 0.0;
    let blade = blade_vertex(blade_ref(vertex_index, instance_index), eye);
    return view_clip_from_world() * vec4<f32>(blade.world_position, 1.0);
}
