//! GPU-driven, wind-animated tall grass for Bevy.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_grass::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, GrassPlugin::default()))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
//!     commands.spawn(GrassField {
//!         size: Vec2::splat(64.0),
//!         style: styles.add(GrassStyle::MEADOW),
//!         ..default()
//!     });
//! }
//! ```
//!
//! Every camera draws grass unless it carries [`NoGrass`]. A camera with a
//! depth, normal or motion-vector prepass gets grass in it too, so SSAO, TAA
//! and anything else reading the prepass see it; a deferred camera (with
//! `DefaultOpaqueRendererMethod::deferred()`) draws grass into its G-buffer.
//!
//! Styles are assets: build one in code from a preset, or load a
//! `.grass.ron` file ([`GrassStyleLoader`]). [`FarGrassMaterial`] carries the
//! grass's colour onto the ground beyond the last blade.
//!
//! A style can vary its grass the way real fields do: other plants among the
//! blades (seed heads, flowers, clover, broad leaves: [`GrassSpecies`](api::GrassSpecies)),
//! tufts and bare patches, blades that twist, droop and fold, and colour
//! patches, dry tips and dead blades. Every kind is a setting of
//! [`GrassStyle`], off at 0, and costs nothing while no field uses it;
//! `GrassStyle::WILD` uses them all.
//!
//! The design, and what each milestone adds, is in
//! `bevy-grass-architecture-plan.md` at the repository root.

pub mod api;
mod far;
mod fields;
mod mirror;
mod render;
mod style_file;

pub use far::{FarGrass, FarGrassMaterial, FarGrassUniform};
pub use style_file::{GrassStyleFileError, GrassStyleLoader};

use bevy::app::{App, Plugin, PostUpdate, PreStartup, Update};
use bevy::asset::{AssetApp, embedded_asset, load_embedded_asset};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::reflect::Reflect;
use bevy::shader::load_shader_library;
use bevy::transform::TransformSystems;

use crate::api::{GrassField, GrassInteractor, GrassStyle, NoGrass, Wind};

/// The things a game needs, in one import.
pub mod prelude {
    pub use crate::GrassPlugin;
    pub use crate::GrassQuality;
    pub use crate::GrassShadows;
    pub use crate::api::{
        GrassField, GrassGround, GrassInteractor, GrassSpecies, GrassStyle, NoGrass, Wind,
    };
    pub use crate::far::{FarGrass, FarGrassMaterial};
}

/// Quality tier: sets the grass radius, density, LOD distances and how many
/// blades each camera's buffers hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
pub enum GrassQuality {
    /// Steam Deck and integrated GPUs.
    Low,
    /// GTX 1660 / RX 5600 class.
    Medium,
    /// RTX 3060 / RX 6700 class.
    #[default]
    High,
    /// RTX 4070 class and up.
    Ultra,
}

impl GrassQuality {
    /// Visible blades the tier is budgeted for.
    pub const fn instance_capacity(self) -> u32 {
        match self {
            Self::Low => 80_000,
            Self::Medium => 250_000,
            Self::High => 600_000,
            Self::Ultra => 1_200_000,
        }
    }

    /// How far grass reaches from a perspective camera, in metres. LOD 1
    /// starts at 20% of it and LOD 2 at 50%.
    pub const fn radius(self) -> f32 {
        match self {
            Self::Low => 40.0,
            Self::Medium => 70.0,
            Self::High => 100.0,
            Self::Ultra => 150.0,
        }
    }

    /// Which blades cast shadows: none at Low, `Near` at Medium and High,
    /// `Full` at Ultra.
    pub const fn shadows(self) -> GrassShadows {
        match self {
            Self::Low => GrassShadows::Off,
            Self::Medium | Self::High => GrassShadows::Near,
            Self::Ultra => GrassShadows::Full,
        }
    }

    /// Multiplier on every style's density.
    pub const fn density_scale(self) -> f32 {
        match self {
            Self::Low => 0.6,
            Self::Medium => 0.8,
            Self::High | Self::Ultra => 1.0,
        }
    }

    /// The level of detail orthographic cameras draw everything at; every
    /// blade is the same size on their screen, so distance cannot pick it.
    ///
    /// LOD 1 (3 rows) at DEMO ARPG GAME's camera, where a 1 m blade is about 64 px
    /// tall, could not be told from LOD 0 at 1:1 and drew 9% faster, so only
    /// Ultra pays for 7 rows.
    pub const fn orthographic_lod(self) -> u32 {
        match self {
            Self::Ultra => 0,
            _ => 1,
        }
    }
}

