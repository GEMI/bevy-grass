use super::*;

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_stats_and_diagnostics_report_what_the_gpu_generated() {
    use bevy::diagnostic::{Diagnostic, DiagnosticsStore};

    use crate::{GrassDiagnostics, GrassSettings, GrassStats};

    let mut app = headless_app();
    // Counters are read back every 120 frames; wait for the first reading.
    let mut readings = 0;
    for _ in 0..600 {
        app.update();
        readings = app.world().resource::<GrassStats>().gpu.readings;
        if readings > 0 {
            break;
        }
    }
    assert!(readings > 0, "no GPU reading in 600 frames");

    // The reading is what the GPU generated: the blades the readback test
    // finds in the instance buffers (the camera has not moved).
    let stats = app.world().resource::<GrassStats>().clone();
    let (_, instances) = gpu_instances(&mut app).expect("a grass view");
    let total: u32 = stats.gpu.blades.iter().sum();
    assert!(total > 10_000, "the test scene should place many blades");
    assert_eq!(total as usize, instances.len(), "stats {stats:?}");
    assert!(stats.gpu.blades[0] > 0 && stats.gpu.slices > 0, "{stats:?}");
    assert_eq!(stats.gpu.overflows, 0, "High holds this scene");
    assert!(stats.tables.rebuilds >= 1);
    // A 70 m by 50 m field is 5 by 4 chunks of 16 m.
    assert_eq!((stats.tables.fields, stats.tables.chunks), (1, 5 * 4));
    assert!(stats.tables.last_rebuild > Duration::ZERO);

    // The same numbers are Bevy diagnostics.
    let store = app.world().resource::<DiagnosticsStore>();
    let value = |path: &_| store.get(path).and_then(Diagnostic::value);
    assert_eq!(
        value(&GrassDiagnostics::BLADES_NEAR),
        Some(f64::from(stats.gpu.blades[0]))
    );
    assert_eq!(
        value(&GrassDiagnostics::BLADES_MID),
        Some(f64::from(stats.gpu.blades[1]))
    );
    assert_eq!(
        value(&GrassDiagnostics::BLADES_FAR),
        Some(f64::from(stats.gpu.blades[2]))
    );
    assert_eq!(
        value(&GrassDiagnostics::VISIBLE_SLICES),
        Some(f64::from(stats.gpu.slices))
    );
    assert_eq!(value(&GrassDiagnostics::OVERFLOWS), Some(0.0));
    assert!(value(&GrassDiagnostics::TABLE_REBUILDS).is_some());
    assert!(value(&GrassDiagnostics::REBUILD_TIME).is_some());

    // Too small a buffer overflows, and the game can see it: the capacity
    // change makes new buffers, whose first reading comes 120 frames on.
    *app.world_mut().resource_mut::<GrassSettings>() = GrassSettings {
        instance_capacity: 2_000,
        ..GrassSettings::HIGH
    };
    let mut overflows = 0;
    for _ in 0..600 {
        app.update();
        overflows = app.world().resource::<GrassStats>().gpu.overflows;
        if overflows > 0 {
            break;
        }
    }
    assert!(overflows >= 1, "an overflow went unreported");
    let store = app.world().resource::<DiagnosticsStore>();
    let diagnosed = store
        .get(&GrassDiagnostics::OVERFLOWS)
        .and_then(Diagnostic::value)
        .unwrap();
    assert!(diagnosed >= 1.0);
    // What was wanted is still reported: far more than fit.
    let wanted: u32 = app.world().resource::<GrassStats>().gpu.blades.iter().sum();
    assert!(wanted > 2_000 + 1_000 + 1_000, "{wanted}");
}

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_the_counters_are_read_back_as_often_as_the_settings_say() {
    use crate::{GrassSettings, GrassStats};

    // A reading every 2 frames: plenty in 90.
    let mut app = headless_app();
    app.insert_resource(GrassSettings {
        readback_interval: 2,
        ..GrassSettings::HIGH
    });
    wait_for_grass(&mut app);
    for _ in 0..90 {
        app.update();
    }
    let readings = app.world().resource::<GrassStats>().gpu.readings;
    assert!(readings >= 5, "{readings} readings at one in 2 frames");

    // Never: nothing is read however long the game runs.
    let mut app = headless_app();
    app.insert_resource(GrassSettings {
        readback_interval: 0,
        ..GrassSettings::HIGH
    });
    wait_for_grass(&mut app);
    for _ in 0..300 {
        app.update();
    }
    let stats = app.world().resource::<GrassStats>();
    assert_eq!(stats.gpu.readings, 0);
}
