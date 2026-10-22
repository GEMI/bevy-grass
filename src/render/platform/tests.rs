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
    // `generate`: group 0 (7) and its group 1 (4).
    assert_eq!(storage_buffers_needed(&GrassLayouts::new()), 11);
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
