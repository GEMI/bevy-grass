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
//!
//! Any other `--flag` is kept for the example itself (`ExampleArgs::has`).

#![allow(dead_code)]

use std::time::Duration;

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};

#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct ExampleArgs {
    pub(crate) frames: Option<u32>,
    pub(crate) screenshot: Option<String>,
    pub(crate) fixed_time: bool,
    pub(crate) perf: bool,
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
                other => args.flags.push(other.to_owned()),
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
        app.insert_resource(self)
            .init_resource::<FrameTimes>()
            .add_systems(Last, count_frames);
    }
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
            MeshMaterial3d(materials.add(Color::srgb(0.7, 0.2, 0.15))),
            Transform::from_translation(at + Vec3::Y * 0.9),
        ))
        .id()
}
