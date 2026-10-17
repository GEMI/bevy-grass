//! What grass is doing, as numbers: the [`GrassStats`] resource, and the same
//! numbers as Bevy [`Diagnostic`]s for `LogDiagnosticsPlugin` and overlays.
//!
//! "Why is there no grass here?" and "why did my frame spike?" should be
//! answerable without a profiler: how many blades each level of detail drew,
//! whether the buffers overflowed, how many times the tables were rebuilt and
//! how long that took, and whether a limit dropped some fields.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::app::{App, Plugin, PostUpdate};
use bevy::diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};
use bevy::ecs::prelude::*;
use bevy::ecs::reflect::ReflectResource;
use bevy::reflect::Reflect;
use bevy::reflect::std_traits::ReflectDefault;

use crate::GrassSystems;

/// Numbers about what grass is doing. Updated by the plugin; read it for a
/// HUD, a log line or a test.
///
/// The `gpu` numbers come from reading counters back from the GPU, which is
/// done every couple of seconds (120 frames) per camera, not every frame:
/// they are the latest reading, summed over cameras.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// fn hud(stats: Res<GrassStats>) {
///     if stats.gpu.overflows > 0 {
///         warn!("grass buffers overflowed {} times", stats.gpu.overflows);
///     }
/// }
/// ```
#[derive(Resource, Debug, Clone, PartialEq, Default, Reflect)]
#[reflect(Resource, Default, Debug, PartialEq)]
pub struct GrassStats {
    /// What the CPU built: the tables of fields and chunks.
    pub tables: TableStats,
    /// What the GPU generated, as of its latest reading.
    pub gpu: GpuStats,
}

/// The tables of fields and chunks the GPU draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct TableStats {
    /// Fields in the tables, drawn or not (a hidden field is still in them).
    pub fields: usize,
    /// Chunks of 16 m in the tables.
    pub chunks: usize,
    /// Fields left out because the tables hold 256 entries, each species of a
    /// field counting as one. These are not drawn.
    pub fields_dropped: usize,
    /// Species left out because a field mixes at most 16, its own blades
    /// included. These are not drawn.
    pub species_dropped: usize,
    /// Times the tables changed because a field, style, mask or heightmap
    /// did. A field only shown or hidden patches the tables and is not
    /// counted, nor is a change no field shows (a style no field uses).
    pub rebuilds: u64,
    /// Of those, the times the chunks, masks or heightmaps were rebuilt and
    /// uploaded again: a field moved, was resized or was added or removed,
    /// or an image changed. The expensive kind. Changing a style, a colour
    /// every frame for instance, is not one.
    pub layout_rebuilds: u64,
    /// Masks and heightmaps read texel by texel from their images so far. Each
    /// is read once, and again only when that image changes.
    pub images_read: u64,
    /// Field entries (a field and each of its species) worked out again so
    /// far. A change rewrites only the entries of the fields it touches.
    pub entries_rewritten: u64,
    /// How long the latest rebuild took on the CPU.
    pub last_rebuild: Duration,
}

/// Counters read back from the GPU. Summed over cameras.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
#[reflect(Default, Debug, PartialEq)]
pub struct GpuStats {
    /// Blades generated in the latest reading, for the near, mid and far
    /// levels of detail (LOD 0, 1 and 2). Before any buffer overflow they are
    /// what is drawn; after one they are what was wanted.
    pub blades: [u32; 3],
    /// Slices of visible chunks in the latest reading. A dense chunk is
    /// several slices of 512 candidates.
    pub slices: u32,
    /// Readings so far in which a buffer overflowed and blades were dropped.
    /// Raise `GrassSettings::instance_capacity`, or thin the grass.
    pub overflows: u64,
    /// Readings taken so far; the numbers above change when this does.
    pub readings: u64,
}

/// The names of grass's [`Diagnostic`]s, for `DiagnosticsStore::get` and
/// `LogDiagnosticsPlugin`.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy::diagnostic::LogDiagnosticsPlugin;
/// # use bevy_grass::prelude::*;
/// # let mut app = App::new();
/// app.add_plugins((GrassPlugin::default(), LogDiagnosticsPlugin::default()));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct GrassDiagnostics;

impl GrassDiagnostics {
    /// Blades generated at the near level of detail (LOD 0).
    pub const BLADES_NEAR: DiagnosticPath = DiagnosticPath::const_new("grass/blades/near");
    /// Blades generated at the mid level of detail (LOD 1).
    pub const BLADES_MID: DiagnosticPath = DiagnosticPath::const_new("grass/blades/mid");
    /// Blades generated at the far level of detail (LOD 2).
    pub const BLADES_FAR: DiagnosticPath = DiagnosticPath::const_new("grass/blades/far");
    /// Slices of visible chunks.
    pub const VISIBLE_SLICES: DiagnosticPath = DiagnosticPath::const_new("grass/visible_slices");
    /// Readings in which a buffer overflowed, cumulative.
    pub const OVERFLOWS: DiagnosticPath = DiagnosticPath::const_new("grass/overflows");
    /// Full table rebuilds, cumulative.
    pub const TABLE_REBUILDS: DiagnosticPath = DiagnosticPath::const_new("grass/table_rebuilds");
    /// CPU time of the latest full table rebuild, in milliseconds.
    pub const REBUILD_TIME: DiagnosticPath = DiagnosticPath::const_new("grass/rebuild_time");
}

