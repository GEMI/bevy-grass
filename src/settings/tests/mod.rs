use super::*;

#[test]
fn the_tiers_keep_the_numbers_they_always_had() {
    // (radius, density scale, shadows, orthographic LOD, capacity)
    let expected = [
        (
            GrassQuality::Competitive,
            40.0,
            0.5,
            GrassShadows::Off,
            1,
            100_000,
        ),
        (GrassQuality::Low, 40.0, 0.6, GrassShadows::Off, 1, 80_000),
        (
            GrassQuality::Medium,
            70.0,
            0.8,
            GrassShadows::Near,
            1,
            250_000,
        ),
        (
            GrassQuality::High,
            100.0,
            1.0,
            GrassShadows::Near,
            1,
            600_000,
        ),
        (
            GrassQuality::Ultra,
            150.0,
            1.0,
            GrassShadows::Full,
            0,
            1_200_000,
        ),
    ];
    for (tier, radius, density, shadows, lod, capacity) in expected {
        let settings = GrassSettings::from(tier);
        assert_eq!(settings.radius, radius, "{tier:?}");
        assert_eq!(settings.density_scale, density, "{tier:?}");
        assert_eq!(settings.min_width_px, 1.0, "{tier:?}");
        assert_eq!(settings.shadows, shadows, "{tier:?}");
        assert_eq!(settings.orthographic_lod, lod, "{tier:?}");
        assert_eq!(settings.instance_capacity, capacity, "{tier:?}");
        assert_eq!(settings.max_interactors, 64, "{tier:?}");
        // The convenience accessors agree.
        assert_eq!(tier.radius(), radius);
        assert_eq!(tier.instance_capacity(), capacity);
        assert_eq!(tier.shadows(), shadows);
    }
    assert_eq!(GrassSettings::default(), GrassSettings::HIGH);
    assert_eq!(GrassQuality::default().settings(), GrassSettings::default());
}

#[test]
fn a_tier_can_be_overridden_one_knob_at_a_time_even_in_a_const() {
    // The point of the type: High, but a longer reach, no `Option`s.
    const TALL_CAMERA: GrassSettings = GrassSettings {
        radius: 140.0,
        ..GrassSettings::HIGH
    };
    assert_eq!(TALL_CAMERA.radius, 140.0);
    assert_eq!(
        TALL_CAMERA.instance_capacity,
        GrassSettings::HIGH.instance_capacity
    );
    assert_eq!(TALL_CAMERA.shadows, GrassSettings::HIGH.shadows);
    assert_ne!(TALL_CAMERA, GrassSettings::ULTRA);
}

#[test]
fn the_settings_are_a_reflected_resource() {
    use bevy::app::App;
    use bevy::ecs::reflect::AppTypeRegistry;
    use std::any::TypeId;

    let mut app = App::new();
    app.register_type::<GrassSettings>()
        .register_type::<GrassShadows>();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    // Inspectors and scenes find the resource through these.
    let id = TypeId::of::<GrassSettings>();
    assert!(registry.get_type_data::<ReflectResource>(id).is_some());
    assert!(registry.get_type_data::<ReflectDefault>(id).is_some());
    assert!(
        registry
            .get_type_data::<ReflectDefault>(TypeId::of::<GrassShadows>())
            .is_some()
    );
}

#[test]
fn a_camera_overrides_only_what_it_names() {
    let settings = GrassSettings::HIGH;
    // No component, or one that names nothing: the resource, exactly.
    assert_eq!(settings.for_camera(None), settings);
    assert_eq!(settings.for_camera(Some(&GrassCamera::default())), settings);

    // A minimap at Low reach next to a main camera at High.
    let minimap = GrassCamera {
        radius: Some(30.0),
        density_scale: Some(0.4),
        min_width_px: Some(3.0),
        shadows: Some(GrassShadows::Off),
        radius_from_ground: false,
    };
    let seen = settings.for_camera(Some(&minimap));
    assert_eq!(seen.radius, 30.0);
    assert_eq!(seen.density_scale, 0.4);
    assert_eq!(seen.min_width_px, 3.0);
    assert_eq!(seen.shadows, GrassShadows::Off);
    // Everything it did not name is the resource's, capacity included.
    assert_eq!(seen.instance_capacity, settings.instance_capacity);
    assert_eq!(seen.max_interactors, settings.max_interactors);
    assert_eq!(seen.orthographic_lod, settings.orthographic_lod);

    // One override leaves the rest following the resource as it changes.
    let radius_only = GrassCamera {
        radius: Some(30.0),
        ..GrassCamera::default()
    };
    let changed = GrassSettings {
        density_scale: 0.5,
        shadows: GrassShadows::Full,
        ..settings
    };
    let seen = changed.for_camera(Some(&radius_only));
    assert_eq!((seen.radius, seen.density_scale), (30.0, 0.5));
    assert_eq!(seen.shadows, GrassShadows::Full);

    // An override is clamped like the resource.
    let bad = GrassCamera {
        radius: Some(-1.0),
        density_scale: Some(f32::NAN),
        ..GrassCamera::default()
    };
    let seen = settings.for_camera(Some(&bad));
    assert_eq!(seen.radius, 1.0);
    assert_eq!(seen.density_scale, settings.density_scale);
}

