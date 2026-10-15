//! Flags shared by the examples, so every example can be measured and
//! screenshotted the same way:
//!
//! * `--frames N` exits after N frames and prints p50 and p95 frame times.
//! * `--screenshot PATH` saves the frame at `--frames` (default 600, after
//!   shader compilation) and exits.
//! * `--fixed-time` advances the clock exactly 1/60 s per frame, so the wind in
//!   a screenshot is the same on every run and every machine.
//! * `--perf` logs Bevy's per-pass CPU and GPU times once a second; the grass
//!   compute pass appears as `grass_indirect_args`.
//! * `--single-threaded` runs each app's systems on one thread (see
//!   `single_threaded_schedules`); faster on CPUs with few cores.
//! * `--record DIR` saves every second frame of the last `--record-frames`
//!   (default 600) before `--frames` as numbered PNGs in `DIR`: with
//!   `--fixed-time`, 30 frames for each second of game time, for a video.
//!
//! Any other `--flag` is kept for the example itself (`ExampleArgs::has`).

#![allow(dead_code)]

use std::time::Duration;

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::render::{ExtractSchedule, Render, RenderApp};
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};

#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct ExampleArgs {
    pub(crate) frames: Option<u32>,
    pub(crate) screenshot: Option<String>,
    pub(crate) fixed_time: bool,
    pub(crate) perf: bool,
    /// Single-threaded schedules, from `--single-threaded`.
    pub(crate) single_threaded: bool,
    /// Where `--record` saves frames, and how many frames it covers.
    pub(crate) record: Option<(String, u32)>,
    /// Flags the example reads itself.
    pub(crate) flags: Vec<String>,
}

impl ExampleArgs {
    pub(crate) fn parse() -> Self {
        let mut args = Self::default();
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--frames" => args.frames = it.next().and_then(|v| v.parse().ok()),
                "--screenshot" => args.screenshot = it.next(),
                "--fixed-time" => args.fixed_time = true,
                "--perf" => args.perf = true,
                "--single-threaded" => args.single_threaded = true,
                "--record" => {
                    let dir = it.next().expect("--record needs a directory");
                    args.record = Some((dir, args.record.as_ref().map_or(600, |r| r.1)));
                }
                "--record-frames" => {
                    let count = it.next().and_then(|v| v.parse().ok()).unwrap_or(600);
                    let dir = args.record.take().map_or_else(String::new, |r| r.0);
                    args.record = Some((dir, count));
                }
                other => args.flags.push(other.to_owned()),
            }
        }
        if let Some((dir, count)) = &args.record {
            assert!(!dir.is_empty(), "--record-frames needs --record DIR");
            if args.frames.is_none() {
                args.frames = Some(count + 300);
            }
        }
        if args.screenshot.is_some() && args.frames.is_none() {
            args.frames = Some(600);
        }
        args
    }

    /// Whether the example's own `flag` was given.
    pub(crate) fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    /// The value after the example's own `flag`, if given.
    pub(crate) fn value(&self, flag: &str) -> Option<&str> {
        let at = self.flags.iter().position(|f| f == flag)?;
        self.flags.get(at + 1).map(String::as_str)
    }

    /// `DefaultPlugins` with a window that does not wait for vsync when
    /// measuring, plus the requested measurement plugins.
    pub(crate) fn install(self, app: &mut App, title: &str) {
        let measuring = self.frames.is_some();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: title.into(),
                        // Physical 1920x1080 whatever the display scaling, so measurements
                        // compare across machines.
                        resolution: WindowResolution::new(1920, 1080)
                            .with_scale_factor_override(1.0),
                        present_mode: if measuring {
                            PresentMode::AutoNoVsync
                        } else {
                            PresentMode::AutoVsync
                        },
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    // The repository's `assets` folder, also when the example's exe
                    // is started directly rather than through `cargo run`.
                    file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").into(),
                    ..default()
                }),
        );
        if self.fixed_time {
            app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                1.0 / 60.0,
            )));
        }
        if self.perf {
            app.add_plugins((
                RenderDiagnosticsPlugin,
                FrameTimeDiagnosticsPlugin::default(),
                LogDiagnosticsPlugin::default(),
            ));
        }
        if self.single_threaded {
            single_threaded_schedules(app);
        }
        app.insert_resource(self)
            .init_resource::<FrameTimes>()
            .add_systems(Last, count_frames);
    }
}

/// Runs the main app's per-frame schedules and the render app's schedules on
/// one thread each. Pipelined rendering still puts the two apps on separate
/// threads.
///
/// On a CPU with few cores a frame of Bevy is hundreds of small systems, and
/// the multi-threaded executor spends more time waking threads than the
/// systems take. See "The frame-time floor" in `docs/performance.md`.
fn single_threaded_schedules(app: &mut App) {
    let single = |schedule: &mut Schedule| {
        schedule.set_executor(SingleThreadedExecutor::new());
    };
    app.edit_schedule(First, single)
        .edit_schedule(PreUpdate, single)
        .edit_schedule(Update, single)
        .edit_schedule(PostUpdate, single)
        .edit_schedule(Last, single);
    let render = app.sub_app_mut(RenderApp);
    render.edit_schedule(Render, single);
    render.edit_schedule(ExtractSchedule, single);
}

#[derive(Resource, Debug, Default)]
struct FrameTimes(Vec<f32>);

fn count_frames(
    mut commands: Commands,
    args: Res<ExampleArgs>,
    mut times: ResMut<FrameTimes>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(frames) = args.frames else {
        return;
    };
    times.0.push(time.delta_secs() * 1000.0);
    let frame = times.0.len() as u32;
    if let Some((dir, count)) = &args.record
        && frame + count > frames
        && frame <= frames
        && frame % 2 == frames % 2
    {
        let _ = std::fs::create_dir_all(dir);
        let mut shot = commands.spawn(Screenshot::primary_window());
        shot.observe(save_to_disk(format!("{dir}/{frame:05}.png")));
        // The last frame ends the recording once it is saved. A window takes
        // one screenshot a frame, so this one has to end it.
        if frame == frames {
            shot.observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
        }
    }
    if frame != frames {
        return;
    }
    // Skip the first 60 frames: pipeline compilation and the CPU fill.
    let mut measured: Vec<f32> = times.0.iter().skip(60).copied().collect();
    if !measured.is_empty() {
        measured.sort_by(f32::total_cmp);
        let at = |p: f32| measured[((measured.len() - 1) as f32 * p) as usize];
        info!(
            "frame time over {} frames: p50 {:.2} ms, p95 {:.2} ms, worst {:.2} ms",
            measured.len(),
            at(0.5),
            at(0.95),
            measured[measured.len() - 1]
        );
    }
    match &args.screenshot {
        Some(path) => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.clone()))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    },
                );
        }
        // A recording ends with its last frame, above.
        None if args.record.is_some() => {}
        None => {
            exit.write(AppExit::Success);
        }
    }
}

/// A 1.8 m capsule, the height of a person, for judging scale.
pub(crate) fn spawn_person(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Capsule3d::new(0.25, 1.3))),
            MeshMaterial3d(materials.add(Color::srgb(0.15, 0.32, 0.75))),
            Transform::from_translation(at + Vec3::Y * 0.9),
        ))
        .id()
}
