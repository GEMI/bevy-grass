use super::*;
use crate::api::{GrassVariation, GroundTint};

fn srgb(color: Color) -> (f32, f32, f32) {
    let c = color.to_srgba();
    (c.red, c.green, c.blue)
}

#[test]
fn min_width_px_is_no_longer_a_style_setting_but_old_files_still_load() {
    // A file from before the setting moved to `GrassSettings`, with the
    // line in the style and in a species.
    let old = r#"(
        base: "SUMMER_TALL",
        min_width_px: 2.0,
        species: [(share: 0.1, style: (base: "CLOVER", min_width_px: 3.0))],
    )"#;
    let style = GrassStyle::from_ron(old).expect("old files must keep loading");
    let without = GrassStyle::from_ron(
        r#"(base: "SUMMER_TALL", species: [(share: 0.1, style: (base: "CLOVER"))])"#,
    )
    .unwrap();
    assert_eq!(style.to_ron(), without.to_ron(), "the old value is ignored");
    assert!(
        !style.to_ron().contains("min_width_px"),
        "a style no longer writes the setting"
    );
    assert!(!GrassStyleFile::default().mentions_min_width());
    let file: GrassStyleFile = options().from_str(old).unwrap();
    assert!(file.mentions_min_width(), "so the loader can warn about it");
}

fn preset_path(name: &str) -> String {
    format!(
        "{}/assets/grass_styles/{}.grass.ron",
        env!("CARGO_MANIFEST_DIR"),
        name.to_ascii_lowercase()
    )
}

/// The shipped files, which must stay equal to the constants. With
/// `BLESS_STYLES` set, the file's text is generated from the constant so
/// a missing file is created rather than failing the test.
fn shipped(name: &str) -> String {
    if std::env::var_os("BLESS_STYLES").is_some() {
        let style = GrassStyle::preset(name).expect("a preset");
        return format!("{}{}\n", style_doc(name), style.to_ron());
    }
    let file = preset_path(name);
    std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{file}: {error}"))
}

#[test]
fn every_preset_file_matches_its_constant() {
    // `BLESS_STYLES=1 cargo test` rewrites the files from the constants.
    let bless = std::env::var_os("BLESS_STYLES").is_some();
    for (name, style) in GrassStyle::presets() {
        if bless {
            std::fs::write(preset_path(name), shipped(name)).unwrap();
        }
        let read = GrassStyle::from_ron(&shipped(name)).unwrap();
        assert_eq!(read, style, "{name}");
    }
}

