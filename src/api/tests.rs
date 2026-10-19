use bevy::math::Vec3;

use super::*;

#[test]
fn one_disc_covers_a_small_footprint_at_its_centre() {
    let center = Vec3::new(1.0, 2.0, 3.0);
    let (radius, spread) =
        GrassInteractorSpread::covering_aabb(center, Vec3::new(0.4, 0.9, 0.4), 2.0);

    assert_eq!(radius, 0.4_f32.hypot(0.4));
    assert_eq!(spread.offsets, vec![center]);
}

#[test]
fn a_long_footprint_gets_a_grid_within_the_cap() {
    let (radius, spread) =
        GrassInteractorSpread::covering_aabb(Vec3::ZERO, Vec3::new(5.0, 0.5, 2.0), 2.0);

    assert_eq!(radius, 2.0);
    assert!(spread.offsets.len() > 1);
    assert!(spread.offsets.len() <= GrassInteractorSpread::MAX_DISCS);
    for offset in spread.offsets {
        assert!(offset.x >= -5.0 && offset.x <= 5.0);
        assert_eq!(offset.y, 0.0);
        assert!(offset.z >= -2.0 && offset.z <= 2.0);
    }
}

#[test]
fn a_footprint_too_large_for_the_cap_is_sampled_coarsely() {
    let (radius, spread) =
        GrassInteractorSpread::covering_aabb(Vec3::ZERO, Vec3::splat(10_000.0), 2.0);

    assert_eq!(radius, 2.0);
    assert!(!spread.offsets.is_empty());
    assert!(spread.offsets.len() <= GrassInteractorSpread::MAX_DISCS);
}

#[test]
fn a_degenerate_footprint_is_one_disc_and_does_not_panic() {
    let (radius, spread) = GrassInteractorSpread::covering_aabb(Vec3::ZERO, Vec3::ZERO, 2.0);

    assert_eq!(radius, 0.0);
    assert_eq!(spread.offsets, vec![Vec3::ZERO]);
}

#[test]
fn non_finite_extents_and_radius_are_sanitized() {
    let (radius, spread) = GrassInteractorSpread::covering_aabb(
        Vec3::new(f32::NAN, f32::INFINITY, f32::NEG_INFINITY),
        Vec3::new(f32::NAN, f32::INFINITY, f32::NEG_INFINITY),
        f32::NAN,
    );

    assert!(radius.is_finite());
    assert!(!spread.offsets.is_empty());
    assert!(spread.offsets.iter().all(|offset| offset.is_finite()));
}

#[test]
fn a_zero_radius_covers_nothing() {
    let (radius, spread) = GrassInteractorSpread::covering_aabb(Vec3::ZERO, Vec3::splat(1.0), 0.0);

    assert_eq!(radius, 0.0);
    assert!(spread.offsets.is_empty());
}

#[test]
fn an_empty_spread_means_one_disc_at_the_origin() {
    assert_eq!(interactor_offsets(None), &[Vec3::ZERO]);
    assert_eq!(
        interactor_offsets(Some(&GrassInteractorSpread::new())),
        &[Vec3::ZERO]
    );
    let spread = GrassInteractorSpread {
        offsets: vec![Vec3::X],
    };
    assert_eq!(interactor_offsets(Some(&spread)), &[Vec3::X]);
}
