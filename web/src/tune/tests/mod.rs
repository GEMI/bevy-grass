use bevy::color::Color;
use bevy::reflect::{PartialReflect, ReflectPath, TypeRegistry};
use bevy_grass::prelude::*;

use super::{set_value, write_node, write_rust_changes};

/// The types the tests set values of, as `AppTypeRegistry` has them.
fn registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    registry.register::<f32>();
    registry.register::<Color>();
    registry.register::<GrassShadows>();
    registry
}

/// Sets `path` of `root` to `text`, as a control on the page does.
fn set(root: &mut dyn PartialReflect, path: &str, text: &str) -> Result<(), String> {
    let field = path.reflect_element_mut(root).map_err(|e| e.to_string())?;
    set_value(field, text, &registry())
}

#[test]
fn sets_numbers_colours_and_list_items_by_path() {
    let mut style = GrassStyle::WILDFLOWER_MEADOW;
    set(&mut style, "blade.height.y", "0.9").unwrap();
    set(&mut style, "blade.tip_color", "#ff8000").unwrap();
    set(&mut style, "toon_bands", "3").unwrap();
    set(&mut style, "species[0].share", "0.25").unwrap();
    assert!((style.blade.height.y - 0.9).abs() < f32::EPSILON);
    assert_eq!(style.blade.tip_color.to_srgba().to_hex(), "#FF8000");
    assert_eq!(style.toon_bands, 3);
    assert!((style.species[0].share - 0.25).abs() < f32::EPSILON);
    assert!(set(&mut style, "density", "lots").is_err());
}

#[test]
fn switches_enum_variants_and_their_values() {
    let mut camera = GrassCamera::default();
    set(&mut camera, "radius", "Some:150").unwrap();
    assert_eq!(camera.radius, Some(150.0));
    set(&mut camera, "radius.0", "90").unwrap();
    assert_eq!(camera.radius, Some(90.0));
    set(&mut camera, "radius", "None").unwrap();
    assert_eq!(camera.radius, None);

    let mut settings = GrassSettings::HIGH;
    set(&mut settings, "shadows", "Full").unwrap();
    assert_eq!(settings.shadows, GrassShadows::Full);

    let mut brush = GrassBrush::new(GrassEditOp::Density(0.0), 2.0);
    set(&mut brush, "op", "Mow:0.3").unwrap();
    assert_eq!(brush.op, GrassEditOp::Mow(0.3));
    set(&mut brush, "op", "Restore").unwrap();
    assert_eq!(brush.op, GrassEditOp::Restore);
}

#[test]
fn rust_names_only_what_changed_from_the_preset() {
    let mut style = GrassStyle::MEADOW;
    style.blade.width = 0.037;
    let mut rust = String::new();
    write_rust_changes(
        &style,
        Some(&GrassStyle::MEADOW),
        "GrassStyle::MEADOW",
        0,
        &mut rust,
    );
    assert_eq!(
        rust,
        "GrassStyle {\n    blade: GrassBlade {\n        width: 0.037,\n        \
         ..GrassStyle::MEADOW.blade\n    },\n    ..GrassStyle::MEADOW\n}"
    );

    let mut unchanged = String::new();
    write_rust_changes(
        &GrassStyle::MEADOW,
        Some(&GrassStyle::MEADOW),
        "GrassStyle::MEADOW",
        0,
        &mut unchanged,
    );
    assert_eq!(unchanged, "GrassStyle::MEADOW");
}

#[test]
fn json_describes_each_kind_of_value() {
    let mut json = String::new();
    write_node(&GrassCamera::default(), &mut json);
    assert!(json.starts_with("{\"t\":\"struct\",\"ty\":\"GrassCamera\""));
    assert!(json.contains(
        "{\"n\":\"radius\",\"v\":{\"t\":\"enum\",\"ty\":\"Option\",\"v\":\"None\",\"variants\":[\"None\",\"Some\"],\"f\":[]}}"
    ) || json.contains("\"n\":\"radius\",\"d\":"));
    json.clear();
    write_node(&GrassSettings::HIGH.blade_rows, &mut json);
    assert_eq!(
        json,
        "{\"t\":\"list\",\"fixed\":true,\"items\":[{\"t\":\"int\",\"v\":7},{\"t\":\"int\",\"v\":3},{\"t\":\"int\",\"v\":1}]}"
    );
}