#[test]
fn every_preset_is_shipped_as_an_embedded_file_that_is_the_preset() {
    use bevy::MinimalPlugins;
    use bevy::asset::{AssetPlugin, AssetServer, Assets};

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<GrassStyle>()
        .add_plugins(GrassStyleFilePlugin);
    let handles: Vec<_> = {
        let server = app.world().resource::<AssetServer>();
        GrassStyle::presets()
            .map(|(name, _)| {
                let path = format!(
                    "embedded://bevy_grass/styles/{}.grass.ron",
                    name.to_ascii_lowercase()
                );
                (name, server.load::<GrassStyle>(path))
            })
            .collect()
    };
    for _ in 0..500 {
        app.update();
        let styles = app.world().resource::<Assets<GrassStyle>>();
        if handles
            .iter()
            .all(|(_, handle)| styles.get(handle).is_some())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let styles = app.world().resource::<Assets<GrassStyle>>();
    for (name, style) in GrassStyle::presets() {
        let handle = handles
            .iter()
            .find(|(preset, _)| *preset == name)
            .unwrap_or_else(|| panic!("{name} is not embedded"))
            .1
            .clone();
        let loaded = styles
            .get(&handle)
            .unwrap_or_else(|| panic!("{name} did not load"));
        assert_eq!(*loaded, style, "{name}");
    }
}

#[test]
fn no_shipped_file_still_sets_the_old_min_width() {
    // Copying a preset must not start a game with a warning.
    for (name, _) in GrassStyle::presets() {
        assert!(!shipped(name).contains("min_width_px"), "{name}");
    }
}

/// The comment at the top of a preset's file.
fn style_doc(name: &str) -> String {
    format!(
        "// GrassStyle::{name}, as a file. Copy it, edit it, and load it with\n\
         // `asset_server.load(\"path/to/copy.grass.ron\")`. Every field is\n\
         // optional: a missing one comes from `base`, a preset name.\n"
    )
}

#[test]
fn colours_can_be_hex_as_artists_copy_them() {
    let style = GrassStyle::from_ron(
        r##"(base: "MEADOW", tip_color: "#5c8f24", root_color: (0.1, 0.2, 0.3),
             species: [(share: 0.1, style: (head_color: "#ff8000"))])"##,
    )
    .unwrap();
    let (r, g, b) = srgb(style.blade.tip_color);
    let near = |a: f32, b: u8| (a - f32::from(b) / 255.0).abs() < 1e-5;
    assert!(
        near(r, 0x5c) && near(g, 0x8f) && near(b, 0x24),
        "{r} {g} {b}"
    );
    assert_eq!(
        srgb(style.blade.root_color),
        (0.1, 0.2, 0.3),
        "tuples still work"
    );
    assert!(near(srgb(style.species[0].blade.head_color).0, 0xff));

    let Err(GrassStyleFileError::Parse { message, .. }) =
        GrassStyle::from_ron(r##"(tip_color: "#5c8f2")"##)
    else {
        panic!("a bad hex colour must be an error");
    };
    assert!(message.contains("hex colour"), "{message}");
}

#[test]
fn the_names_before_the_rename_still_load() {
    let old = GrassStyle::from_ron(
        "(base: \"MEADOW\", dead_blades: 0.1, bare_patches: 0.2, tuft_size: 0.9,
          species: [(share: 0.1, style: (head: 0.3))])",
    )
    .unwrap();
    let new = GrassStyle::from_ron(
        "(base: \"MEADOW\", dead_share: 0.1, bare_share: 0.2, tuft_spacing: 0.9,
          species: [(share: 0.1, style: (head_length: 0.3))])",
    )
    .unwrap();
    assert_eq!(old, new);
    assert_eq!(old.variation.dead_share, 0.1);
    assert_eq!(old.species[0].blade.head_length, 0.3);
    // Written with the new names only.
    let text = old.to_ron();
    assert!(text.contains("dead_share") && !text.contains("dead_blades"));
}

#[test]
fn the_format_holds_every_setting_of_the_style() {
    // The file is written from the type's fields, so a setting added to
    // `GrassBlade` or `GrassStyle` and left out of the `style_file!` list
    // shows up here: the style is changed in every setting, written,
    // read, and must come back equal.
    let mut style = GrassStyle::SUMMER_WILD;
    style.blade = GrassBlade {
        height: Vec2::new(0.11, 0.22),
        width: 0.011,
        lean: 0.12,
        stiffness: 0.13,
        root_color: Color::srgb(0.1, 0.2, 0.3),
        tip_color: Color::srgb(0.4, 0.5, 0.6),
        root_occlusion: 0.14,
        translucency: 0.15,
        specular: 0.16,
        twist: 0.17,
        droop: 0.18,
        fold: 0.19,
        head_length: 0.21,
        head_width: 0.023,
        head_color: Color::srgb(0.7, 0.8, 0.9),
    };
    style.density = 12.0;
    style.toon_bands = 3;
    style.ground_tint = GroundTint {
        color: Color::srgb(0.9, 0.8, 0.7),
        amount: 0.31,
    };
    style.variation = GrassVariation {
        patch_color: Color::srgb(0.3, 0.2, 0.1),
        patch_amount: 0.32,
        patch_size: 7.0,
        dry_tips: 0.33,
        dead_share: 0.34,
        tufts: 0.35,
        tuft_spacing: 0.36,
        bare_share: 0.37,
        bare_patch_size: 5.0,
    };
    assert_eq!(GrassStyle::from_ron(&style.to_ron()).unwrap(), style);
}

#[test]
fn a_variant_names_only_what_differs() {
    let style = GrassStyle::from_ron(
        r#"(
            base: "AUTUMN_TALL",
            tip_color: (0.70, 0.72, 0.74),
            density: 20.0,
        )"#,
    )
    .unwrap();
    assert_eq!(style.density, 20.0);
    assert_eq!(style.blade.height, GrassStyle::AUTUMN_TALL.blade.height);
    assert_eq!(srgb(style.blade.tip_color), (0.70, 0.72, 0.74));
}

