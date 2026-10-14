//! bevy_grass::shaders::grass — the main pass: blades lit by Bevy's own PBR
//! functions, so grass receives every light, shadow, fog and tonemapping
//! setting the rest of the scene does.

import bevy_pbr::render::{
    mesh_view_bindings::{view, lights},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};
import package::shaders::blade::{blade_ref, blade_vertex, current_eye, fields};
import package::shaders::look::{
    blade_color, blade_occlusion, blade_style, blade_sheen, blade_pbr_input, transmitted, toon,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    /// rgb: base colour; a: the style's translucency.
    @location(2) color: vec4<f32>,
    @location(3) occlusion: f32,
    /// How much sheen shows here (`blade_sheen`).
    @if(GRASS_SHEEN)
    @location(4) sheen: f32,
    /// Toon bands.
    @if(GRASS_TOON)
    @location(5) @interpolate(flat) bands: f32,
}

fn is_orthographic() -> bool {
    return view.clip_from_view[3].w == 1.0;
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
    let field = fields[blade_at.instance.field];

    var out: VertexOutput;
    out.world_position = vec4<f32>(blade.world_position, 1.0);
    out.position = view.clip_from_world * out.world_position;
    out.world_normal = blade.normal;
    out.color = blade_color(blade_at.instance, field, blade.t);
    out.occlusion = blade_occlusion(field, blade.t);
@if(GRASS_SHEEN)
    out.sheen = blade_sheen(blade_style(field).x, blade.t);
@if(GRASS_TOON)
    out.bands = blade_style(field).y;
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    // Blades are two-sided: the back face is lit with the flipped normal.
    let normal = normalize(in.world_normal) * select(-1.0, 1.0, is_front);

    // Translucency, approximated so that it costs no second light or shadow
    // evaluation: a face turned away from the sun is lit as its sunward side
    // (the same shadow lookup, so shade still darkens it), and the direct
    // part of that light is dimmed and tinted by the blade, as light that
    // passed through it. The ambient part is estimated (Bevy's ambient term
    // for a diffuse surface) and kept whole. Branch-free: branches here cost
    // more than the arithmetic.
    let backlit = lights.n_directional_lights > 0u
        && dot(normal, lights.directional_lights[0].direction_to_light) < 0.0;
    let lit_side = select(normal, -normal, backlit);
    var sheen = 0.0;
@if(GRASS_SHEEN)
    sheen = in.sheen;
    let pbr_input = blade_pbr_input(
        in.color.rgb,
        in.occlusion,
        sheen,
        lit_side,
        in.position,
        in.world_position,
        is_orthographic(),
    );

    let lit = apply_pbr_lighting(pbr_input);
    let ambient = view.exposure * lights.ambient_color.rgb * in.color.rgb * in.occlusion;
    let through = select(vec3<f32>(1.0), transmitted(in.color), backlit);
    var color = min(ambient, lit.rgb) + max(lit.rgb - ambient, vec3<f32>(0.0)) * through;
@if(GRASS_TOON)
    // Toon styles round the light to flat bands.
    color = select(color, toon(color, in.color.rgb, in.bands), in.bands > 0.5);
    return main_pass_post_lighting_processing(pbr_input, vec4<f32>(color, lit.a));
}
