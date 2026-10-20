//! GPU-driven, wind-animated tall grass for Bevy.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_grass::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, GrassPlugins))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands, mut styles: ResMut<Assets<GrassStyle>>) {
//!     commands.spawn(GrassField::new(
//!         Vec2::splat(64.0),
//!         GrassStyle::MEADOW,
//!         &mut styles,
//!     ));
//! }
//! ```
//!
//! [`GrassPlugins`] is a plugin group: [`GrassPlugin`] (the grass itself),
//! [`FarGrassPlugin`] (the terrain material for ground past the last blade)
//! and [`GrassStyleFilePlugin`] (`.grass.ron` files and the presets as
//! assets). Turn one off with `GrassPlugins.build().disable::<FarGrassPlugin>()`,
//! or add only the ones you want. `GrassDebugPlugin` (cargo feature
//! `debug`) is always added by hand.
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
//! blades (seed heads, flowers, clover, broad leaves, ferns: [`GrassSpecies`]),
//! tufts and bare patches, blades that twist, droop and fold, and colour
//! patches, dry tips and dead blades. Every kind is a setting of
//! [`GrassStyle`], off at 0, and costs nothing while no field uses it;
//! `GrassStyle::WILD` and the forest-floor presets use them all.

mod api;
mod clock;
#[cfg(feature = "debug")]
mod debug;
mod far;
mod fields;
mod ground;
#[cfg(test)]
mod mirror;
mod packed;
mod physics;
mod presets;
mod render;
mod settings;
mod shader_model;
mod stats;
mod style;
mod style_file;
mod sun;
mod trample;
pub mod wind;

pub use api::{
    GrassBlade, GrassCamera, GrassField, GrassFieldOverrides, GrassGround, GrassInteractor,
    GrassInteractorFootprint, GrassInteractorSpread, GrassSpecies, GrassStyle, GrassSun,
    GrassVariation, GroundTint, NoGrass,
};
#[cfg(feature = "debug")]
pub use debug::{GrassDebug, GrassDebugPlugin, GrassGizmos};
pub use far::{FarGrass, FarGrassMaterial, FarGrassPlugin, FarGrassUniform};
pub use physics::{GrassColliderBounds, GrassColliderSystems, GrassInteractorFromCollider};
pub use settings::{GrassQuality, GrassSettings, GrassShadows};
pub use stats::{GpuStats, GrassDiagnostics, GrassStats, TableStats};
pub use style_file::{GrassStyleFileError, GrassStyleFilePlugin, GrassStyleLoader};
pub use wind::{GrassWind, GrassWindFrame, GrassWindPlugin, GrassWindSystems};

use bevy::app::{App, Plugin, PluginGroup, PluginGroupBuilder, PostUpdate, Update};
use bevy::asset::{AssetApp, embedded_asset, load_embedded_asset};
use bevy::ecs::schedule::common_conditions::resource_changed;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::Res;
use bevy::log::warn;
use bevy::shader::load_shader_library;

/// The things a game needs, in one import.
pub mod prelude {
    pub use crate::{
        FarGrass, FarGrassMaterial, FarGrassPlugin, GrassBlade, GrassCamera, GrassColliderBounds,
        GrassColliderSystems, GrassField, GrassFieldOverrides, GrassGround, GrassInteractor,
        GrassInteractorFootprint, GrassInteractorFromCollider, GrassInteractorSpread, GrassPlugin,
        GrassPlugins, GrassQuality, GrassSettings, GrassShadows, GrassSpecies, GrassStats,
        GrassStyle, GrassStyleFilePlugin, GrassSun, GrassSystems, GrassVariation, GrassWind,
        GrassWindFrame, GrassWindPlugin, GrassWindSystems, GroundTint, NoGrass,
    };
    #[cfg(feature = "debug")]
    pub use crate::{GrassDebug, GrassDebugPlugin};
}

/// The plugin's systems in the main world, all in `PostUpdate`, in this order.
///
/// Fields, styles, density masks and heightmaps are turned into the tables
/// the GPU draws from once per change, not per frame: [`DetectChanges`] notices
/// what changed and [`Rebuild`] updates the tables. Both run after Bevy has
/// propagated transforms and visibility and flushed asset events, so
/// they see this frame's `GlobalTransform`, `InheritedVisibility` and
/// `AssetEvent`s.
///
/// Order your own systems against them when what you do must show up this
/// frame.
///
/// - **Fields** (their component, transform, visibility, layers): a system
///   that changes them `.before(GrassSystems::DetectChanges)` is seen this
///   frame.
/// - **Assets** (a style, a density mask, a heightmap): they are noticed
///   through their `AssetEvent`, which Bevy sends in
///   [`AssetEventSystems`](bevy::asset::AssetEventSystems), so a system that
///   edits one `.before(AssetEventSystems)` is seen this frame.
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy::asset::AssetEventSystems;
/// # use bevy_grass::prelude::*;
/// # fn move_the_meadow() {}
/// # fn paint_density_mask() {}
/// # let mut app = App::new();
/// app.add_systems(PostUpdate, move_the_meadow.before(GrassSystems::DetectChanges))
///     .add_systems(PostUpdate, paint_density_mask.before(AssetEventSystems));
/// ```
///
/// Systems in `Update` need no ordering: they run before all of these.
/// A system `.after(GrassSystems::Rebuild)` sees the tables as this frame's
/// changes left them; its own changes show up the next frame.
///
/// [`DetectChanges`]: GrassSystems::DetectChanges
/// [`Rebuild`]: GrassSystems::Rebuild
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GrassSystems {
    /// Notices changed fields (their component, position, visibility or
    /// render layers), styles, masks, heightmaps and meshes, removed fields,
    /// and fields Bevy cannot draw as asked (a tilt, a layer above 31), bakes
    /// mesh grounds, and marks the tables for an update.
    DetectChanges,
    /// Updates the tables from what changed: a full rebuild for changed
    /// fields, styles and images, or only the entries for a field shown,
    /// hidden or moved to other render layers. A field whose style or images
    /// have not loaded yet waits.
    Rebuild,
}

