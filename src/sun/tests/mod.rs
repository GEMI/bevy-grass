use bevy::ecs::world::World;

use super::*;

/// Lights with ids 0, 1, 2 …, shining along +X, +Y, +Z in turn.
fn lights(world: &mut World, spec: &[(bool, bool)]) -> Vec<SunCandidate> {
    let directions = [Vec3::X, Vec3::Y, Vec3::Z, Vec3::NEG_X];
    spec.iter()
        .enumerate()
        .map(|(i, &(casts, marked))| SunCandidate {
            entity: world.spawn_empty().id(),
            casts,
            marked,
            shines: directions[i],
        })
        .collect()
}

#[test]
fn the_marked_light_decides_whatever_the_order() {
    let mut world = World::new();
    // A sun and a moon, both casting shadows; the moon is spawned first
    // and would win "first".
    let l = lights(&mut world, &[(true, false), (true, true)]);
    let (shadow, sun) = pick_sun(l.clone());
    assert_eq!(shadow, Some(Vec3::Y), "the marked light casts the shadows");
    assert_eq!(sun, Some(Vec3::Y));
    // Whichever way round the candidates arrive.
    let reversed = pick_sun(l.iter().rev().copied());
    assert_eq!(reversed, (shadow, sun));
}

#[test]
fn without_a_mark_the_first_shadow_caster_by_entity_wins_every_time() {
    let mut world = World::new();
    let l = lights(&mut world, &[(false, false), (true, false), (true, false)]);
    let (shadow, sun) = pick_sun(l.clone());
    assert_eq!(
        shadow,
        Some(Vec3::Y),
        "the lowest caster, not the lowest light"
    );
    assert_eq!(sun, Some(Vec3::Y));
    for _ in 0..3 {
        let mut shuffled = l.clone();
        shuffled.rotate_left(1);
        assert_eq!(
            pick_sun(shuffled),
            (shadow, sun),
            "arrival order must not matter"
        );
    }
}

#[test]
fn without_shadows_the_sun_is_still_the_first_light() {
    let mut world = World::new();
    let l = lights(&mut world, &[(false, false), (false, false)]);
    assert_eq!(pick_sun(l), (None, Some(Vec3::X)));
    assert_eq!(pick_sun([]), (None, None));
}

#[test]
fn a_mark_on_a_light_without_shadows_is_obeyed_not_second_guessed() {
    let mut world = World::new();
    let l = lights(&mut world, &[(true, false), (false, true)]);
    let (shadow, sun) = pick_sun(l);
    assert_eq!(
        shadow, None,
        "the marked light casts no shadows, so neither does grass"
    );
    assert_eq!(sun, Some(Vec3::Y), "but it is the sun");
}

#[test]
fn the_likely_mistakes_are_named() {
    let mut world = World::new();
    // A sun and a moon, neither marked.
    let two = lights(&mut world, &[(true, false), (true, false)]);
    let problems = sun_problems(&two, true);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("GrassSun"), "{problems:?}");
    assert!(
        sun_problems(&two, false).is_empty(),
        "no grass shadows, no problem"
    );

    // Marked, and fine.
    let marked = lights(&mut world, &[(true, false), (true, true)]);
    assert!(sun_problems(&marked, true).is_empty());

    // Marked without shadows.
    let dark = lights(&mut world, &[(false, true)]);
    let problems = sun_problems(&dark, true);
    assert!(problems[0].contains("shadow_maps_enabled"), "{problems:?}");

    // Two marks.
    let twice = lights(&mut world, &[(true, true), (true, true)]);
    assert!(sun_problems(&twice, true)[0].contains("2 lights carry GrassSun"));
    assert!(sun_problems(&twice, false)[0].contains("2 lights carry GrassSun"));

    // One light and no mark is the common, fine case.
    let one = lights(&mut world, &[(true, false)]);
    assert!(sun_problems(&one, true).is_empty());
}

#[test]
fn a_cascade_wider_than_a_blade_is_reported_and_a_good_one_is_not() {
    // The measured case: 60 m in a 2048 map is 4 to 5 cm a texel.
    let texel = texel_size(60.0, 2048);
    assert!((0.04..0.05).contains(&texel), "{texel}");

    // Blades 1 to 6 cm wide, the presets' range (a lawn's are 0.8 cm).
    assert!(cascade_problem(60.0, 2048, 0.012).is_some());
    assert!(
        cascade_problem(60.0, 2048, 0.06).is_none(),
        "wide blades are fine"
    );
    // The recommended setup: 12 m, which a 1.2 cm blade just clears.
    assert!(cascade_problem(12.0, 2048, 0.012).is_none());
    // A bigger shadow map makes the same cascade fine.
    assert!(cascade_problem(60.0, 8192, 0.012).is_none());

    // The message tells what to do, with a depth that really clears it.
    let message = cascade_problem(60.0, 2048, 0.012).unwrap();
    assert!(message.contains("first_cascade_far_bound"), "{message}");
    let deepest = 0.012 * 2048.0 / 1.5;
    assert!(texel_size(deepest, 2048) <= 0.012 + 1e-6);
    assert!(message.contains(&format!("{deepest:.0} m")), "{message}");
}
mod system {
    use std::sync::Arc;

    use bevy::MinimalPlugins;
    use bevy::app::{App, Update};
    use bevy::light::CascadeShadowConfigBuilder;
    use bevy::math::Vec4;

    use super::*;
    use crate::gpu_layout::FieldGpu;