#[test]
fn mistakes_are_reported() {
    assert!(matches!(
        GrassStyle::from_ron("(base: \"NOT_A_PRESET\")"),
        Err(GrassStyleFileError::UnknownBase(_))
    ));
    assert!(matches!(
        GrassStyle::from_ron("(hieght: (0.1, 0.2))"),
        Err(GrassStyleFileError::Parse { .. })
    ));
    assert!(matches!(
        GrassStyle::from_ron("(height: (0.5, 0.2))"),
        Err(GrassStyleFileError::Invalid(_))
    ));
}

#[test]
fn a_parse_error_says_where_and_why() {
    let error = GrassStyle::from_ron("(\n    width: 0.02,\n    hieght: (0.1, 0.2),\n)")
        .expect_err("a misspelt setting must be an error");
    let GrassStyleFileError::Parse { line, message, .. } = &error else {
        panic!("a misspelt setting is a parse error, not {error:?}");
    };
    assert_eq!(*line, 3, "{error}");
    assert!(message.contains("hieght"), "{message}");
    assert!(
        error.to_string().starts_with("not a grass style: 3:"),
        "{error}"
    );
}

#[test]
fn a_species_is_a_blade_and_a_file_cannot_say_otherwise() {
    // The settings of the whole field do nothing in a species. In code the
    // type has nowhere to put them; in a file they are an error that
    // names them, not something silently ignored.
    let Err(error) = GrassStyle::from_ron(
        "(species: [(share: 0.1, style: (base: \"CLOVER\", tufts: 0.8, density: 5.0))])",
    ) else {
        panic!("field settings in a species must be an error");
    };
    let GrassStyleFileError::SpeciesFieldSettings(names) = &error else {
        panic!("wrong error: {error}");
    };
    assert_eq!(names, &["density", "tufts"]);
    let message = error.to_string();
    assert!(message.contains("density, tufts"), "{message}");

    // Nested species are the same mistake.
    assert!(matches!(
        GrassStyle::from_ron(
            "(species: [(share: 0.1, style: (species: [(share: 0.1, style: ())]))])"
        ),
        Err(GrassStyleFileError::SpeciesFieldSettings(names)) if names == ["species"]
    ));

    // Blade settings are fine, and are range-checked like a style's.
    assert!(
        GrassStyle::from_ron("(species: [(share: 0.1, style: (head_color: (1.0, 0.0, 0.0)))])")
            .is_ok()
    );
    assert!(matches!(
        GrassStyle::from_ron("(species: [(share: 0.1, style: (height: (2.0, 1.0)))])"),
        Err(GrassStyleFileError::Invalid(_))
    ));
    assert!(matches!(
        GrassStyle::from_ron("(species: [(share: 0.1, style: (head_length: 0.95))])"),
        Err(GrassStyleFileError::Invalid(_))
    ));
}

#[test]
fn every_preset_survives_a_round_trip_through_a_file() {
    for (name, style) in GrassStyle::presets() {
        let text = style.to_ron();
        let read = GrassStyle::from_ron(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(read, style, "{name}");
        // A species lists its blade and nothing of the field's.
        let species = species_text(&text);
        for field_setting in ["density", "tufts", "ground_tint", "toon_bands"] {
            assert!(!species.contains(field_setting), "{name}: {field_setting}");
        }
    }
}

/// The part of a written style after `species:`, where the species are.
fn species_text(text: &str) -> &str {
    text.split("species:").nth(1).unwrap_or("")
}

#[test]
fn species_name_only_what_differs_too() {
    let style = GrassStyle::from_ron(
        r#"(
            base: "SUMMER_TALL",
            species: [
                (share: 0.1, style: (base: "SEED_HEADS")),
                (share: 0.03, clustering: 0.8, style: (
                    base: "WHITE_FLOWERS",
                    head_color: (0.75, 0.55, 0.95),
                )),
            ],
        )"#,
    )
    .unwrap();
    assert_eq!(style.species.len(), 2);
    assert_eq!(style.species[0].clustering, 0.0);
    assert_eq!(style.species[0].blade, GrassBlade::SEED_HEADS);
    assert_eq!(style.species[1].clustering, 0.8);
    assert_eq!(srgb(style.species[1].blade.head_color), (0.75, 0.55, 0.95));
    assert_eq!(
        style.species[1].blade.height,
        GrassBlade::WHITE_FLOWERS.height
    );
}
