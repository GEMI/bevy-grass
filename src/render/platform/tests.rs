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
    // Group 0 (7) and group 1 (6) of every compute pass, plus group 2 of
    // `size_dispatch`.
    assert_eq!(storage_buffers_needed(&GrassLayouts::new()), 14);
}

#[test]
fn too_few_storage_buffers_names_both_numbers() {
    let limits = WgpuLimits {
        max_storage_buffers_per_shader_stage: 8,
        ..WgpuLimits::defaults()
    };
    let support = check(DownlevelFlags::all(), &limits, &GrassLayouts::new());
    assert_eq!(
        support,
        GrassSupport::TooFewStorageBuffers {
            needed: 14,
            limit: 8
        }
    );
    let message = support.to_string();
    assert!(
        message.contains("allows 8") && message.contains("binds 14"),
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
