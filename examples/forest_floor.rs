//! The forest-floor presets side by side, loaded from their `.grass.ron`
//! files: a moss carpet, fern clumps, the whole floor mixed, and autumn
//! bracken. A 1.8 m figure stands in front for scale. The presets come from
//! `GrassStyle::FOREST`, so a new one appears here without editing the example.
//!
//! They are built from the settings a style already has (`width`, `fold`,
//! `droop`, `tufts`, `clustering` and species). "Ferns" are clumps of wide,
//! arching, folded blades, not branched plants.
//!
//! ```text
//! cargo run --example forest_floor
//! cargo run --example forest_floor -- --only forest_ferns
//! cargo run --example forest_floor -- --fixed-time --screenshot shots/forest_floor.png
//! ```

mod common;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_grass::prelude::*;

/// Side of each preset's patch, and the gap between patches, in metres.
const PATCH: f32 = 7.0;
const GAP: f32 = 1.5;

/// The forest presets' file names, in the group's order.
fn forest_presets() -> Vec<String> {
    GrassStyle::FOREST
        .iter()
        .map(|style| {
            let name = GrassStyle::presets()
                .find(|(_, preset)| preset == style)
                .map(|(name, _)| name)
                .expect("every forest style is a preset");
            name.to_ascii_lowercase()
        })
        .collect()
}

/// A name under a patch: where on the ground its front corner is.
#[derive(Component)]
struct Label {
    corner: Vec3,
}

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: forest floor");
    app.add_plugins(GrassPlugins)
        .insert_resource(ClearColor(Color::srgb(0.10, 0.08, 0.05)))
        .add_systems(Startup, setup)
        .add_systems(Update, place_labels)
        .run();
}

#[allow(clippy::needless_pass_by_value)]
fn setup(
    mut commands: Commands,
    args: Res<common::ExampleArgs>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.6, -0.55, 0.0)),
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
            base_color: Color::srgb(0.12, 0.09, 0.05),
            perceptual_roughness: 0.95,
            ..default()
        })),
    ));

    let presets = forest_presets();
    let style = |name: &str| assets.load(format!("grass_styles/{name}.grass.ron"));

    if let Some(name) = args.value("--only") {
        assert!(
            presets.iter().any(|preset| preset == name),
            "unknown preset {name:?}; the presets are {}",
            presets.join(", ")
        );
        commands.spawn(GrassField {
            size: Vec2::splat(12.0),
            style: style(name),
            ..default()
        });
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 1.4, 4.5).looking_at(Vec3::new(0.0, 0.2, 0.0), Vec3::Y),
        ));
        let _ = common::spawn_person(
            &mut commands,
            &mut meshes,
            &mut materials,
            Vec3::new(2.6, 0.0, 2.0),
        );
        return;
    }

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 12.0, 16.0).looking_at(Vec3::new(0.0, 0.0, -1.0), Vec3::Y),
    ));
    let _ = common::spawn_person(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(0.0, 0.0, 9.0),
    );

    let columns = 2;
    for (index, name) in presets.iter().enumerate() {
        let row = index / columns;
        let column = index % columns;
        let width = columns as f32 * (PATCH + GAP) - GAP;
        let x = -width * 0.5 + PATCH * 0.5 + column as f32 * (PATCH + GAP);
        let z = -(row as f32) * (PATCH + GAP);
        commands.spawn((
            GrassField {
                size: Vec2::splat(PATCH),
                style: style(name),
                ..default()
            },
            Transform::from_xyz(x, 0.0, z),
        ));
        // The patch is a square of ground: its nearest corner is half a
        // diagonal toward the camera.
        let corner = Vec3::new(x, 0.0, z) - Vec3::new(1.0, 0.0, 1.0).normalize() * (PATCH * 0.5);
        commands
            .spawn((
                Label { corner },
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(240.0),
                    justify_content: JustifyContent::Center,
                    display: Display::None,
                    ..default()
                },
            ))
            .with_child((
                Text::new(name.clone()),
                TextFont {
                    font_size: 22.0.into(),
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.93, 0.85)),
                TextLayout::justify(Justify::Center),
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.72)),
            ));
    }
}

/// Puts each name under its patch, from where the camera sees its corner.
fn place_labels(
    camera: Single<(&Camera, &GlobalTransform)>,
    mut labels: Query<(&Label, &mut Node)>,
) {
    let (camera, transform) = *camera;
    for (label, mut node) in &mut labels {
        let Ok(at) = camera.world_to_viewport(transform, label.corner) else {
            continue;
        };
        node.display = Display::Flex;
        node.left = Val::Px(at.x - 120.0);
        node.top = Val::Px(at.y + 4.0);
    }
}