/// Registers the diagnostics and publishes [`GrassStats`] to them. Added by
/// `GrassPlugin`.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct GrassDiagnosticsPlugin;

impl Plugin for GrassDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        let count = |path: DiagnosticPath| Diagnostic::new(path).with_smoothing_factor(0.0);
        app.init_resource::<GrassStats>()
            .init_resource::<GpuReadings>()
            .register_type::<GrassStats>()
            .register_type::<TableStats>()
            .register_type::<GpuStats>()
            .register_diagnostic(count(GrassDiagnostics::BLADES_NEAR))
            .register_diagnostic(count(GrassDiagnostics::BLADES_MID))
            .register_diagnostic(count(GrassDiagnostics::BLADES_FAR))
            .register_diagnostic(count(GrassDiagnostics::VISIBLE_SLICES))
            .register_diagnostic(count(GrassDiagnostics::OVERFLOWS))
            .register_diagnostic(count(GrassDiagnostics::TABLE_REBUILDS))
            .register_diagnostic(
                Diagnostic::new(GrassDiagnostics::REBUILD_TIME)
                    .with_suffix(" ms")
                    .with_smoothing_factor(0.0),
            )
            .add_systems(
                PostUpdate,
                (collect_gpu_readings, publish)
                    .chain()
                    .after(GrassSystems::Rebuild),
            );
    }
}

/// What the render world read back, shared with the main world. A readback
/// completes on the render side, so this is how it gets across.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct GpuReadings(Arc<Mutex<Readings>>);

#[derive(Debug, Default)]
struct Readings {
    /// The latest reading of each camera's counters, `[near, mid, far,
    /// slices]`, keyed by the camera's render-world entity.
    views: HashMap<Entity, [u32; 4]>,
    overflows: u64,
    samples: u64,
}

impl GpuReadings {
    /// Records a reading of `view`'s counters, and whether it overflowed.
    pub(crate) fn record(&self, view: Entity, counters: [u32; 4], overflowed: bool) {
        if let Ok(mut readings) = self.0.lock() {
            readings.views.insert(view, counters);
            readings.overflows += u64::from(overflowed);
            readings.samples += 1;
        }
    }

    /// Forgets a camera that is gone, so its last reading stops counting.
    pub(crate) fn forget(&self, view: Entity) {
        if let Ok(mut readings) = self.0.lock() {
            readings.views.remove(&view);
        }
    }

    /// The readings so far, summed over cameras.
    fn total(&self) -> GpuStats {
        let Ok(readings) = self.0.lock() else {
            return GpuStats::default();
        };
        let mut stats = GpuStats {
            overflows: readings.overflows,
            readings: readings.samples,
            ..GpuStats::default()
        };
        for counters in readings.views.values() {
            for (sum, count) in stats.blades.iter_mut().zip(counters) {
                *sum = sum.saturating_add(*count);
            }
            stats.slices = stats.slices.saturating_add(counters[3]);
        }
        stats
    }
}

/// Copies the render world's readings into [`GrassStats`] when there is a new
/// one. Reading `stats` first keeps its change flag quiet otherwise.
fn collect_gpu_readings(readings: Res<GpuReadings>, mut stats: ResMut<GrassStats>) {
    let total = readings.total();
    if total != stats.gpu {
        stats.gpu = total;
    }
}

/// Measures the diagnostics from [`GrassStats`] when it changed.
fn publish(stats: Res<GrassStats>, mut diagnostics: Diagnostics, mut published: Local<(u64, u64)>) {
    let (rebuilds, readings) = *published;
    if stats.tables.rebuilds != rebuilds {
        diagnostics.add_measurement(&GrassDiagnostics::TABLE_REBUILDS, || {
            stats.tables.rebuilds as f64
        });
        diagnostics.add_measurement(&GrassDiagnostics::REBUILD_TIME, || {
            stats.tables.last_rebuild.as_secs_f64() * 1000.0
        });
    }
    if stats.gpu.readings != readings {
        let gpu = stats.gpu;
        diagnostics.add_measurement(&GrassDiagnostics::BLADES_NEAR, || f64::from(gpu.blades[0]));
        diagnostics.add_measurement(&GrassDiagnostics::BLADES_MID, || f64::from(gpu.blades[1]));
        diagnostics.add_measurement(&GrassDiagnostics::BLADES_FAR, || f64::from(gpu.blades[2]));
        diagnostics.add_measurement(&GrassDiagnostics::VISIBLE_SLICES, || f64::from(gpu.slices));
        diagnostics.add_measurement(&GrassDiagnostics::OVERFLOWS, || gpu.overflows as f64);
    }
    *published = (stats.tables.rebuilds, stats.gpu.readings);
}

#[cfg(test)]
mod tests {
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
}