/// Everything a game needs to draw grass, as a plugin group. Add it after
/// `DefaultPlugins`:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// App::new()
///     .add_plugins((DefaultPlugins, GrassPlugins))
///     .run();
///
/// // Start from other settings, and leave out the far-grass material:
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(
///         GrassPlugins
///             .set(GrassPlugin {
///                 settings: GrassSettings::MEDIUM,
///                 ..default()
///             })
///             .disable::<FarGrassPlugin>(),
///     )
///     .run();
/// ```
///
/// - [`GrassPlugin`]: fields, styles, wind, interaction and the rendering.
///   The only one that is required.
/// - [`FarGrassPlugin`]: [`FarGrassMaterial`], for ground past the last blade.
/// - [`GrassStyleFilePlugin`]: `.grass.ron` files, and the presets as assets.
///
/// `GrassDebugPlugin` (cargo feature `debug`)
/// is not in the group; add it by hand.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrassPlugins;

impl PluginGroup for GrassPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(GrassPlugin::default())
            .add(FarGrassPlugin)
            .add(GrassStyleFilePlugin)
    }
}

/// Registers grass rendering: fields, styles, wind, interaction, settings and
/// the passes that draw them. Part of [`GrassPlugins`]; add it alone to leave
/// out the far-grass material or the style files.
///
/// The settings are the [`GrassSettings`] resource from then on; change that
/// to change them at runtime. The built-in wind can be left out:
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_grass::prelude::*;
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(GrassPlugins.set(GrassPlugin {
///         wind: false,
///         ..default()
///     }))
///     // Write `GrassWindFrame` from your own wind plugin.
///     .run();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct GrassPlugin {
    /// How much grass is drawn, how far, and at what cost: a tier such as
    /// [`GrassSettings::MEDIUM`], with any setting changed.
    pub settings: GrassSettings,
    /// Add the built-in [`GrassWindPlugin`]. Turn it off to drive the grass
    /// with another wind model that writes [`GrassWindFrame`].
    pub wind: bool,
}

impl Default for GrassPlugin {
    fn default() -> Self {
        Self {
            settings: GrassSettings::default(),
            wind: true,
        }
    }
}

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "shaders/blade.wesl");
        load_shader_library!(app, "shaders/common.wesl");
        load_shader_library!(app, "shaders/wind.wesl");
        load_shader_library!(app, "shaders/look.wesl");
        load_shader_library!(app, "shaders/far.wesl");
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
        register_types(app);
        app.init_resource::<GrassWind>()
            .init_resource::<GrassWindFrame>()
            .init_resource::<clock::GrassClock>()
            .add_systems(Update, clock::tick)
            .init_resource::<sun::ShadowSetupReport>()
            .insert_resource(self.settings)
            .add_plugins((stats::GrassDiagnosticsPlugin, fields::plugin))
            .add_systems(
                PostUpdate,
                (
                    api::warn_oversized_interactors,
                    warn_about_settings.run_if(resource_changed::<GrassSettings>),
                    sun::check_shadow_setup.after(GrassSystems::Rebuild),
                ),
            )
            .add_plugins(render::GrassRenderPlugin { shaders });
        physics::plugin(app);
        if self.wind {
            app.add_plugins(GrassWindPlugin);
        }
    }
}

/// Registers everything a game puts in a scene, an inspector or a save file,
/// with the type data to match: components with `ReflectComponent`,
/// resources with `ReflectResource`, the style asset with `ReflectAsset`.
fn register_types(app: &mut App) {
    app.init_asset::<GrassStyle>()
        .register_asset_reflect::<GrassStyle>()
        .register_type::<GrassBlade>()
        .register_type::<GrassVariation>()
        .register_type::<GroundTint>()
        .register_type::<GrassSpecies>()
        .register_type::<GrassField>()
        .register_type::<GrassFieldOverrides>()
        .register_type::<GrassGround>()
        .register_type::<GrassInteractor>()
        .register_type::<GrassInteractorSpread>()
        .register_type::<GrassInteractorFootprint>()
        .register_type::<GrassInteractorFromCollider>()
        .register_type::<GrassColliderBounds>()
        .register_type::<NoGrass>()
        .register_type::<GrassSun>()
        .register_type::<GrassCamera>()
        .register_type::<GrassWind>()
        .register_type::<GrassWindFrame>()
        .register_type::<GrassSettings>()
        .register_type::<GrassQuality>()
        .register_type::<GrassShadows>();
}

/// Says which settings were out of range, once per change. The render world
/// clamps them either way.
fn warn_about_settings(settings: Res<GrassSettings>) {
    for problem in settings.problems() {
        warn!("bevy_grass: GrassSettings::{problem}");
    }
}

#[cfg(test)]
mod tests;
