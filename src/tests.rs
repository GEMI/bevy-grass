use std::any::TypeId;

use bevy::app::App;
use bevy::asset::{AssetPlugin, Assets, ReflectAsset};
use bevy::ecs::reflect::{AppTypeRegistry, ReflectComponent, ReflectResource};
use bevy::math::{Vec2, Vec3};
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
    let text =
        ron::ser::to_string(&ReflectSerializer::new(value, registry)).expect("the type serializes");
    let mut deserializer = ron::de::Deserializer::from_str(&text).unwrap();
    ReflectDeserializer::new(registry)
        .deserialize(&mut deserializer)
        .unwrap_or_else(|error| panic!("{error}\n{text}"))
}

#[test]
fn the_readme_quick_start_is_the_minimal_example() {
    // The first thing a new user copies must be a program that runs, so
    // the README holds the example's text and a test keeps them equal.
    let example = include_str!("../examples/minimal.rs");
    let code = example
        .split(
            "

",
        )
        .skip(1)
        .collect::<Vec<_>>()
        .join(
            "

",
        );
    let readme = include_str!("../README.md");
    let section = readme
        .split("## Quick start")
        .nth(1)
        .expect("a quick start");
    let block = section
        .split(
            "```rust
",
        )
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("a code block");
    assert_eq!(
        block.trim(),
        code.trim(),
        "update the README from examples/minimal.rs"
    );
}

#[test]
fn fields_and_styles_can_be_compared() {
    // `set_if_neq` and plain `==` in games and tests.
    let style = GrassField {
        size: Vec2::splat(8.0),
        ..Default::default()
    };
    assert_eq!(style, style.clone());
    assert_ne!(
        style,
        GrassField {
            size: Vec2::splat(9.0),
            ..style.clone()
        }
    );
    assert_ne!(
        GrassGround::Flat,
        GrassGround::Heightmap {
            image: bevy::asset::Handle::default(),
            height_scale: 1.0
        }
    );
    assert_eq!(GrassStyle::MEADOW, GrassStyle::MEADOW.clone());
    assert_ne!(GrassStyle::MEADOW, GrassStyle::WHEAT);
    assert_eq!(
        GrassStyle::SUMMER_WILD.species[0],
        GrassStyle::SUMMER_WILD.species[0]
    );
}

#[test]
fn a_field_can_be_built_from_a_preset_in_one_call() {
    let mut app = app();
    let mut styles = app.world_mut().resource_mut::<Assets<GrassStyle>>();

    let field = GrassField::new(Vec2::splat(8.0), GrassStyle::MEADOW, &mut styles);
    assert_eq!(field.size, Vec2::splat(8.0));
    assert_eq!(field.ground, GrassGround::Flat);
    assert_eq!(field.density_map, None);
    assert_eq!(styles.get(&field.style), Some(&GrassStyle::MEADOW));

    let alias = GrassField::from_preset(Vec2::splat(4.0), GrassStyle::WHEAT, &mut styles);
    assert_eq!(alias.size, Vec2::splat(4.0));
    assert_eq!(styles.get(&alias.style), Some(&GrassStyle::WHEAT));
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
    check!(GrassFieldOverrides: ReflectComponent, ReflectDefault);
    check!(GrassInteractor: ReflectComponent, ReflectDefault);
    check!(GrassInteractorSpread: ReflectComponent, ReflectDefault);
    check!(GrassInteractorFootprint: ReflectComponent, ReflectDefault);
    check!(GrassInteractorFromCollider: ReflectComponent, ReflectDefault);
    check!(GrassColliderBounds: ReflectComponent, ReflectDefault);
    check!(NoGrass: ReflectComponent, ReflectDefault);
    check!(GrassCamera: ReflectComponent, ReflectDefault);
    // Resources: inspector resource panels.
    check!(GrassWind: ReflectResource, ReflectComponent, ReflectDefault);
    check!(GrassWindFrame: ReflectResource, ReflectDefault);
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
        max_height: Some(1.0),
    };
    let read = through_ron(interactor.as_partial_reflect(), &registry);
    assert_eq!(
        GrassInteractor::from_reflect(read.as_ref()),
        Some(interactor)
    );

    let spread = GrassInteractorSpread {
        offsets: vec![Vec3::X, Vec3::new(0.0, 1.0, 2.0)],
    };
    let read = through_ron(spread.as_partial_reflect(), &registry);
    assert_eq!(
        GrassInteractorSpread::from_reflect(read.as_ref()),
        Some(spread)
    );

    let settings = GrassSettings {
        radius: 140.0,
        ..GrassSettings::ULTRA
    };
    let read = through_ron(settings.as_partial_reflect(), &registry);
    assert_eq!(GrassSettings::from_reflect(read.as_ref()), Some(settings));

    let read = through_ron(GrassWind::STORM.as_partial_reflect(), &registry);
    let wind = GrassWind::from_reflect(read.as_ref()).expect("a wind");
    assert_eq!(wind.strength, GrassWind::STORM.strength);
    assert_eq!(wind.gust_speed, GrassWind::STORM.gust_speed);

    let frame = GrassWindFrame::from(GrassWind::GUSTY);
    let read = through_ron(frame.as_partial_reflect(), &registry);
    assert_eq!(GrassWindFrame::from_reflect(read.as_ref()), Some(frame));

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
    let mut wind = GrassWind::BREEZE;
    let storm = GrassWind::STORM;
    // An inspector applies a reflected value onto the live one.
    wind.apply(storm.as_partial_reflect());
    assert_eq!(wind.strength, GrassWind::STORM.strength);
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
    let make_default = registry
        .get_type_data::<ReflectDefault>(TypeId::of::<GrassSettings>())
        .unwrap();
    let value = make_default.default();
    assert_eq!(
        GrassSettings::from_reflect(value.as_partial_reflect()),
        Some(GrassSettings::HIGH)
    );
}