    /// An app running `check_shadow_setup` over one field of blades
    /// `blade_width` metres wide.
    fn app(shadows: GrassShadows, blade_width: f32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GrassSettings {
                shadows,
                ..GrassSettings::HIGH
            })
            .insert_resource(FieldTables {
                fields: Arc::new(vec![FieldGpu {
                    shape: Vec4::new(0.3, 0.6, blade_width, 40.0),
                    ..FieldGpu::default()
                }]),
                ..FieldTables::default()
            })
            .init_resource::<ShadowSetupReport>()
            .add_systems(Update, check_shadow_setup);
        app
    }

    fn sun(app: &mut App, first_cascade: f32, casts: bool, marked: bool) -> Entity {
        let mut light = app.world_mut().spawn((
            DirectionalLight {
                shadow_maps_enabled: casts,
                ..Default::default()
            },
            GlobalTransform::IDENTITY,
            CascadeShadowConfigBuilder {
                num_cascades: 3,
                first_cascade_far_bound: first_cascade,
                maximum_distance: first_cascade * 5.0,
                ..Default::default()
            }
            .build(),
        ));
        if marked {
            light.insert(GrassSun);
        }
        light.id()
    }

    fn said(app: &App) -> Vec<String> {
        app.world().resource::<ShadowSetupReport>().said.clone()
    }

    #[test]
    fn a_deep_first_cascade_is_reported_once() {
        let mut app = app(GrassShadows::Near, 0.012);
        sun(&mut app, 60.0, true, true);
        app.update();
        let said_now = said(&app);
        assert_eq!(said_now.len(), 1, "{said_now:?}");
        assert!(
            said_now[0].contains("first_cascade_far_bound"),
            "{said_now:?}"
        );
        // Nothing changed, nothing more to say; and a nudge to the same
        // light that says the same thing says it once.
        app.update();
        app.world_mut()
            .query::<&mut DirectionalLight>()
            .single_mut(app.world_mut())
            .unwrap()
            .illuminance += 1.0;
        app.update();
        assert_eq!(said(&app), said_now);
    }

    #[test]
    fn the_recommended_setup_is_quiet() {
        let mut app = app(GrassShadows::Near, 0.012);
        sun(&mut app, 12.0, true, true);
        app.update();
        assert!(said(&app).is_empty(), "{:?}", said(&app));
    }

    #[test]
    fn nothing_is_said_about_shadows_grass_does_not_cast() {
        let mut app = app(GrassShadows::Off, 0.012);
        sun(&mut app, 60.0, true, false);
        sun(&mut app, 60.0, true, false);
        app.update();
        assert!(said(&app).is_empty(), "{:?}", said(&app));
    }

    #[test]
    fn a_sun_and_a_moon_without_a_mark_are_reported_and_a_mark_settles_it() {
        let mut app = app(GrassShadows::Near, 0.06);
        let moon = sun(&mut app, 12.0, true, false);
        sun(&mut app, 12.0, true, false);
        app.update();
        let before = said(&app);
        assert_eq!(before.len(), 1, "{before:?}");
        assert!(before[0].contains("none carries GrassSun"), "{before:?}");

        // Marking one is the fix: the same problem is not said again, and
        // no new one appears.
        app.world_mut().entity_mut(moon).insert(GrassSun);
        app.update();
        assert_eq!(said(&app), before);
    }

    #[test]
    fn narrow_blades_make_a_cascade_that_wide_blades_forgive() {
        // The same 30 m cascade: fine for 6 cm blades, not for 1 cm ones.
        let mut wide = app(GrassShadows::Near, 0.06);
        sun(&mut wide, 30.0, true, true);
        wide.update();
        assert!(said(&wide).is_empty(), "{:?}", said(&wide));

        let mut narrow = app(GrassShadows::Near, 0.01);
        sun(&mut narrow, 30.0, true, true);
        narrow.update();
        assert_eq!(said(&narrow).len(), 1, "{:?}", said(&narrow));
    }
}

mod adapter {
    use bevy::MinimalPlugins;
    use bevy::app::App;
    use bevy::transform::components::Transform;

    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, GrassLightPlugin));
        configure_light_systems(&mut app);
        app
    }

    fn light(app: &mut App, casts: bool, looking: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                DirectionalLight {
                    shadow_maps_enabled: casts,
                    ..Default::default()
                },
                GlobalTransform::from(Transform::IDENTITY.looking_to(looking, Vec3::Y)),
            ))
            .id()
    }

    fn frame(app: &App) -> GrassLightFrame {
        *app.world().resource::<GrassLightFrame>()
    }

    #[test]
    fn the_frame_follows_the_light_grass_would_pick() {
        let mut app = app();
        app.update();
        assert_eq!(frame(&app), GrassLightFrame::default(), "no light yet");

        let moon = Vec3::new(0.3, -1.0, 0.0).normalize();
        let sun = Vec3::new(-0.5, -1.0, 0.2).normalize();
        light(&mut app, false, moon);
        let marked = light(&mut app, true, sun);
        app.update();
        let seen = frame(&app);
        // Only the sun casts shadows, so it is both; the moon came first but
        // casts none.
        assert!((seen.shadow_direction.unwrap() - sun).length() < 1e-5);
        assert!((seen.sun_direction.unwrap() - sun).length() < 1e-5);

        // With the sun gone, grass follows the moon, which casts no shadows.
        app.world_mut()
            .entity_mut(marked)
            .remove::<DirectionalLight>();
        app.update();
        let seen = frame(&app);
        assert_eq!(seen.shadow_direction, None);
        assert!((seen.sun_direction.unwrap() - moon).length() < 1e-5);
    }

    #[test]
    fn without_the_adapter_a_frame_the_game_writes_is_kept() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        configure_light_systems(&mut app);
        light(&mut app, true, Vec3::NEG_Y);
        let own = GrassLightFrame {
            shadow_direction: None,
            sun_direction: Some(Vec3::new(1.0, -1.0, 0.0)),
        };
        app.insert_resource(own);
        app.update();
        assert_eq!(frame(&app), own);
    }
}
