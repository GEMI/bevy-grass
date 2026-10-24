//! Far grass in a terrain material of the game's own: a ground with its own
//! shader (`custom_terrain.wesl`) that calls `grass_far_color`, as a game
//! with its own terrain shader would, instead of using `FarGrassMaterial`.
//!
//! The material holds a `FarGrassUniform` and the gust texture
//! (`FarGrass::GUSTS`); a system rewrites the uniform every frame from the
//! wind and the grass radius (`FarGrass::uniform`), so the gusts roll across
//! the far ground in step with the blades.
//!
//! ```text
//! cargo run --release --example custom_terrain
//! cargo run --release --example custom_terrain -- --fixed-time --screenshot shots/custom_terrain.png
//! ```

mod common;

use bevy::asset::uuid_handle;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy_grass::FarGrassUniform;
use bevy_grass::prelude::*;

/// The terrain shader, added from the file next to this example.
const TERRAIN_SHADER: Handle<Shader> = uuid_handle!("4f1e2a9c-7b3d-4c6e-8a5f-2d9b1c0e7a64");

fn main() {
    let mut app = App::new();
    common::ExampleArgs::parse().install(&mut app, "bevy_grass: custom terrain material");
    app.add_plugins((GrassPlugins, MaterialPlugin::<Terrain>::default()))
        .insert_resource(TerrainGrass(FarGrass::new(&GrassStyle::MEADOW)))
        .add_systems(Startup, setup)
        .add_systems(Update, follow_the_grass.after(GrassWindSystems::Sample))
        .run();
}

/// The game's terrain material.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
struct Terrain {
    #[uniform(0)]
    far: FarGrassUniform,
    #[texture(1)]
    #[sampler(2)]
    gusts: Handle<Image>,
    #[uniform(3)]
    look: TerrainLook,
}

/// The terrain's own settings. Mirrors `Terrain` in `custom_terrain.wesl`.
#[derive(ShaderType, Debug, Clone, Copy)]
struct TerrainLook {
    soil: Vec4,
    sun: Vec4,
}

impl Material for Terrain {
    fn fragment_shader() -> ShaderRef {
        TERRAIN_SHADER.into()
    }
}

/// What the far grass on the terrain looks like.
#[derive(Resource)]
struct TerrainGrass(FarGrass);

/// Keeps the terrain's far grass on this frame's wind and radius.
fn follow_the_grass(
    grass: Res<TerrainGrass>,
    wind: Res<GrassWindFrame>,
    settings: Res<GrassSettings>,
    mut terrains: ResMut<Assets<Terrain>>,
) {
    let far = grass.0.uniform(&wind, &settings);
    for (_, terrain) in terrains.iter_mut() {
        terrain.far = far;
    }
}

fn setup(
    mut commands: Commands,
    mut shaders: ResMut<Assets<Shader>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut terrains: ResMut<Assets<Terrain>>,
    mut styles: ResMut<Assets<GrassStyle>>,
    grass: Res<TerrainGrass>,
) {
    shaders
        .insert(
            &TERRAIN_SHADER,
            Shader::from_wesl(include_str!("custom_terrain.wesl"), "custom_terrain.wesl"),
        )
        .expect("a fixed handle is always valid");
    let sun = Vec3::new(4.0, 8.0, 3.0);
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.0, 12.0).looking_at(Vec3::new(0.0, 0.0, -40.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(sun).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(1000.0, 1000.0))),
        MeshMaterial3d(
            terrains.add(Terrain {
                // `follow_the_grass` brings it up to date every frame.
                far: grass
                    .0
                    .uniform(&GrassWindFrame::default(), &GrassSettings::default()),
                gusts: FarGrass::GUSTS,
                look: TerrainLook {
                    soil: LinearRgba::from(Color::srgb(0.2, 0.15, 0.09)).to_vec4(),
                    sun: sun.normalize().extend(0.0),
                },
            }),
        ),
    ));
    commands.spawn(GrassField::new(
        Vec2::splat(400.0),
        GrassStyle::MEADOW,
        &mut styles,
    ));
}
