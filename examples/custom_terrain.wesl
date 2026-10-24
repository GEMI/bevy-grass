//! The `custom_terrain` example's own terrain shader: a ground lit by one
//! sun, as a game's terrain shader might be, turning to far grass with
//! distance through `bevy_grass::shaders::far::grass_far_color`.

import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_grass::shaders::far::{FarGrass, grass_far_color};

struct Terrain {
    /// rgb: the soil's colour, linear.
    soil: vec4<f32>,
    /// xyz: direction toward the sun.
    sun: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> far: FarGrass;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var gust_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var gust_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var<uniform> terrain: Terrain;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let ground = grass_far_color(
        in.world_position.xyz,
        view.world_position,
        terrain.soil.rgb,
        far,
        gust_texture,
        gust_sampler,
    );
    let light = 0.35 + 0.65 * max(dot(normalize(in.world_normal), terrain.sun.xyz), 0.0);
    return vec4<f32>(ground * light, 1.0);
}
