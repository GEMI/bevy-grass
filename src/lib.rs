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
//! blades (seed heads, flowers, clover, broad leaves: [`GrassSpecies`]),
//! tufts and bare patches, blades that twist, droop and fold, and colour
//! patches, dry tips and dead blades. Every kind is a setting of
//! [`GrassStyle`], off at 0, and costs nothing while no field uses it;
//! `GrassStyle::WILD` uses them all.
//!
//! The design, and what each milestone adds, is in
//! `bevy-grass-architecture-plan.md` at the repository root.

pub mod api;
#[cfg(feature = "debug")]
mod debug;
mod far;
mod fields;
mod mirror;
mod packed;
mod render;
mod settings;
mod stats;
mod style;
mod style_file;
mod sun;

#[cfg(feature = "debug")]
pub use debug::{GrassDebug, GrassDebugPlugin, GrassGizmos};
pub use far::{FarGrass, FarGrassMaterial, FarGrassUniform};
pub use settings::{GrassQuality, GrassSettings, GrassShadows};
pub use stats::{GpuStats, GrassDiagnostics, GrassStats, TableStats};
pub use style_file::{GrassStyleFileError, GrassStyleLoader};

use bevy::app::{App, Plugin, PostUpdate, PreStartup, Update};
use bevy::asset::{AssetApp, embedded_asset, load_embedded_asset};
use bevy::ecs::schedule::common_conditions::resource_changed;
use bevy::ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy::ecs::system::Res;
use bevy::log::warn;
use bevy::shader::load_shader_library;

use crate::api::{
    GrassBlade, GrassCamera, GrassField, GrassGround, GrassInteractor, GrassSpecies, GrassStyle,
    GrassSun, GrassVariation, GroundTint, NoGrass, Wind,
};

/// The things a game needs, in one import.
pub mod prelude {
    pub use crate::GrassPlugin;
    pub use crate::GrassQuality;
    pub use crate::GrassSettings;
    pub use crate::GrassShadows;
    pub use crate::GrassStats;
    pub use crate::GrassSystems;
    pub use crate::api::{
        GrassBlade, GrassCamera, GrassField, GrassGround, GrassInteractor, GrassSpecies,
        GrassStyle, GrassSun, GrassVariation, GroundTint, NoGrass, Wind,
    };
    pub use crate::far::{FarGrass, FarGrassMaterial};
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
    /// render layers), styles, masks and heightmaps, removed fields, and
    /// fields Bevy cannot draw as asked (a rotation, a layer above 31), and
    /// marks the tables for an update.
    DetectChanges,
    /// Updates the tables from what changed: a full rebuild for changed
    /// fields, styles and images, or only the entries for a field shown,
    /// hidden or moved to other render layers. A field whose style or images
    /// have not loaded yet waits.
    Rebuild,
}

/// Registers grass rendering. Add it after `DefaultPlugins`.
///
/// The settings are the [`GrassSettings`] resource from then on; change that
/// to change them at runtime.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrassPlugin {
    /// How much grass is drawn, how far, and at what cost: a tier such as
    /// [`GrassSettings::MEDIUM`], with any setting changed.
    pub settings: GrassSettings,
}

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
        register_types(app);
        app.init_asset_loader::<GrassStyleLoader>()
            .init_resource::<Wind>()
            .init_resource::<sun::ShadowSetupReport>()
            .insert_resource(self.settings)
            .add_plugins((stats::GrassDiagnosticsPlugin, fields::plugin))
            .add_plugins(bevy::pbr::MaterialPlugin::<FarGrassMaterial>::default())
            .add_systems(PreStartup, far::add_gust_texture)
            .add_systems(Update, far::sync_wind)
            .add_systems(
                PostUpdate,
                (
                    warn_about_settings.run_if(resource_changed::<GrassSettings>),
                    sun::check_shadow_setup.after(GrassSystems::Rebuild),
                ),
            )
            .add_plugins(render::GrassRenderPlugin { shaders });
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
        .register_type::<GrassGround>()
        .register_type::<GrassInteractor>()
        .register_type::<NoGrass>()
        .register_type::<GrassSun>()
        .register_type::<GrassCamera>()
        .register_type::<Wind>()
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
mod tests {
    use std::any::TypeId;

