//! Grass in Bevy's other passes: the prepass (SSAO, TAA and anything else
//! that reads depth, normals or motion vectors) and deferred rendering.
//!
//! ```text
//! cargo run --example passes                     # forward, no prepass
//! cargo run --example passes -- --ssao           # SSAO darkens grass roots
//! cargo run --example passes -- --taa            # TAA, no smearing on moving blades
//! cargo run --example passes -- --deferred       # grass in the G-buffer
//! cargo run --example passes -- --show motion    # the prepass texture itself:
//!                                                # depth, normals, motion or
//!                                                # motion-raw (signed, 0.5 still)
//! ```
//!
//! `--no-msaa` turns multisampling off, as every option above does, so a
//! forward picture can be compared with a deferred one. `--storm` blows harder, so blades move fast enough to show smearing; `--pan`
//! moves the camera sideways. With `--fixed-time` and `--screenshot` the frame
//! is the same on every run, which is how the forward and deferred pictures
//! are compared.

mod common;

use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::core_pipeline::prepass::{
    DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass,
};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::pbr::{DefaultOpaqueRendererMethod, ScreenSpaceAmbientOcclusion};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::window::WindowResized;
use bevy_grass::prelude::*;

/// The overlay's shader, built from `SHOW_PREPASS` at startup.
const SHOW_SHADER: Handle<Shader> =
    bevy::asset::uuid_handle!("2b8f6a55-7c1e-4d0a-9f3e-5d6c1a7b9e42");

/// Where the camera looks from and at.
const EYE: Vec3 = Vec3::new(-5.0, 1.7, 8.0);
const TARGET: Vec3 = Vec3::new(0.5, 0.5, 0.0);

fn main() {
    let args = common::ExampleArgs::parse();
    let deferred = args.has("--deferred");
    let mut app = App::new();
    args.clone().install(&mut app, "bevy_grass: passes");
    if deferred {
        app.insert_resource(DefaultOpaqueRendererMethod::deferred());
    }
    app.add_plugins((GrassPlugins, MaterialPlugin::<ShowPrepass>::default()))
        .insert_resource(if args.has("--storm") {
            GrassWind::STORM
        } else {
            GrassWind::GUSTY
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (pan, fit_overlay));
    app.run();
}

#[allow(clippy::needless_pass_by_value)]
fn setup(
    mut commands: Commands,
    args: Res<common::ExampleArgs>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut overlays: ResMut<Assets<ShowPrepass>>,
    mut shaders: ResMut<Assets<Shader>>,
    mut styles: ResMut<Assets<GrassStyle>>,
) {
    let show = args.value("--show").map(|what| match what {
        "depth" => 0,
        "normals" => 1,
        "motion" => 2,
        "motion-raw" => 3,
        other => panic!("--show depth|normals|motion|motion-raw, not {other}"),
    });
    let ssao = args.has("--ssao");
    let taa = args.has("--taa");
    let deferred = args.has("--deferred");

    let mut camera = commands.spawn((
        Camera3d::default(),
        Transform::from_translation(EYE).looking_at(TARGET, Vec3::Y),
    ));
    if ssao || taa || deferred || show.is_some() || args.has("--no-msaa") {
        // Every one of them needs the multisampling off.
        camera.insert(Msaa::Off);
    }
    if ssao {
        camera.insert(ScreenSpaceAmbientOcclusion::default());
    }
    if taa {
        camera.insert(TemporalAntiAliasing::default());
    }
    if deferred {
        camera.insert((DepthPrepass, MotionVectorPrepass, DeferredPrepass));
    }
    if let Some(mode) = show {
        camera.insert((
            DepthPrepass,
            NormalPrepass,
            MotionVectorPrepass,
            Tonemapping::None,
        ));
        let camera = camera.id();
        // A quad just in front of the camera that shows a prepass texture.
        let _ = shaders.insert(
            &SHOW_SHADER,
            Shader::from_wesl(SHOW_PREPASS, "show_prepass.wesl"),
        );
        commands.spawn((
            Mesh3d(meshes.add(Rectangle::new(1.0, 1.0))),
            MeshMaterial3d(overlays.add(ShowPrepass {
                settings: ShowSettings {
                    mode,
                    motion_scale: 40.0,
                },
            })),
            Transform::from_xyz(0.0, 0.0, -0.2),
            Overlay,
            ChildOf(camera),
        ));
    }

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.7, -0.5, 0.0)),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 40.0,
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
    let _ = common::spawn_person(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(1.0, 0.0, 1.0),
    );
    // A crate, so SSAO has something hard-edged to compare grass with.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.55, 0.45, 0.3))),
        Transform::from_xyz(-1.5, 0.5, -1.0).with_rotation(Quat::from_rotation_y(0.5)),
    ));

    commands.spawn(GrassField {
        size: Vec2::splat(60.0),
        style: styles.add(GrassStyle::MEADOW),
        ..default()
    });
    commands.spawn((
        GrassField {
            size: Vec2::new(8.0, 6.0),
            style: styles.add(GrassStyle::TALL_PRAIRIE),
            ..default()
        },
        Transform::from_xyz(4.0, 0.0, -5.0),
    ));
}

