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
fn every_theme_preset_is_tall_and_valid() {
    for style in GrassStyle::SUMMER.iter().chain(&GrassStyle::AUTUMN) {
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
    for style in &GrassStyle::SUMMER[..3] {
        let tip = rgb(style);
        assert!(tip.green > tip.red && tip.green > tip.blue, "{tip:?}");
    }
    for style in &GrassStyle::AUTUMN {
        let tip = rgb(style);
        assert!(tip.red > tip.green && tip.green > tip.blue, "{tip:?}");
    }
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
