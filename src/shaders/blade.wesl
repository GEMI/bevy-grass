//! bevy_grass::shaders::blade — what both the main and the shadow pass need to
//! turn one 16-byte instance and a vertex index into a point on a blade.
//! Bindings are group 2 in every grass draw pipeline.

import package::shaders::common::{
    Frame, View, Field, Instance, TAU, WIDTH_RANGE, LOD_VERTEX_STRIDE, unpack, blade_curve, lod_rows,
};

@group(2) @binding(0) var<uniform> frame: Frame;
/// The camera whose blades these are, even when a light view draws them.
@group(2) @binding(1) var<uniform> grass_view: View;
@group(2) @binding(2) var<storage, read> instances: array<vec4<u32>>;
@group(2) @binding(3) var<storage, read> fields: array<Field>;

/// A vertex of a blade.
struct BladeVertex {
    world_position: vec3<f32>,
    normal: vec3<f32>,
    /// 0 at the root, 1 at the tip.
    t: f32,
    instance: Instance,
}

/// Builds vertex `vertex_index` of instance `instance_index`.
///
/// The draw for bucket `b` drawing LOD `l` passes
/// `base_vertex = (3b + l) * LOD_VERTEX_STRIDE`, which is how both are known
/// here without `first_instance`.
///
/// `eye_position` and `eye_forward` describe the camera the blade turns toward
/// when edge-on. `pixel_scale` is `2 / (clip_from_view[0][0] * viewport width)`,
/// which times a point's clip-space `w` gives the metres one pixel covers there;
/// pass 0 to skip the minimum-width rule (shadow passes).
fn blade_vertex(
    vertex_index: u32,
    instance_index: u32,
    eye_position: vec3<f32>,
    eye_forward: vec3<f32>,
    is_orthographic: bool,
    clip_from_world: mat4x4<f32>,
    pixel_scale: f32,
) -> BladeVertex {
    let code = vertex_index / LOD_VERTEX_STRIDE;
    let bucket = code / 3u;
    let lod = code % 3u;
    let local = vertex_index % LOD_VERTEX_STRIDE;
    let instance = unpack(instances[grass_view.bucket_base[bucket] + instance_index], grass_view.instance_origin);
    let field = fields[instance.field];
    let height = instance.height * field.shape.y;
    let root_width = instance.width * WIDTH_RANGE * field.shape.z;

    let rows = lod_rows(lod);
    let is_tip = local >= 2u * rows;
    let row = select(local / 2u, rows, is_tip);
    let t = f32(row) / f32(rows);
    let side = select(f32(local % 2u) - 0.5, 0.0, is_tip);

    // The blade's flat side faces `facing`. As it turns edge-on to the eye it
    // is rotated part of the way toward it, so a field seen side-on does not
    // thin out to lines.
    var to_eye = select(eye_position.xz - instance.root.xz, -eye_forward.xz, is_orthographic);
    to_eye = select(vec2<f32>(0.0, 1.0), normalize(to_eye), dot(to_eye, to_eye) > 1e-8);
    let yaw = instance.facing * TAU;
    var face = vec2<f32>(sin(yaw), cos(yaw));
    let facing_eye = dot(face, to_eye);
    let edge_on = 1.0 - abs(facing_eye);
    face = normalize(mix(face, to_eye * select(-1.0, 1.0, facing_eye >= 0.0), edge_on * 0.4));
    let across = vec3<f32>(face.y, 0.0, -face.x);

    let curve = blade_curve(height, face * field.motion.x + instance.bend);
    let u = 1.0 - t;
    var point = curve[0] * (u * u) + curve[1] * (2.0 * u * t) + curve[2] * (t * t);
    let tangent = normalize((curve[1] - curve[0]) * (2.0 * u) + (curve[2] - curve[1]) * (2.0 * t) + vec3<f32>(0.0, 1e-5, 0.0));

    // Flutter: a small, fast shiver near the tip of full-detail blades.
    if lod == 0u {
        let flutter = sin(frame.time * 11.0 + instance.seed * 37.0 + t * 4.0) * 0.012 * frame.wind_strength;
        point += across * flutter * t * t * height;
    }

    // Widen blades narrower than the style's minimum on-screen width, keeping
    // the taper; sub-pixel blades shimmer.
    let metres_per_pixel = (clip_from_world * vec4<f32>(instance.root, 1.0)).w * pixel_scale;
    let widen = max(1.0, field.motion.z * metres_per_pixel / max(root_width, 1e-6));
    let width = root_width * widen * (1.0 - pow(t, 1.4));

    var out: BladeVertex;
    out.t = t;
    out.instance = instance;
    let normal = normalize(cross(across, tangent));
    // Rounded normals: each edge tilts its normal outward by about 30°, so the
    // flat strip shades like a curved leaf.
    out.normal = normalize(normal + across * side * 2.0 * 0.577);
    out.world_position = instance.root + point + across * side * width;
    return out;
}