/// `--pan` slides the camera sideways and back, about 1 m/s.
#[allow(clippy::needless_pass_by_value)]
fn pan(
    args: Res<common::ExampleArgs>,
    time: Res<Time>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    if !args.has("--pan") {
        return;
    }
    let offset = Vec3::X * (time.elapsed_secs() * 0.4).sin() * 2.5;
    for mut transform in &mut cameras {
        *transform = Transform::from_translation(EYE + offset).looking_at(TARGET + offset, Vec3::Y);
    }
}

/// Marks the quad that shows a prepass texture.
#[derive(Component)]
struct Overlay;

/// Sizes the overlay quad to fill the view.
fn fit_overlay(
    mut resized: MessageReader<WindowResized>,
    windows: Query<&Window>,
    projections: Query<&Projection, With<Camera3d>>,
    mut overlays: Query<&mut Transform, With<Overlay>>,
    mut done: Local<bool>,
) {
    if resized.read().count() == 0 && *done {
        return;
    }
    let (Ok(window), Ok(Projection::Perspective(projection))) =
        (windows.single(), projections.single())
    else {
        return;
    };
    let height = 2.0 * 0.2 * (projection.fov * 0.5).tan();
    let aspect = window.width() / window.height().max(1.0);
    for mut transform in &mut overlays {
        transform.scale = Vec3::new(height * aspect, height, 1.0);
        *done = true;
    }
}

#[derive(Debug, Clone, Copy, ShaderType)]
struct ShowSettings {
    /// 0 depth, 1 normals, 2 motion vectors, 3 motion vectors signed
    /// (0.5 is still), for measuring.
    mode: u32,
    /// Motion vectors are tiny; this scales them into colours.
    motion_scale: f32,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct ShowPrepass {
    #[uniform(0)]
    settings: ShowSettings,
}

impl Material for ShowPrepass {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHOW_SHADER)
    }

    // The quad must not hide the scene from the textures it shows.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

const SHOW_PREPASS: &str = r"
import bevy_pbr::{
    render::forward_io::VertexOutput,
    prepass::utils::{prepass_depth, prepass_normal, prepass_motion_vector},
};

struct ShowSettings {
    mode: u32,
    motion_scale: f32,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> settings: ShowSettings;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if settings.mode == 0u {
        // Reversed depth: near is 1. Stretched so 1–60 m spans the range.
        let depth = prepass_depth(in.position, 0u);
        return vec4<f32>(vec3<f32>(pow(depth * 10.0, 0.4)), 1.0);
    }
    if settings.mode == 1u {
        return vec4<f32>(prepass_normal(in.position, 0u) * 0.5 + vec3<f32>(0.5), 1.0);
    }
    let motion = prepass_motion_vector(in.position, 0u) * settings.motion_scale;
    if settings.mode == 3u {
        return vec4<f32>(motion + vec2<f32>(0.5), 0.0, 1.0);
    }
    return vec4<f32>(abs(motion.x), abs(motion.y), 0.0, 1.0);
}
";
