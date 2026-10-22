use super::*;

#[test]
fn webgl2_has_no_compute_shaders() {
    let support = check(
        DownlevelFlags::empty(),
        &WgpuLimits::downlevel_webgl2_defaults(),
        &GrassLayouts::new(),
    );
    assert_eq!(support, GrassSupport::NoComputeShaders);
    assert!(!support.is_supported());
}

#[test]
fn storage_buffers_are_counted_from_the_layouts() {
    // `generate`: group 0 (6) and its group 1 (4).
    assert_eq!(storage_buffers_needed(&GrassLayouts::new()), 10);
}

#[test]
fn every_pipeline_fits_the_budget_and_draws_read_no_storage_per_fragment() {
    let layouts = GrassLayouts::new();
    for pass in ComputePass::ALL {
        let count = storage_buffers(&layouts.compute(pass), ShaderStages::COMPUTE);
        assert!(count <= STORAGE_BUFFER_BUDGET, "{pass:?} binds {count}");
    }
    let draw = core::slice::from_ref(&layouts.draw);
    assert!(storage_buffers(draw, ShaderStages::VERTEX) <= STORAGE_BUFFER_BUDGET);
    assert_eq!(storage_buffers(draw, ShaderStages::FRAGMENT), 0);
    assert!(storage_buffers_needed(&layouts) <= STORAGE_BUFFER_BUDGET);
}

#[test]
fn too_few_storage_buffers_names_both_numbers() {
    let layouts = GrassLayouts::new();
    let needed = storage_buffers_needed(&layouts);
    let limits = WgpuLimits {
        max_storage_buffers_per_shader_stage: needed - 1,
        ..WgpuLimits::defaults()
    };
    let support = check(DownlevelFlags::all(), &limits, &layouts);
    assert_eq!(
        support,
        GrassSupport::TooFewStorageBuffers {
            needed,
            limit: needed - 1
        }
    );
    let message = support.to_string();
    assert!(
        message.contains(&format!("allows {}", needed - 1))
            && message.contains(&format!("binds {needed}")),
        "{message}"
    );
}

#[test]
fn a_device_with_room_draws_grass() {
    let limits = WgpuLimits {
        max_storage_buffers_per_shader_stage: 16,
        ..WgpuLimits::defaults()
    };
    let support = check(DownlevelFlags::all(), &limits, &GrassLayouts::new());
    assert_eq!(support, GrassSupport::Supported);
}

#[test]
fn small_storage_bindings_are_refused() {
    let limits = WgpuLimits {
        max_storage_buffers_per_shader_stage: 16,
        max_storage_buffer_binding_size: 64 << 20,
        ..WgpuLimits::defaults()
    };
    let support = check(DownlevelFlags::all(), &limits, &GrassLayouts::new());
    assert_eq!(
        support,
        GrassSupport::StorageBindingTooSmall {
            needed: MAX_INSTANCE_BYTES,
            limit: 64 << 20
        }
    );
}

#[test]
fn masks_and_heightmaps_must_fit_one_binding_together() {
    let limit = 128 << 20;
    let half = (limit / 4 / 2) as usize;
    // Each alone fits, as each did in a binding of its own.
    assert_eq!(scene_too_large(half + 1, 0, limit), None);
    assert_eq!(scene_too_large(0, half + 1, limit), None);
    assert_eq!(scene_too_large(half, half, limit), None);
    assert_eq!(
        scene_too_large(half + 1, half, limit),
        Some(limit + 4),
        "together they are one word over"
    );
}

#[test]
fn every_compute_pass_is_listed_once_in_order() {
    // `GeneratePipelines` is indexed by `pass as usize`.
    for (index, pass) in ComputePass::ALL.into_iter().enumerate() {
        assert_eq!(pass as usize, index, "{pass:?}");
    }
}
