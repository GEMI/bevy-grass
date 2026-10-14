//! bevy_grass::shaders::trample — each view's trample cells, updated before
//! blade generation reads them (`compute.rs` dispatches both passes):
//!
//! 1. `trample_decay`: one thread per cell. Flattened grass recovers a step,
//!    and cells that just came into the window are cleared.
//! 2. `trample_stamp`: one workgroup per interactor. Pushes the cells of its
//!    disc outward; overlapping discs keep the strongest push.
//!
//! Bindings are the view group (1) of the generation passes; this module
//! declares only what it uses.

import package::shaders::common::{
    View, TRAMPLE_TEXELS, MAX_PUSH_RADIUS, trample_pack, trample_slot, trample_cell,
    in_trample_window,
};

@group(1) @binding(0) var<uniform> view: View;
@group(1) @binding(5) var<storage, read_write> trample: array<atomic<u32>>;
/// This frame's interactors: x, z, radius, strength.
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

/// One workgroup per interactor: pushes every cell of its disc outward,
/// strongest at the centre. Overlapping discs keep the strongest push.
@compute @workgroup_size(64)
fn trample_stamp(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    if group.x >= view.interactor_count {
        return;
    }
    let interactor = interactors[group.x];
    let center = interactor.xy;
    let radius = min(interactor.z, MAX_PUSH_RADIUS);
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
