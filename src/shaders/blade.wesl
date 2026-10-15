//! bevy_grass::shaders::blade — what every grass draw needs to turn one
//! 16-byte instance and a vertex index into a point on a blade.
//! Bindings are group 2 in every grass draw pipeline.

import package::shaders::common::{
    Frame, View, Field, Instance, TAU, WIDTH_RANGE, LOD_VERTEX_STRIDE, unpack, blade_curve, lod_rows,
    instance_random,
};

@group(2) @binding(0) var<uniform> frame: Frame;
/// The camera whose blades these are, even when a light view draws them.
@group(2) @binding(1) var<uniform> grass_view: View;
@group(2) @binding(2) var<storage, read> instances: array<vec4<u32>>;
@group(2) @binding(3) var<storage, read> fields: array<Field>;
/// Smooth noise, wrapping: the gust noise, which colour patches come from.
@group(2) @binding(4) var noise_texture: texture_2d<f32>;
@group(2) @binding(5) var noise_sampler: sampler;

/// One vertex of one blade, before it is placed: which blade, which LOD and
/// which vertex of its strip.
struct BladeRef {
    instance: Instance,
    lod: u32,
    /// The vertex within the LOD's strip.
    local: u32,
}

/// Decodes vertex `vertex_index` of instance `instance_index`.
///
/// The draw for bucket `b` drawing LOD `l` passes
/// `base_vertex = (3b + l) * LOD_VERTEX_STRIDE`, which is how both are known
/// here without `first_instance`.
fn blade_ref(vertex_index: u32, instance_index: u32) -> BladeRef {
    let code = vertex_index / LOD_VERTEX_STRIDE;
    let bucket = code / 3u;
    var out: BladeRef;
    out.lod = code % 3u;
    out.local = vertex_index % LOD_VERTEX_STRIDE;
    out.instance = unpack(instances[grass_view.bucket_base[bucket] + instance_index], grass_view.instance_origin);
    return out;
}

/// The eye a blade is built for, and when.
struct BladeEye {
    /// The camera the blade turns toward when edge-on.
    position: vec3<f32>,
    forward: vec3<f32>,
    is_orthographic: bool,
    clip_from_world: mat4x4<f32>,
    /// `2 / (clip_from_view[0][0] * viewport width)`: times a point's
    /// clip-space `w`, the metres one pixel covers there. 0 skips the
    /// minimum-width rule (shadow passes).
    pixel_scale: f32,
    /// Build the blade as it stood last frame (last frame's bend and clock),
    /// for motion vectors. `position`, `forward` and `clip_from_world` should
    /// then be last frame's too.
    previous: bool,
}

/// A vertex of a blade.
struct BladeVertex {
    world_position: vec3<f32>,
    normal: vec3<f32>,
    /// 0 at the root, 1 at the tip.
    t: f32,
}

