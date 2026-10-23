use super::*;

#[test]
fn spacing_and_density_describe_the_same_field() {
    let style = GrassStyle::SUMMER_TALL.with_spacing(0.2);
    assert!((style.density - 25.0).abs() < 1e-3);
    assert!((style.spacing() - 0.2).abs() < 1e-6);
    let style = style.with_density(100.0);
    assert!((style.spacing() - 0.1).abs() < 1e-6);
}

#[test]
fn every_season_preset_is_tall_and_valid() {
    for style in &GrassStyle::SEASONS {
        let blade = &style.blade;
        assert!(blade.height.x > 0.0 && blade.height.x < blade.height.y);
        assert!(blade.height.y >= 1.0, "tall grass reaches at least 1 m");
        assert!(style.density > 0.0 && blade.width > 0.0);
        assert!((0.0..=1.0).contains(&blade.translucency));
    }
}

#[test]
fn summer_is_green_and_autumn_is_brown() {
    let rgb = |style: &GrassStyle| style.blade.tip_color.to_srgba();
    let summer = rgb(&GrassStyle::SUMMER_TALL);
    assert!(
        summer.green > summer.red && summer.green > summer.blue,
        "{summer:?}"
    );
    let autumn = rgb(&GrassStyle::AUTUMN_TALL);
    assert!(
        autumn.red > autumn.green && autumn.green > autumn.blue,
        "{autumn:?}"
    );
    // The two seasons are one field, so they mix and blend: the same blade
    // sizes and density.
    let (summer, autumn) = (&GrassStyle::SUMMER_TALL, &GrassStyle::AUTUMN_TALL);
    assert_eq!(summer.blade.height, autumn.blade.height);
    assert_eq!(summer.blade.width, autumn.blade.width);
    assert_eq!(summer.density, autumn.density);
}

#[test]
fn every_preset_is_valid_and_looks_like_no_other() {
    let presets: Vec<_> = GrassStyle::presets().collect();
    for (name, style) in &presets {
        let blades = std::iter::once(&style.blade).chain(style.species.iter().map(|s| &s.blade));
        for blade in blades {
            assert!(
                blade.height.x >= 0.0 && blade.height.x <= blade.height.y,
                "{name}"
            );
            assert!(blade.width > 0.0, "{name}");
            for unit in [
                blade.lean,
                blade.stiffness,
                blade.translucency,
                blade.specular,
            ] {
                assert!((0.0..=1.0).contains(&unit), "{name}");
            }
            for unit in [blade.droop, blade.fold, blade.root_occlusion] {
                assert!((0.0..=1.0).contains(&unit), "{name}");
            }
            assert!((0.0..0.9).contains(&blade.head_length), "{name}");
        }
        assert!(style.density > 0.0, "{name}");
        assert!(style.species.len() <= 15, "{name}");
        let share: f32 = style.species.iter().map(|species| species.share).sum();
        assert!(
            share < 1.0,
            "{name}: the species leave some of the style's own blades"
        );
    }
    // No two presets are the same field.
    for (index, (name, style)) in presets.iter().enumerate() {
        for (other, other_style) in &presets[index + 1..] {
            assert_ne!(style, other_style, "{name} and {other}");
        }
    }
}

#[test]
fn the_new_looks_push_what_the_meadows_leave_alone() {
    // Each of these is in the set because it uses a setting harder than any
    // general preset does; keep it that way when tuning them.
    // A preset, why it is in the set, and the setting that shows it.
    type Push = (&'static str, &'static str, fn(&GrassStyle) -> bool);
    let pushes: [Push; 11] = [
        ("CORKSCREW_ALIEN", "ribbons twist", |s| s.blade.twist >= 1.5),
        ("CRYSTAL_SPIRES", "crystal barely moves", |s| {
            s.blade.stiffness >= 0.95
        }),
        ("CRYSTAL_SPIRES", "crystal is faceted", |s| {
            s.blade.fold >= 0.95
        }),
        ("KELP_BED", "kelp has no stiffness", |s| {
            s.blade.stiffness <= 0.05
        }),
        ("PAMPAS", "tufts become fountains", |s| {
            s.variation.tufts >= 1.0 && s.blade.droop >= 0.85
        }),
        ("SPINIFEX", "hummocks splay out", |s| {
            s.blade.lean >= 0.8 && s.variation.tufts >= 1.0
        }),
        ("DUNE_MARRAM", "mostly sand", |s| {
            s.variation.bare_share >= 0.4
        }),
        ("ELEPHANT_GRASS", "taller than a person", |s| {
            s.blade.height.x >= 2.0
        }),
        ("REED_MARSH", "over your head", |s| s.blade.height.y >= 1.8),
        ("CEL_FIELD", "two bands", |s| s.toon_bands == 2),
        ("COZY_PASTEL", "toon", |s| s.toon_bands > 0),
    ];
    for (name, why, holds) in pushes {
        let style = GrassStyle::preset(name).expect("a preset");
        assert!(holds(&style), "{name}: {why}");
    }
    let value = |color: Color| {
        let c = color.to_srgba();
        c.red.max(c.green).max(c.blue)
    };
    assert!(
        value(GrassStyle::BURNT_REGROWTH.blade.tip_color) < 0.15,
        "charcoal"
    );
    let frost = GrassStyle::FROSTED_MEADOW.blade.tip_color.to_srgba();
    assert!(frost.red > 0.75 && frost.blue > 0.75, "near white");
    let cornflower = GrassBlade::CORNFLOWER.head_color.to_srgba();
    assert!(cornflower.blue > cornflower.red && cornflower.blue > cornflower.green);
    assert!(
        GrassStyle::COTTON_BOG
            .species
            .iter()
            .any(|species| species.blade == GrassBlade::COTTON_GRASS)
    );
    assert!(
        GrassStyle::REED_MARSH
            .species
            .iter()
            .any(|species| species.blade == GrassBlade::CATTAIL)
    );
}

