//! bevy_grass::shaders::grass — the main pass: blades lit by Bevy's own PBR
//! functions, so grass receives every light, shadow, fog and tonemapping
//! setting the rest of the scene does.

import bevy_pbr::render::{
    mesh_view_bindings::{view, lights},
    pbr_types::{pbr_input_new, STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT},
    pbr_functions::{apply_pbr_lighting, calculate_view, main_pass_post_lighting_processing},
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
};
import package::shaders::blade::{blade_vertex, fields};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    /// rgb: base colour; a: the style's translucency.
    @location(2) color: vec4<f32>,
    @location(3) occlusion: f32,
}

fn is_orthographic() -> bool {
    return view.clip_from_view[3].w == 1.0;
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    let forward = -view.world_from_view[2].xyz;
    let pixel_scale = 2.0 / (view.clip_from_view[0][0] * view.viewport.z);
    let blade = blade_vertex(
        vertex_index,
        instance_index,
        view.world_position,
        forward,
        is_orthographic(),
        view.clip_from_world,
        pixel_scale,
    );
    let field = fields[blade.instance.field];

    var out: VertexOutput;
    out.world_position = vec4<f32>(blade.world_position, 1.0);
    out.position = view.clip_from_world * out.world_position;
    out.world_normal = blade.normal;
    // Dark roots to bright tips. Brightness varies mostly per clump and a
    // little per blade, and one clump in five is drier, toward straw: patches
    // read as natural where per-blade noise reads as static.
    let clump = blade.instance.clump;
    let shade = mix(0.82, 1.12, clump) * (0.94 + 0.12 * blade.instance.seed);
    let green = mix(field.root_color.rgb, field.tip_color.rgb, pow(blade.t, 0.7)) * shade;
    let straw = vec3<f32>(dot(green, vec3<f32>(0.3, 0.59, 0.11))) * vec3<f32>(1.25, 1.1, 0.6);
    out.color = vec4<f32>(mix(green, straw, 0.4 * step(0.8, fract(clump * 5.3))), field.tip_color.w);
    // Fake ambient occlusion: the surrounding blades shade the roots.
    out.occlusion = mix(1.0 - field.motion.w, 1.0, smoothstep(0.0, 0.6, blade.t));
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    var pbr_input = pbr_input_new();
    pbr_input.material.base_color = vec4<f32>(in.color.rgb, 1.0);
    pbr_input.material.perceptual_roughness = 0.7;
    pbr_input.material.metallic = 0.0;
    pbr_input.material.reflectance = vec3<f32>(0.25);
    // Bevy fogs only materials that ask for it; without this grass stood out,
    // unfogged, in front of everything the mist had faded.
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    pbr_input.diffuse_occlusion = vec3<f32>(in.occlusion);

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
    pbr_input.world_normal = lit_side;
    pbr_input.N = lit_side;
    pbr_input.frag_coord = in.position;
    pbr_input.world_position = in.world_position;
    pbr_input.is_orthographic = is_orthographic();
    pbr_input.V = calculate_view(in.world_position, pbr_input.is_orthographic);
    pbr_input.flags = MESH_FLAGS_SHADOW_RECEIVER_BIT;

    let lit = apply_pbr_lighting(pbr_input);
    let ambient = view.exposure * lights.ambient_color.rgb * in.color.rgb * in.occlusion;
    let through = select(vec3<f32>(1.0), in.color.a * min(in.color.rgb * 2.0, vec3<f32>(1.0)), backlit);
    let color = min(ambient, lit.rgb) + max(lit.rgb - ambient, vec3<f32>(0.0)) * through;
    return main_pass_post_lighting_processing(pbr_input, vec4<f32>(color, lit.a));
}
