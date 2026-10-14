//! bevy_grass::shaders::far_ground — `FarGrassMaterial`: a `StandardMaterial`
//! whose colour turns to far grass with distance (`far.wesl`). Forward and
//! deferred.

import bevy_pbr::render::{
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    mesh_view_bindings::{view, globals},
};
import package::shaders::far::{FarGrass, grass_far_color};

@if(PREPASS_PIPELINE)
import bevy_pbr::{
    prepass::io::{VertexOutput, FragmentOutput},
    deferred::functions::deferred_output,
};
@else
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> far: FarGrass;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var gust_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var gust_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );
    let ground = pbr_input.material.base_color.rgb;
    pbr_input.material.base_color = vec4<f32>(
        grass_far_color(in.world_position.xyz, view.world_position, ground, far, globals.time, gust_texture, gust_sampler),
        pbr_input.material.base_color.a,
    );

@if(PREPASS_PIPELINE)
    let out = deferred_output(in, pbr_input);
@if(!PREPASS_PIPELINE)
    var out: FragmentOutput;
@if(!PREPASS_PIPELINE)
    out.color = main_pass_post_lighting_processing(pbr_input, apply_pbr_lighting(pbr_input));
    return out;
}