/// Which blades cast shadows from directional lights. Grass always
/// receives shadows; casting is the expensive part.
///
/// Also a resource: change it at any time, from a settings menu for
/// instance, and the next frame draws with the new mode.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// fn toggle_grass_shadows(mut shadows: ResMut<GrassShadows>) {
///     *shadows = if *shadows == GrassShadows::Off {
///         GrassShadows::Near
///     } else {
///         GrassShadows::Off
///     };
/// }
/// ```
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default, Reflect)]
pub enum GrassShadows {
    /// No grass shadows.
    Off,
    /// Full-detail blades (LOD 0) into the first shadow cascade only: the
    /// shadows a viewer can resolve, at a fraction of the cost.
    #[default]
    Near,
    /// LOD 0 and LOD 1 blades into the first two cascades.
    Full,
}

/// Registers grass rendering. Add it after `DefaultPlugins`.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrassPlugin {
    /// Quality tier.
    pub quality: GrassQuality,
    /// Overrides the tier's per-camera instance capacity. Blades beyond it
    /// are dropped with a warning.
    pub instance_capacity: Option<u32>,
    /// Overrides the tier's shadow mode at startup. The mode is the
    /// [`GrassShadows`] resource afterwards.
    pub shadows: Option<GrassShadows>,
    /// Most [`GrassInteractor`]s a camera stamps a frame, nearest to the
    /// camera first. 64 when not set; a game with a horde wants more, and 512
    /// costs about the same as 64.
    pub max_interactors: Option<u32>,
}

/// Interactors stamped a frame when `GrassPlugin::max_interactors` is not set.
const DEFAULT_MAX_INTERACTORS: u32 = 64;

/// Slices of visible chunks one camera can generate in a frame. A dense chunk
/// is several slices of 512 candidates (`generate.wesl`); the bench peaks at
/// 739 at Ultra, so 8,192 leaves room for styles ten times as dense.
const MAX_VISIBLE_CHUNKS: u32 = 8192;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/blade.wesl");
        load_shader_library!(app, "shaders/common.wesl");
        load_shader_library!(app, "shaders/look.wesl");
        load_shader_library!(app, "shaders/far.wesl");
        embedded_asset!(app, "shaders/far_ground.wesl");
        embedded_asset!(app, "shaders/grass.wesl");
        embedded_asset!(app, "shaders/grass_prepass.wesl");
        embedded_asset!(app, "shaders/grass_shadow.wesl");
        embedded_asset!(app, "shaders/generate.wesl");
        embedded_asset!(app, "shaders/trample.wesl");
        let shaders = render::resources::GrassShaders {
            main: load_embedded_asset!(app, "shaders/grass.wesl"),
            prepass: load_embedded_asset!(app, "shaders/grass_prepass.wesl"),
            shadow: load_embedded_asset!(app, "shaders/grass_shadow.wesl"),
            generate: load_embedded_asset!(app, "shaders/generate.wesl"),
            trample: load_embedded_asset!(app, "shaders/trample.wesl"),
        };
        let settings = render::view::GrassSettings {
            capacity: self
                .instance_capacity
                .unwrap_or(self.quality.instance_capacity()),
            radius: self.quality.radius(),
            density_scale: self.quality.density_scale(),
            orthographic_lod: self.quality.orthographic_lod(),
            max_visible_chunks: MAX_VISIBLE_CHUNKS,
            max_interactors: self.max_interactors.unwrap_or(DEFAULT_MAX_INTERACTORS),
        };

        app.init_asset::<GrassStyle>()
            .init_asset_loader::<GrassStyleLoader>()
            .register_type::<GrassField>()
            .register_type::<GrassInteractor>()
            .register_type::<NoGrass>()
            .register_type::<Wind>()
            .init_resource::<Wind>()
            .insert_resource(self.shadows.unwrap_or(self.quality.shadows()))
            .register_type::<GrassShadows>()
            .init_resource::<fields::FieldTables>()
            .init_resource::<fields::TablesDirty>()
            .add_systems(
                PostUpdate,
                (fields::detect_changes, fields::rebuild)
                    .chain()
                    .after(TransformSystems::Propagate),
            )
            .add_plugins(bevy::pbr::MaterialPlugin::<FarGrassMaterial>::default())
            .add_systems(PreStartup, far::add_gust_texture)
            .add_systems(Update, far::sync_wind)
            .add_plugins(render::GrassRenderPlugin { settings, shaders });
    }
}
