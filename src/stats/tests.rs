use bevy::MinimalPlugins;
use bevy::app::Update;
use bevy::diagnostic::{DiagnosticsPlugin, DiagnosticsStore};

use super::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, DiagnosticsPlugin, GrassDiagnosticsPlugin));
    app
}

fn latest(app: &App, path: &DiagnosticPath) -> Option<f64> {
    app.world()
        .resource::<DiagnosticsStore>()
        .get(path)
        .and_then(Diagnostic::value)
}

#[test]
fn every_diagnostic_is_registered() {
    let app = app();
    let store = app.world().resource::<DiagnosticsStore>();
    for path in [
        GrassDiagnostics::BLADES_NEAR,
        GrassDiagnostics::BLADES_MID,
        GrassDiagnostics::BLADES_FAR,
        GrassDiagnostics::VISIBLE_SLICES,
        GrassDiagnostics::OVERFLOWS,
        GrassDiagnostics::TABLE_REBUILDS,
        GrassDiagnostics::REBUILD_TIME,
    ] {
        assert!(store.get(&path).is_some(), "{path} is not registered");
        assert!(path.as_str().starts_with("grass/"), "{path}");
    }
}

#[test]
fn nothing_is_measured_until_there_is_something_to_say() {
    let mut app = app();
    app.update();
    app.update();
    assert_eq!(latest(&app, &GrassDiagnostics::TABLE_REBUILDS), None);
    assert_eq!(latest(&app, &GrassDiagnostics::BLADES_NEAR), None);
}

#[test]
fn a_rebuild_is_published_once_with_its_time() {
    let mut app = app();
    app.world_mut().resource_mut::<GrassStats>().tables = TableStats {
        rebuilds: 3,
        last_rebuild: Duration::from_micros(2500),
        ..TableStats::default()
    };
    app.update();
    assert_eq!(latest(&app, &GrassDiagnostics::TABLE_REBUILDS), Some(3.0));
    let time = latest(&app, &GrassDiagnostics::REBUILD_TIME).unwrap();
    assert!((time - 2.5).abs() < 1e-9, "{time}");

    // The same rebuild is not measured again every frame.
    let measured = |app: &App| {
        app.world()
            .resource::<DiagnosticsStore>()
            .get(&GrassDiagnostics::TABLE_REBUILDS)
            .unwrap()
            .measurement()
            .map(|m| m.time)
    };
    let first = measured(&app);
    app.update();
    app.update();
    assert_eq!(measured(&app), first);
}

#[test]
fn gpu_readings_are_summed_over_cameras_and_forgotten_with_them() {
    let mut app = app();
    let mut world = World::new();
    let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
    let readings = app.world().resource::<GpuReadings>().clone();

    readings.record(a, [100, 40, 10, 7], false);
    readings.record(b, [50, 20, 5, 3], true);
    app.update();
    let gpu = app.world().resource::<GrassStats>().gpu;
    assert_eq!(gpu.blades, [150, 60, 15]);
    assert_eq!(gpu.slices, 10);
    assert_eq!(gpu.overflows, 1, "one reading overflowed");
    assert_eq!(gpu.readings, 2);
    assert_eq!(latest(&app, &GrassDiagnostics::BLADES_NEAR), Some(150.0));
    assert_eq!(latest(&app, &GrassDiagnostics::BLADES_FAR), Some(15.0));
    assert_eq!(latest(&app, &GrassDiagnostics::VISIBLE_SLICES), Some(10.0));
    assert_eq!(latest(&app, &GrassDiagnostics::OVERFLOWS), Some(1.0));

    // A camera that goes away no longer counts; the overflow stays said.
    readings.forget(b);
    readings.record(a, [90, 30, 8, 6], false);
    app.update();
    let gpu = app.world().resource::<GrassStats>().gpu;
    assert_eq!(gpu.blades, [90, 30, 8]);
    assert_eq!(gpu.overflows, 1);
    assert_eq!(latest(&app, &GrassDiagnostics::BLADES_NEAR), Some(90.0));
}

#[test]
fn the_stats_are_quiet_for_change_detection_when_nothing_changed() {
    let mut app = app();
    app.add_systems(Update, |stats: Res<GrassStats>, mut changes: Local<u32>| {
        if stats.is_changed() {
            *changes += 1;
        }
        assert!(
            *changes <= 2,
            "changed {} times with nothing to report",
            *changes
        );
    });
    for _ in 0..10 {
        app.update();
    }
}
