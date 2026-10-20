//! bevy_grass::shaders::trample — each view's trample cells, updated before
//! blade generation reads them (`compute.rs` dispatches both passes):
//!
//! 1. `trample_decay`: one thread per cell. Flattened grass recovers a step,
//!    and cells that just came into the window are cleared.
//! 2. `trample_stamp`: one workgroup per interactor. A disc pushes the cells
//!    under it outward; a box flattens the cells under it the way it moves
//!    and pushes the ones along its sides outward. Overlapping stamps keep
//!    the strongest push.
//!
//! Bindings are the view group (1) of the generation passes; this module
//! declares only what it uses.

import package::shaders::common::{
    View, TRAMPLE_TEXELS, trample_pack, trample_slot, trample_cell,
    in_trample_window,
};

@group(1) @binding(0) var<uniform> view: View;
@group(1) @binding(5) var<storage, read_write> trample: array<atomic<u32>>;
/// This frame's interactor stamps, three `vec4`s each (`trample::Stamp`):
/// x, z, radius, strength; then a box's axis on the ground and core half
/// extents; then its motion on the ground, rounding and margin. A disc has a
/// margin of 0.
@group(1) @binding(6) var<storage, read_write> interactors: array<vec4<f32>>;

/// Lets every flattened cell recover a step, and clears the cells whose world
/// position has just come into the window (their slot held a cell that
/// scrolled out).
@compute @workgroup_size(64)
fn trample_decay(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = u32(TRAMPLE_TEXELS);
    if id.x >= size * size {
        return;
    }
    let slot = vec2<i32>(i32(id.x % size), i32(id.x / size));
    let cell = trample_cell(slot, view.trample_origin.xy);
    if !in_trample_window(cell, view.trample_origin.zw) {
        atomicStore(&trample[id.x], 0u);
        return;
    }
    let word = atomicLoad(&trample[id.x]);
    let recovery = view.trample_step.y;
    if word == 0u || recovery <= 0.0 {
        return;
    }
    // At least one step a frame, so grass recovers at any frame rate.
    let step = max(1u, u32(round(recovery * 1023.0)));
    let strength = word >> 22u;
    atomicStore(&trample[id.x], select(0u, ((strength - step) << 22u) | (word & 0x3fffffu), strength > step));
}

/// One workgroup per interactor: a disc or a box.
@compute @workgroup_size(64)
fn trample_stamp(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    if group.x >= view.interactor_count {
        return;
    }
    let interactor = interactors[group.x * 3u];
    let motion = interactors[group.x * 3u + 2u];
    if motion.w > 0.0 {
        stamp_box(interactor, interactors[group.x * 3u + 1u], motion, lane);
    } else {
        stamp_disc(interactor, lane);
    }
}

/// Pushes every cell of a disc outward, strongest at the centre.
fn stamp_disc(interactor: vec4<f32>, lane: u32) {
    let center = interactor.xy;
    let radius = min(interactor.z, view.limits.y);
    let texel = view.trample_step.x;
    let low = vec2<i32>(floor((center - radius) / texel));
    let side = i32(ceil(2.0 * radius / texel)) + 2;
    for (var i = i32(lane); i < side * side; i += 64) {
        let cell = low + vec2<i32>(i % side, i / side);
        if !in_trample_window(cell, view.trample_origin.xy) {
            continue;
        }
        let offset = (vec2<f32>(cell) + 0.5) * texel - center;
        let reach = length(offset);
        if reach >= radius {
            continue;
        }
        let strength = interactor.w * (1.0 - smoothstep(radius * 0.6, radius, reach));
        let away = select(vec2<f32>(1.0, 0.0), offset / reach, reach > 1e-4);
        atomicMax(&trample[trample_slot(cell)], trample_pack(away * strength));
    }
}

/// Flattens the cells of a box: a rounded rectangle on the ground, `shape.xy`
/// its axis, `shape.zw` the half extents of its square core, `motion.z` the
/// rounding around the core and `motion.w` the margin past its edge over
/// which the push fades out.
///
/// Every cell under the box lies flat; the direction is what a moving box
/// does to grass. Under it, the grass lies the way it moves (`motion.xy`,
/// whose length says how much it moves). Along its sides, in a band a fifth
/// of its half width deep (a cell at least), the grass is pushed out from
/// the nearest edge. The trailing edge does not push back: the grass behind a
/// moving box keeps lying the way the box went. A box at rest pushes every
/// cell out from its nearest edge.
fn stamp_box(interactor: vec4<f32>, shape: vec4<f32>, motion: vec4<f32>, lane: u32) {
    let center = interactor.xy;
    let along = shape.xy;
    let across = vec2<f32>(-along.y, along.x);
    let core = shape.zw;
    let rounding = motion.z;
    let margin = motion.w;
    let texel = view.trample_step.x;
    let moving = min(length(motion.xy), 1.0);
    let heading = select(vec2<f32>(0.0), motion.xy / length(motion.xy), moving > 1e-4);
    let band = max(0.2 * min(core.x, core.y) + 0.2 * rounding, texel);
    let radius = interactor.z;
    let low = vec2<i32>(floor((center - radius) / texel));
    let side = i32(ceil(2.0 * radius / texel)) + 2;
    for (var i = i32(lane); i < side * side; i += 64) {
        let cell = low + vec2<i32>(i % side, i / side);
        if !in_trample_window(cell, view.trample_origin.xy) {
            continue;
        }
        let offset = (vec2<f32>(cell) + 0.5) * texel - center;
        let local = vec2<f32>(dot(offset, along), dot(offset, across));
        // The signed distance to the rounded rectangle, and its outward
        // normal: from the nearest point of the core outside it, along the
        // nearest side inside it.
        let past = abs(local) - core;
        let outside = length(max(past, vec2<f32>(0.0)));
        let distance = outside + min(max(past.x, past.y), 0.0) - rounding;
        if distance >= margin {
            continue;
        }
        let sides = sign(local) + select(vec2<f32>(0.0), vec2<f32>(1.0), local == vec2<f32>(0.0));
        var normal = select(vec2<f32>(0.0, sides.y), vec2<f32>(sides.x, 0.0), past.x > past.y);
        if outside > 1e-4 {
            normal = sides * max(past, vec2<f32>(0.0)) / outside;
        }
        let out = along * normal.x + across * normal.y;
        let strength = interactor.w * (1.0 - smoothstep(0.0, margin, distance));
        let edge = smoothstep(-band, -0.25 * band, distance);
        let trailing = max(-dot(out, heading), 0.0) * moving;
        let under = normalize(mix(out, heading, moving) + out * 1e-3);
        let sideways = edge * (1.0 - trailing);
        let push = normalize(mix(under, out, sideways) + out * 1e-3);
        atomicMax(&trample[trample_slot(cell)], trample_pack(push * strength));
    }
}