#[test]
fn a_style_is_a_blade_and_what_belongs_to_the_field() {
    // Struct update reaches one level into the blade, in a constant too.
    const DRY: GrassStyle = GrassStyle {
        blade: GrassBlade {
            tip_color: Color::srgb(0.7, 0.6, 0.3),
            ..GrassStyle::MEADOW.blade
        },
        density: 10.0,
        ..GrassStyle::MEADOW
    };
    assert_eq!(DRY.blade.height, GrassStyle::MEADOW.blade.height);
    assert_ne!(DRY.blade.tip_color, GrassStyle::MEADOW.blade.tip_color);
    assert_eq!(DRY.variation, GrassStyle::MEADOW.variation);
    assert_eq!(DRY.ground_tint, GrassStyle::MEADOW.ground_tint);
    assert_ne!(DRY, GrassStyle::MEADOW, "styles compare");

    // The defaults are the meadow's.
    assert_eq!(GrassStyle::default(), GrassStyle::MEADOW);
    assert_eq!(GrassBlade::default(), GrassStyle::MEADOW.blade);
    assert_eq!(GrassVariation::default(), GrassStyle::MEADOW.variation);
    assert_eq!(GroundTint::default(), GrassStyle::MEADOW.ground_tint);
}

#[test]
fn the_forest_presets_are_short_wide_and_mixed() {
    // The group holds each forest style once, in order.
    let listed = [
        (GrassStyle::FOREST_MOSS, "FOREST_MOSS"),
        (GrassStyle::FOREST_FERNS, "FOREST_FERNS"),
        (GrassStyle::FOREST_FLOOR, "FOREST_FLOOR"),
        (GrassStyle::FOREST_BRACKEN, "FOREST_BRACKEN"),
    ];
    for ((style, name), group) in listed.iter().zip(&GrassStyle::FOREST) {
        assert_eq!(style, group, "{name}");
        assert!(style.density > 0.0, "{name}");
        assert!(style.blade.height.y >= style.blade.height.x, "{name}");
        assert!(style.blade.width > 0.02, "{name}: forest plants are wide");
        let share: f32 = style.species.iter().map(|species| species.share).sum();
        assert!(share <= 1.0, "{name}: the species leave some blades");
    }

    // Moss is a carpet: very short, wide, dense, and only moss.
    let moss = GrassStyle::FOREST_MOSS;
    assert!(moss.blade.height.y <= 0.1, "moss stays low");
    assert!(
        moss.blade.width > moss.blade.height.y * 0.3,
        "moss is broad"
    );
    assert!(moss.density > GrassStyle::LAWN.density * 0.5);
    assert!(moss.species.is_empty());

    // The fern styles have fern blades among their species, or as their
    // own blades, and the whole-floor mix has every floor plant.
    let is_fern = |blade: &GrassBlade| blade.droop >= 0.6 && blade.fold >= 0.5;
    assert!(is_fern(&GrassStyle::FOREST_FERNS.blade));
    assert!(
        GrassStyle::FOREST_FERNS
            .species
            .iter()
            .any(|species| species.clustering >= 0.7)
    );
    assert!(
        GrassStyle::FOREST_FLOOR
            .species
            .iter()
            .any(|species| species.blade == GrassBlade::FERN)
    );
    assert!(
        GrassStyle::FOREST_FLOOR
            .species
            .iter()
            .any(|species| species.blade == GrassBlade::LEAF_LITTER)
    );
    assert!(is_fern(&GrassStyle::FOREST_BRACKEN.blade));
    let bracken = GrassStyle::FOREST_BRACKEN.blade.tip_color.to_srgba();
    assert!(bracken.red > bracken.green && bracken.green > bracken.blue);
}

#[test]
fn a_plant_is_a_blade_and_a_style_of_its_own() {
    let plants = [
        (GrassStyle::SEED_HEADS, GrassBlade::SEED_HEADS),
        (GrassStyle::WHITE_FLOWERS, GrassBlade::WHITE_FLOWERS),
        (GrassStyle::YELLOW_FLOWERS, GrassBlade::YELLOW_FLOWERS),
        (GrassStyle::CLOVER, GrassBlade::CLOVER),
        (GrassStyle::BROAD_LEAF, GrassBlade::BROAD_LEAF),
    ];
    for ((style, blade), listed) in plants.iter().zip(&GrassStyle::PLANTS) {
        assert_eq!(&style.blade, blade);
        assert_eq!(style, listed);
        assert!(style.species.is_empty(), "a plant is not a mix");
    }
    // The wild styles mix them by blade.
    let wild = &GrassStyle::SUMMER_WILD.species;
    assert_eq!(wild[0].blade, GrassBlade::SEED_HEADS);
    assert_eq!(wild[0].share, 0.10);
    let species = GrassSpecies::new(GrassBlade::CLOVER, 0.2).clustered(0.5);
    assert_eq!((species.share, species.clustering), (0.2, 0.5));
    assert_eq!(species.blade, GrassBlade::CLOVER);
}