    use bevy::app::App;
    use bevy::asset::{AssetPlugin, Assets, ReflectAsset};
    use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent, ReflectResource};
    use bevy::math::Vec2;
    use bevy::reflect::serde::{ReflectDeserializer, ReflectSerializer};
    use bevy::reflect::std_traits::ReflectDefault;
    use bevy::reflect::{FromReflect, GetPath, PartialReflect, TypeRegistry};
    use serde::de::DeserializeSeed;

    use super::*;

    /// An app with exactly what the plugin registers for reflection.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(AssetPlugin::default());
        register_types(&mut app);
        app
    }

    /// Writes `value` as RON through reflection and reads it back, the way a
    /// `.scn.ron` file or a save game does.
    fn through_ron(value: &dyn PartialReflect, registry: &TypeRegistry) -> Box<dyn PartialReflect> {
        let text = ron::ser::to_string(&ReflectSerializer::new(value, registry))
            .expect("the type serializes");
        let mut deserializer = ron::de::Deserializer::from_str(&text).unwrap();
        ReflectDeserializer::new(registry)
            .deserialize(&mut deserializer)
            .unwrap_or_else(|error| panic!("{error}\n{text}"))
    }

    #[test]
    fn every_public_type_has_the_type_data_tools_look_for() {
        let app = app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let has = |id: TypeId, what: &str, found: bool| {
            let name = registry
                .get(id)
                .map_or("<unregistered>", |r| r.type_info().type_path());
            assert!(found, "{name} lacks {what}");
        };
        macro_rules! check {
            ($t:ty: $($data:ident),*) => {{
                let id = TypeId::of::<$t>();
                has(id, "registration", registry.contains(id));
                $( has(id, stringify!($data), registry.get_type_data::<$data>(id).is_some()); )*
            }};
        }
        // Components: scenes, BSN, Blender exporters and inspectors.
        check!(GrassField: ReflectComponent, ReflectDefault);
        check!(GrassInteractor: ReflectComponent, ReflectDefault);
        check!(NoGrass: ReflectComponent, ReflectDefault);
        check!(GrassCamera: ReflectComponent, ReflectDefault);
        // Resources: inspector resource panels.
        check!(Wind: ReflectResource, ReflectComponent, ReflectDefault);
        check!(GrassSettings: ReflectResource, ReflectComponent, ReflectDefault);
        // The style asset: inspector asset panels.
        check!(GrassStyle: ReflectAsset, ReflectDefault);
        // What they contain.
        check!(GrassBlade: ReflectDefault);
        check!(GrassVariation: ReflectDefault);
        check!(GroundTint: ReflectDefault);
        check!(GrassGround: ReflectDefault);
        check!(GrassShadows: ReflectDefault);
        check!(GrassQuality: ReflectDefault);
        check!(GrassSpecies:);
    }

    #[test]
    fn a_field_survives_a_scene_file_and_can_be_inserted_from_it() {
        let mut app = app();
        let field = GrassField {
            size: Vec2::new(48.0, 24.0),
            ground: GrassGround::Flat,
            ..Default::default()
        };
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let read = through_ron(field.as_partial_reflect(), &registry.read());

        // What a scene loader does: look up the component's type data and
        // insert the deserialized value.
        let mut entity = app.world_mut().spawn_empty();
        let registry = registry.read();
        let component = registry
            .get_type_data::<ReflectComponent>(TypeId::of::<GrassField>())
            .expect("ReflectComponent");
        component.insert(&mut entity, read.as_ref(), &registry);
        let inserted = entity.get::<GrassField>().expect("inserted");
        assert_eq!(inserted.size, Vec2::new(48.0, 24.0));
        assert!(matches!(inserted.ground, GrassGround::Flat));
    }

    #[test]
    fn the_small_components_and_the_settings_round_trip() {
        let app = app();
        let registry = app.world().resource::<AppTypeRegistry>().read();

        let camera = GrassCamera {
            radius: Some(30.0),
            shadows: Some(GrassShadows::Off),
            ..Default::default()
        };
        let read = through_ron(camera.as_partial_reflect(), &registry);
        assert_eq!(GrassCamera::from_reflect(read.as_ref()), Some(camera));

        let interactor = GrassInteractor {
            radius: 1.5,
            strength: 0.5,
        };
        let read = through_ron(interactor.as_partial_reflect(), &registry);
        assert_eq!(
            GrassInteractor::from_reflect(read.as_ref()),
            Some(interactor)
        );

        let settings = GrassSettings {
            radius: 140.0,
            ..GrassSettings::ULTRA
        };
        let read = through_ron(settings.as_partial_reflect(), &registry);
        assert_eq!(GrassSettings::from_reflect(read.as_ref()), Some(settings));

        let read = through_ron(Wind::STORM.as_partial_reflect(), &registry);
        let wind = Wind::from_reflect(read.as_ref()).expect("a wind");
        assert_eq!(wind.strength, Wind::STORM.strength);
        assert_eq!(wind.gust_speed, Wind::STORM.gust_speed);

        let read = through_ron(NoGrass.as_partial_reflect(), &registry);
        assert!(NoGrass::from_reflect(read.as_ref()).is_some());
    }

    #[test]
    fn a_whole_style_survives_a_save_file_and_can_be_edited_like_an_inspector_does() {
        let mut app = app();
        // Every preset, wild ones with their species included.
        {
            let registry = app.world().resource::<AppTypeRegistry>().read();
            for (name, style) in GrassStyle::presets() {
                let read = through_ron(style.as_partial_reflect(), &registry);
                let read = GrassStyle::from_reflect(read.as_ref()).expect(name);
                assert_eq!(read, style, "{name}");
            }
        }

        // An inspector's asset panel: add through `ReflectAsset`, edit a
        // setting by its path in the struct, and the real asset has changed.
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        let registry = registry.read();
        let reflect_asset = registry
            .get_type_data::<ReflectAsset>(TypeId::of::<GrassStyle>())
            .expect("ReflectAsset");
        let handle = reflect_asset.add(app.world_mut(), GrassStyle::MEADOW.as_partial_reflect());
        let asset = reflect_asset
            .get_mut(app.world_mut(), handle.id())
            .expect("the asset is there");
        *asset
            .reflect_path_mut("blade.height.y")
            .unwrap()
            .try_downcast_mut::<f32>()
            .unwrap() = 2.5;
        *asset
            .reflect_path_mut("variation.tufts")
            .unwrap()
            .try_downcast_mut::<f32>()
            .unwrap() = 0.7;
        let styles = app.world().resource::<Assets<GrassStyle>>();
        let edited = styles.get(handle.id().typed::<GrassStyle>()).unwrap();
        assert_eq!(edited.blade.height.y, 2.5);
        assert_eq!(edited.variation.tufts, 0.7);
        assert_eq!(edited.density, GrassStyle::MEADOW.density);
    }

    #[test]
    fn a_resource_is_edited_through_reflection_like_an_inspector_does() {
        let mut wind = Wind::BREEZE;
        let storm = Wind::STORM;
        // An inspector applies a reflected value onto the live one.
        wind.apply(storm.as_partial_reflect());
        assert_eq!(wind.strength, Wind::STORM.strength);
        *wind
            .reflect_path_mut("gust_scale")
            .unwrap()
            .try_downcast_mut::<f32>()
            .unwrap() = 99.0;
        assert_eq!(wind.gust_scale, 99.0);

        let mut settings = GrassSettings::LOW;
        settings.apply(GrassSettings::ULTRA.as_partial_reflect());
        assert_eq!(settings, GrassSettings::ULTRA);
        // `Default` through reflection is the type's default.
        let app = app();
        let registry = app.world().resource::<AppTypeRegistry>().read();
        let make = registry
            .get_type_data::<ReflectDefault>(TypeId::of::<GrassSettings>())
            .unwrap();
        let made = make.default();
        assert_eq!(
            GrassSettings::from_reflect(made.as_partial_reflect()),
            Some(GrassSettings::HIGH)
        );
    }
}