#[test]
fn the_camera_component_is_reflected() {
    use bevy::app::App;
    use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent};
    use std::any::TypeId;

    let mut app = App::new();
    app.register_type::<GrassCamera>();
    let registry = app.world().resource::<AppTypeRegistry>().read();
    let id = TypeId::of::<GrassCamera>();
    assert!(registry.get_type_data::<ReflectComponent>(id).is_some());
    assert!(registry.get_type_data::<ReflectDefault>(id).is_some());
}

#[test]
fn every_tier_is_in_range() {
    for tier in [
        GrassSettings::COMPETITIVE,
        GrassSettings::LOW,
        GrassSettings::MEDIUM,
        GrassSettings::HIGH,
        GrassSettings::ULTRA,
    ] {
        assert_eq!(tier.sanitized(), tier);
        assert!(tier.problems().is_empty());
    }
}

#[test]
fn out_of_range_values_are_clamped_and_named() {
    let bad = GrassSettings {
        radius: -5.0,
        density_scale: f32::NAN,
        min_width_px: -1.0,
        orthographic_lod: 9,
        instance_capacity: u32::MAX,
        max_interactors: 0,
        ..GrassSettings::HIGH
    };
    let fixed = bad.sanitized();
    assert_eq!(fixed.radius, 1.0);
    assert_eq!(fixed.density_scale, GrassSettings::HIGH.density_scale);
    assert_eq!(fixed.min_width_px, 0.0);
    assert_eq!(fixed.orthographic_lod, 2);
    assert_eq!(
        fixed.instance_capacity,
        GrassSettings::MAX_INSTANCE_CAPACITY
    );
    assert_eq!(fixed.max_interactors, 1);
    assert_eq!(fixed.sanitized(), fixed, "clamping is idempotent");

    let problems = bad.problems().join("; ");
    for name in [
        "radius",
        "density_scale",
        "min_width_px",
        "orthographic_lod",
        "instance_capacity",
        "max_interactors",
    ] {
        assert!(problems.contains(name), "no mention of {name}: {problems}");
    }
    // The largest allowed capacity fits the default storage binding.
    let bytes = u64::from(GrassSettings::MAX_INSTANCE_CAPACITY) * 32;
    assert!(bytes <= 128 * 1024 * 1024);
}

#[test]
fn the_new_knobs_default_to_the_numbers_that_were_once_fixed() {
    // Every tier shares them, and they are what the shaders and the
    // golden images were made with: changing one changes every picture.
    for tier in [
        GrassSettings::COMPETITIVE,
        GrassSettings::LOW,
        GrassSettings::MEDIUM,
        GrassSettings::HIGH,
        GrassSettings::ULTRA,
    ] {
        assert_eq!(tier.blade_rows, [7, 3, 1]);
        assert_eq!((tier.lod1_start, tier.lod2_start), (0.2, 0.5));
        assert_eq!(tier.fade_start, 0.9);
        assert_eq!(tier.lod_blend, 4.0);
        assert_eq!((tier.lod2_density, tier.fade_density), (0.5, 0.15));
        assert_eq!((tier.mid_range_start, tier.mid_range_density), (0.1, 0.5));
        assert_eq!(tier.max_widening, 2.5);
        assert_eq!(tier.turn_to_camera, 0.4);
        assert_eq!(tier.orthographic_extent, 256.0);
        assert_eq!(tier.max_chunk_slices, 8192);
        assert_eq!(tier.readback_interval, 120);
        assert_eq!(tier.trample_recovery, 1.5);
        assert_eq!(tier.trample_cell_size, 0.25);
        assert_eq!(tier.max_interactor_radius, 2.0);
    }
}

