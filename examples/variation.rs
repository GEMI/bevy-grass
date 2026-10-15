//! Variation, one kind at a time: a wild style with each kind of variation
//! switched on and off, to see what each adds.
//!
//! The field's style is loaded from its `.grass.ron` file in
//! `assets/grass_styles`, so editing the file and saving it changes the
//! running example; the switches apply on top.
//!
//! * 1–3 pick the style: summer wild, autumn wild, wildflower meadow.
//! * P: other plants (`species`): seed heads, flowers, clover, broad leaves.
//! * T: tufts and bare patches (`tufts`, `bare_patches`).
//! * B: blade shape (`twist`, `droop`, `fold`; heads belong to the plants).
//! * C: colour variation (`patch_amount`, `dry_tips`, `dead_blades`).
//! * S: grass shadows.
//!
//! WASD and the mouse fly the camera (hold the left button to look).
//! `--arpg` uses DEMO ARPG GAME's camera instead: orthographic, 30° down, 45° yaw.
//! For screenshots, `--style N` (1–3) and `--no-plants`, `--no-tufts`,
//! `--no-shape` and `--no-colour` set the start.
//!
//! ```text
//! cargo run --example variation
//! cargo run --example variation -- --arpg
//! cargo run --example variation -- --fixed-time --no-tufts --screenshot shots/variation.png
//! ```

mod common;

use std::borrow::Cow;

use bevy::camera::ScalingMode;
use bevy::camera_controller::free_camera::{FreeCamera, FreeCameraPlugin};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_grass::prelude::*;

const STYLES: [&str; 3] = ["summer_wild", "autumn_wild", "wildflower_meadow"];

/// Which kinds of variation are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Switches {
    plants: bool,
    tufts: bool,
    shape: bool,
    colour: bool,
}

/// How the example starts, from the command line.
#[derive(Resource, Clone, Copy)]
struct Start {
    arpg: bool,
    file: usize,
    switches: Switches,
}

/// The style file shown and the switches applied to it.
#[derive(Resource)]
struct Shown {
    files: Vec<Handle<GrassStyle>>,
    file: usize,
    switches: Switches,
    /// The field's style: the file's with the switches applied.
    style: Handle<GrassStyle>,
    dirty: bool,
}

fn main() {
    let args = common::ExampleArgs::parse();
    let start = Start {
        arpg: args.has("--arpg"),
        file: args
            .value("--style")
            .and_then(|n| n.parse::<usize>().ok())
            .map_or(0, |n| n.clamp(1, STYLES.len()) - 1),
        switches: Switches {
            plants: !args.has("--no-plants"),
            tufts: !args.has("--no-tufts"),
            shape: !args.has("--no-shape"),
            colour: !args.has("--no-colour"),
        },
    };
    let mut app = App::new();
    args.install(&mut app, "bevy_grass: variation");
    app.add_plugins((GrassPlugin::default(), FreeCameraPlugin))
        .insert_resource(start)
        .add_systems(Startup, setup)
        .add_systems(Update, (switch, refresh, title).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut styles: ResMut<Assets<GrassStyle>>,
    start: Res<Start>,
) {
    if start.arpg {
        let rotation =
            Quat::from_euler(EulerRot::YXZ, 45f32.to_radians(), -30f32.to_radians(), 0.0);
        commands.spawn((
            Camera3d::default(),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: 14.64,
                },
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_translation(rotation * Vec3::new(0.0, 0.0, 40.0))
                .with_rotation(rotation),
        ));
    } else {
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 3.5, 14.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
            FreeCamera::default(),
        ));
    }
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.5, -0.5, 0.0)),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 60.0,
            ..default()
        }
        .build(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.20, 0.15, 0.09),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));
    let _ = common::spawn_person(&mut commands, &mut meshes, &mut materials, Vec3::ZERO);

    let style = styles.add(GrassStyle::default());
    commands.spawn(GrassField {
        size: Vec2::splat(80.0),
        style: style.clone(),
        ..default()
    });
    commands.insert_resource(Shown {
        files: STYLES
            .iter()
            .map(|name| assets.load(format!("grass_styles/{name}.grass.ron")))
            .collect(),
        file: start.file,
        switches: start.switches,
        style,
        dirty: true,
    });
}

fn switch(
    keys: Res<ButtonInput<KeyCode>>,
    mut shown: ResMut<Shown>,
    mut shadows: ResMut<GrassShadows>,
) {
    let before = (shown.file, shown.switches);
    for (key, file) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
        .into_iter()
        .zip(0..)
    {
        if keys.just_pressed(key) {
            shown.file = file;
        }
    }
    let switches = &mut shown.switches;
    for (key, on) in [
        (KeyCode::KeyP, &mut switches.plants),
        (KeyCode::KeyT, &mut switches.tufts),
        (KeyCode::KeyB, &mut switches.shape),
        (KeyCode::KeyC, &mut switches.colour),
    ] {
        if keys.just_pressed(key) {
            *on = !*on;
        }
    }
    if (shown.file, shown.switches) != before {
        shown.dirty = true;
    }
    if keys.just_pressed(KeyCode::KeyS) {
        *shadows = if *shadows == GrassShadows::Off {
            GrassShadows::Near
        } else {
            GrassShadows::Off
        };
    }
}

/// Rebuilds the field's style when a switch changes or the file does.
fn refresh(
    mut shown: ResMut<Shown>,
    mut events: MessageReader<AssetEvent<GrassStyle>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let file = shown.files[shown.file].id();
    if events
        .read()
        .any(|event| event.is_loaded_with_dependencies(file) || event.is_modified(file))
    {
        shown.dirty = true;
    }
    if !shown.dirty {
        return;
    }
    let Some(base) = styles.get(file) else {
        return;
    };
    let style = apply(base, shown.switches);
    if let Some(mut field_style) = styles.get_mut(&shown.style) {
        *field_style = style;
        shown.dirty = false;
    }
}

/// `base` with the kinds of variation that are off taken out.
fn apply(base: &GrassStyle, on: Switches) -> GrassStyle {
    let mut style = base.clone();
    if !on.plants {
        style.species = Cow::Borrowed(&[]);
    }
    if !on.tufts {
        style.tufts = 0.0;
        style.bare_patches = 0.0;
    }
    if !on.shape {
        let flat = |style: &mut GrassStyle| {
            style.twist = 0.0;
            style.droop = 0.0;
            style.fold = 0.0;
        };
        flat(&mut style);
        let species: Vec<GrassSpecies> = style
            .species
            .iter()
            .cloned()
            .map(|mut species| {
                flat(&mut species.style);
                species
            })
            .collect();
        style.species = species.into();
    }
    if !on.colour {
        style.patch_amount = 0.0;
        style.dry_tips = 0.0;
        style.dead_blades = 0.0;
    }
    style
}

fn title(shown: Res<Shown>, mut windows: Query<&mut Window>) {
    if !shown.is_changed() {
        return;
    }
    let mark = |on: bool| if on { "on" } else { "off" };
    let s = shown.switches;
    let text = format!(
        "bevy_grass: variation — {} (1-3) | plants {} (P) | tufts {} (T) | shape {} (B) | colour {} (C)",
        STYLES[shown.file],
        mark(s.plants),
        mark(s.tufts),
        mark(s.shape),
        mark(s.colour),
    );
    for mut window in &mut windows {
        window.title.clone_from(&text);
    }
}
