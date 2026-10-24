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

/// What a field's generated image holds, texel by texel, as the field
/// systems read it.
fn texels_of(images: &Assets<Image>, handle: &Handle<Image>) -> (bevy::math::UVec2, Vec<Vec2>) {
    let image = images.get(handle).expect("the image was added");
    let size = image.size();
    let values = (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| (x, y)))
        .map(|(x, y)| {
            let color = bevy::color::LinearRgba::from(image.get_color_at(x, y).unwrap());
            Vec2::new(color.red, color.green)
        })
        .collect();
    (size, values)
}

#[test]
fn a_ground_from_a_function_is_its_heights_at_each_cell_centre() {
    let mut images = Assets::<Image>::default();
    let field = GrassField {
        size: Vec2::new(4.0, 2.0),
        ..Default::default()
    }
    .with_ground_from_fn(&mut images, 1.0, |point| {
        // No ground on the −X half, a slope on the rest.
        if point.x < 0.0 {
            f32::NAN
        } else {
            10.0 * point.x + point.y
        }
    });
    let GrassGround::Heightmap {
        image,
        height_scale,
    } = &field.ground
    else {
        panic!("a heightmap ground, not {:?}", field.ground);
    };
    assert_eq!(*height_scale, 1.0, "the heights are metres");
    let (size, texels) = texels_of(&images, image);
    assert_eq!(size, bevy::math::UVec2::new(4, 2), "one texel per metre");
    // Cell centres are at x = -1.5, -0.5, 0.5, 1.5 and z = -0.5, 0.5, the
    // first row at −Z.
    let heights: Vec<f32> = texels.iter().map(|texel| texel.x).collect();
    let hole = crate::ground::HOLE;
    assert_eq!(heights, [hole, hole, 4.5, 14.5, hole, hole, 5.5, 15.5]);
}

#[test]
fn a_density_from_a_function_is_a_mask_at_full_height() {
    let mut images = Assets::<Image>::default();
    let field = GrassField {
        size: Vec2::splat(2.0),
        ..Default::default()
    }
    .with_density_from_fn(&mut images, 1.0, |point| {
        match (point.x < 0.0, point.y < 0.0) {
            (true, true) => 0.0,
            (false, true) => 0.5,
            (true, false) => f32::NAN,
            (false, false) => 7.0,
        }
    });
    let mask = field.density_map.as_ref().expect("a mask");
    let (size, texels) = texels_of(&images, mask);
    assert_eq!(size, bevy::math::UVec2::splat(2));
    let densities: Vec<f32> = texels.iter().map(|texel| texel.x).collect();
    assert_eq!(
        densities,
        [0.0, 128.0 / 255.0, 0.0, 1.0],
        "NaN is 0, 7 is 1"
    );
    assert!(texels.iter().all(|texel| texel.y == 1.0), "full height");
}

#[test]
fn the_cells_never_exceed_4096_per_side() {
    let mut images = Assets::<Image>::default();
    let field = GrassField {
        size: Vec2::new(10_000.0, 1.0),
        ..Default::default()
    }
    .with_density_from_fn(&mut images, 0.5, |_| 1.0);
    let mask = images.get(field.density_map.as_ref().unwrap()).unwrap();
    assert_eq!(mask.size(), bevy::math::UVec2::new(4096, 2));
}