/// Places a vertex of a blade for `eye`.
fn blade_vertex(blade: BladeRef, eye: BladeEye) -> BladeVertex {
    let instance = blade.instance;
    let field = fields[instance.field];
    let height = instance.height * field.shape.y;
    let root_width = instance.width * WIDTH_RANGE * field.shape.z;

    let rows = lod_rows(blade.lod);
    let is_tip = blade.local >= 2u * rows;
    let row = select(blade.local / 2u, rows, is_tip);
    var t = f32(row) / f32(rows);
    let side = select(f32(blade.local % 2u) - 0.5, 0.0, is_tip);

    // The shape (`GRASS_SHAPE`): without it, every term below is the plain
    // blade's.
    var twist = 0.0;
    var droop = 0.0;
    var fold = 0.0;
    var in_head = false;
@if(GRASS_SHAPE) {
    let form = field.form;
    // Each blade twists its own way and droops its own amount; tall blades
    // droop the most.
    twist = form.x * TAU * (instance_random(instance, 0x7c1u) * 2.0 - 1.0);
    droop = form.y * instance_random(instance, 0x7c2u) * (0.5 + 0.5 * instance.height);
    fold = form.z;
    // A head takes the last row and the tip: the row moves up to the
    // head's widest point, and the stem's rows spread out below it.
    let has_head = form.w > 0.0 && rows > 1u;
    let top = 1.0 - 0.5 * form.w;
    t = select(t, select(f32(row) / f32(rows - 1u) * top, 1.0, is_tip), has_head);
    in_head = form.w > 0.0 && (is_tip || (has_head && row == rows - 1u));
}

    // The blade's flat side faces `facing`. As it turns edge-on to the eye it
    // is rotated part of the way toward it, so a field seen side-on does not
    // thin out to lines.
    var to_eye = select(eye.position.xz - instance.root.xz, -eye.forward.xz, eye.is_orthographic);
    to_eye = select(vec2<f32>(0.0, 1.0), normalize(to_eye), dot(to_eye, to_eye) > 1e-8);
    let yaw = instance.facing * TAU;
    var face = vec2<f32>(sin(yaw), cos(yaw));
    let facing_eye = dot(face, to_eye);
    let edge_on = 1.0 - abs(facing_eye);
    face = normalize(mix(face, to_eye * select(-1.0, 1.0, facing_eye >= 0.0), edge_on * 0.4));
    // The blade leans and bends one way along its length; twist turns only
    // its cross-section, and a head turns to face the eye.
    var turned = face;
@if(GRASS_SHAPE) {
    let c = cos(twist * t);
    let s = sin(twist * t);
    turned = select(vec2<f32>(face.x * c + face.y * s, face.y * c - face.x * s), to_eye, in_head);
}
    let across = vec3<f32>(turned.y, 0.0, -turned.x);

    let bend = select(instance.bend, instance.previous_bend, eye.previous);
    let curve = blade_curve(height, face * field.motion.x + bend, droop);
    let u = 1.0 - t;
    var point = curve[0] * (u * u) + curve[1] * (2.0 * u * t) + curve[2] * (t * t);
    let tangent = normalize((curve[1] - curve[0]) * (2.0 * u) + (curve[2] - curve[1]) * (2.0 * t) + vec3<f32>(0.0, 1e-5, 0.0));

    // Flutter: a small, fast shiver near the tip of full-detail blades.
    if blade.lod == 0u {
        let time = select(frame.time, frame.previous_time, eye.previous);
        let flutter = sin(time * 11.0 + instance.seed * 37.0 + t * 4.0) * 0.012 * frame.wind_strength;
        point += across * flutter * t * t * height;
    }

    // Widen blades narrower than the style's minimum on-screen width, keeping
    // the taper; sub-pixel blades shimmer.
    let metres_per_pixel = (eye.clip_from_world * vec4<f32>(instance.root, 1.0)).w * eye.pixel_scale;
    let widen = max(1.0, field.motion.z * metres_per_pixel / max(root_width, 1e-6));
    var width = root_width * widen * (1.0 - pow(t, 1.4));
    // Rounded normals: each edge tilts its normal outward by about 30°, so the
    // flat strip shades like a curved leaf.
    var cup = 0.577;
@if(GRASS_SHAPE) {
    // Folding narrows the blade face-on and turns the edges' normals inward,
    // as the two halves of a V face apart.
    width *= 1.0 - 0.4 * fold;
    cup = 0.577 * (1.0 - 2.0 * fold);
    // The head row is the head's widest point, as wide as the head, which
    // grows with the blade (and with distance, as the blade does).
    let head_width = field.head.w * instance.width * WIDTH_RANGE * widen;
    width = select(width, max(width, head_width), in_head);
}

    var out: BladeVertex;
    out.t = t;
    let normal = normalize(cross(across, tangent));
    out.normal = normalize(normal + across * side * 2.0 * cup);
    out.world_position = instance.root + point + across * side * width;
    return out;
}

/// The eye of a Bevy view this frame.
fn current_eye(
    world_from_view: mat4x4<f32>,
    clip_from_view: mat4x4<f32>,
    clip_from_world: mat4x4<f32>,
    viewport_width: f32,
) -> BladeEye {
    var eye: BladeEye;
    eye.position = world_from_view[3].xyz;
    eye.forward = -world_from_view[2].xyz;
    eye.is_orthographic = clip_from_view[3].w == 1.0;
    eye.clip_from_world = clip_from_world;
    eye.pixel_scale = 2.0 / (clip_from_view[0][0] * viewport_width);
    eye.previous = false;
    return eye;
}

/// The eye of a Bevy view last frame, from its previous-view uniforms.
fn previous_eye(
    view_from_world: mat4x4<f32>,
    clip_from_view: mat4x4<f32>,
    clip_from_world: mat4x4<f32>,
    viewport_width: f32,
) -> BladeEye {
    // A view transform is a rotation and a translation, so its inverse is
    // the transposed rotation: no matrix inverse in a vertex shader.
    let rotation = mat3x3<f32>(view_from_world[0].xyz, view_from_world[1].xyz, view_from_world[2].xyz);
    let world_rotation = transpose(rotation);
    var eye: BladeEye;
    eye.position = -(world_rotation * view_from_world[3].xyz);
    eye.forward = -world_rotation[2];
    eye.is_orthographic = clip_from_view[3].w == 1.0;
    eye.clip_from_world = clip_from_world;
    eye.pixel_scale = 2.0 / (clip_from_view[0][0] * viewport_width);
    eye.previous = true;
    return eye;
}