#[test]
fn the_new_knobs_are_clamped_and_named_like_the_others() {
    let bad = GrassSettings {
        blade_rows: [0, 99, 5],
        lod_blend: 0.0,
        lod2_density: 7.0,
        fade_density: -1.0,
        mid_range_start: 2.0,
        mid_range_density: f32::NAN,
        max_widening: 50.0,
        turn_to_camera: -2.0,
        orthographic_extent: 1.0e9,
        max_chunk_slices: 0,
        trample_recovery: 0.0,
        trample_cell_size: 0.0,
        max_interactor_radius: 1000.0,
        ..GrassSettings::HIGH
    };
    let fixed = bad.sanitized();
    assert_eq!(fixed.blade_rows, [1, GrassSettings::MAX_BLADE_ROWS, 5]);
    assert_eq!(fixed.lod_blend, 0.01);
    assert_eq!((fixed.lod2_density, fixed.fade_density), (1.0, 0.0));
    assert_eq!(fixed.mid_range_start, 1.0);
    assert_eq!(fixed.mid_range_density, 0.5, "NaN: the default");
    assert_eq!(fixed.max_widening, GrassSettings::MAX_WIDENING);
    assert_eq!(fixed.turn_to_camera, 0.0);
    assert_eq!(
        fixed.orthographic_extent,
        GrassSettings::MAX_ORTHOGRAPHIC_EXTENT
    );
    assert_eq!(fixed.max_chunk_slices, 1);
    assert_eq!(fixed.trample_recovery, 0.01);
    assert_eq!(fixed.trample_cell_size, 0.05);
    assert_eq!(
        fixed.max_interactor_radius,
        GrassSettings::MAX_INTERACTOR_RADIUS
    );
    assert_eq!(fixed.sanitized(), fixed, "clamping is idempotent");

    let problems = bad.problems().join("; ");
    for name in [
        "blade_rows",
        "lod_blend",
        "lod2_density",
        "fade_density",
        "mid_range_start",
        "mid_range_density",
        "max_widening",
        "turn_to_camera",
        "orthographic_extent",
        "max_chunk_slices",
        "trample_recovery",
        "trample_cell_size",
        "max_interactor_radius",
    ] {
        assert!(problems.contains(name), "no mention of {name}: {problems}");
    }

    // The largest limits are ones the GPU can honour: a dispatch holds
    // 65,535 workgroups, and the widest blade fits its stored width.
    assert_eq!(GrassSettings::MAX_CHUNK_SLICES, 65_535);
    const { assert!(1.25 * GrassSettings::MAX_WIDENING <= 4.0) };
    // Sixteen bits of position over the widest extent is still centimetres.
    const { assert!(2.0 * GrassSettings::MAX_ORTHOGRAPHIC_EXTENT / 65_535.0 < 0.15) };
    // A zero readback interval means never, and is valid.
    let never = GrassSettings {
        readback_interval: 0,
        ..GrassSettings::HIGH
    };
    assert_eq!(never.sanitized(), never);
    assert!(never.problems().is_empty());
}

#[test]
fn the_distances_keep_their_order() {
    // LOD 1, then LOD 2, then the fade, each within the radius.
    let backwards = GrassSettings {
        lod1_start: 0.8,
        lod2_start: 0.3,
        fade_start: 0.1,
        ..GrassSettings::HIGH
    };
    let fixed = backwards.sanitized();
    assert_eq!(
        (fixed.lod1_start, fixed.lod2_start, fixed.fade_start),
        (0.8, 0.8, 0.8)
    );
    assert!(backwards.problems().join(";").contains("lod2_start"));
    let beyond = GrassSettings {
        lod1_start: 4.0,
        ..GrassSettings::HIGH
    };
    assert_eq!(beyond.sanitized().lod1_start, 1.0);
    assert_eq!(beyond.sanitized().fade_start, 1.0, "the fade follows");
    // A camera sees them through its own radius.
    let near = GrassSettings {
        radius: 50.0,
        ..GrassSettings::HIGH
    };
    let seen = near.for_camera(Some(&GrassCamera {
        radius: Some(30.0),
        ..GrassCamera::default()
    }));
    assert_eq!(seen.lod1_start, 0.2, "shares of whichever radius is used");
}
