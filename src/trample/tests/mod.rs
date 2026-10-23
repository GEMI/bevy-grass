use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use bevy::math::Quat;
use bevy::transform::components::Transform;

use super::*;

/// The default cell and interactor radius.
const CELL: f32 = 0.25;
const MAX_RADIUS: f32 = 2.0;

/// Disc stamps from `(x, z, radius, strength)`.
fn discs(all: &[Vec4]) -> Vec<Stamp> {
    all.iter()
        .map(|d| Stamp::disc(d.x, d.y, d.z, d.w))
        .collect()
}

/// The discs of the stamps a camera kept.
fn kept_discs(kept: Vec<Stamp>) -> Vec<Vec4> {
    kept.into_iter().map(|stamp| stamp.disc).collect()
}

#[test]
fn an_interactor_fades_out_as_it_rises_above_the_grass() {
    // No limit, or no ground to measure from: always full.
    assert_eq!(reach(None, 50.0, Some(0.0)), 1.0);
    assert_eq!(reach(Some(1.0), 50.0, None), 1.0);
    // Within the lower half of its range: full. At the top: none.
    assert_eq!(reach(Some(1.0), 0.0, Some(0.0)), 1.0);
    assert_eq!(reach(Some(1.0), 0.5, Some(0.0)), 1.0);
    assert_eq!(reach(Some(1.0), 1.0, Some(0.0)), 0.0);
    assert_eq!(reach(Some(1.0), 9.0, Some(0.0)), 0.0);
    // Between, it falls smoothly, and it never rises with height.
    let mut last = 1.0;
    for step in 0..=100 {
        let now = reach(Some(1.0), step as f32 * 0.01, Some(0.0));
        assert!((0.0..=1.0).contains(&now) && now <= last + 1e-6);
        last = now;
    }
    assert!((reach(Some(1.0), 0.75, Some(0.0)) - 0.5).abs() < 1e-6);
    // Below the ground it is as at the ground; the ground may be raised.
    assert_eq!(reach(Some(1.0), -2.0, Some(0.0)), 1.0);
    assert_eq!(reach(Some(1.0), 3.0, Some(3.0)), 1.0);
}

#[test]
fn the_window_contains_its_centre_with_room_on_every_side() {
    // The default cells, finer ones, and coarser ones for a wider window.
    for cell in [CELL, 0.05, 1.0, 2.0] {
        for center in [Vec2::ZERO, Vec2::new(-3.3, 1000.1), Vec2::new(12.6, -0.124)] {
            let origin = window_origin(center, cell).as_vec2() * cell;
            let side = window_side(cell);
            let into = center - origin;
            assert!(
                into.cmpge(Vec2::splat(side * 0.5 - cell)).all(),
                "{center} at {cell}"
            );
            assert!(
                into.cmple(Vec2::splat(side * 0.5 + cell)).all(),
                "{center} at {cell}"
            );
        }
    }
    assert_eq!(window_side(CELL), 64.0, "256 cells of 25 cm");
    assert_eq!(window_side(2.0), 512.0);
}

#[test]
fn an_orthographic_camera_centres_the_window_on_the_ground_it_sees() {
    let forward = Vec3::new(1.0, -1.0, 0.0).normalize();
    let position = Vec3::new(-40.0, 40.0, 5.0);
    let center = focus(position, forward, true, &FieldTables::default());
    assert!(center.distance(Vec2::new(0.0, 5.0)) < 1e-3, "{center}");
    // A perspective camera stands where it is.
    assert_eq!(
        focus(position, forward, false, &FieldTables::default()),
        position.xz()
    );
}

#[test]
fn the_cap_keeps_the_nearest_interactors() {
    let all: Vec<Vec4> = (0..100)
        .map(|i| Vec4::new(i as f32 * 0.3, 0.0, 0.5, 1.0))
        .collect();
    let kept = kept_discs(select_interactors(
        &discs(&all),
        Vec2::ZERO,
        10,
        CELL,
        MAX_RADIUS,
    ));
    assert_eq!(kept.len(), 10);
    assert!(kept.iter().all(|i| i.x < 3.0), "{kept:?}");
}

#[test]
fn interactors_outside_the_window_or_without_effect_are_skipped() {
    let all = [
        Vec4::new(0.0, 0.0, 0.5, 1.0),
        Vec4::new(500.0, 0.0, 0.5, 1.0),
        Vec4::new(1.0, 1.0, 0.0, 1.0),
        Vec4::new(1.0, 1.0, 0.5, 0.0),
        Vec4::new(2.0, 2.0, 9.0, 3.0),
    ];
    let kept = kept_discs(select_interactors(
        &discs(&all),
        Vec2::ZERO,
        64,
        CELL,
        MAX_RADIUS,
    ));
    assert_eq!(kept.len(), 2);
    assert!(kept.contains(&Vec4::new(2.0, 2.0, MAX_RADIUS, 1.0)));
}

#[test]
fn the_radius_cap_and_the_window_follow_the_settings() {
    let all = [
        Vec4::new(0.0, 0.0, 6.0, 1.0),
        // 50 m away: outside a 64 m window, inside a 512 m one.
        Vec4::new(50.0, 0.0, 0.5, 1.0),
    ];
    let kept = kept_discs(select_interactors(&discs(&all), Vec2::ZERO, 64, CELL, 8.0));
    assert_eq!(
        kept,
        vec![Vec4::new(0.0, 0.0, 6.0, 1.0)],
        "6 m fits under 8"
    );
    let kept = kept_discs(select_interactors(&discs(&all), Vec2::ZERO, 64, CELL, 1.0));
    assert_eq!(kept, vec![Vec4::new(0.0, 0.0, 1.0, 1.0)], "cut to 1 m");
    let kept = kept_discs(select_interactors(&discs(&all), Vec2::ZERO, 64, 2.0, 1.0));
    assert_eq!(kept.len(), 2, "a coarser window reaches further");
}

/// A footprint stamp under `transform`, at rest, pushing fully.
fn stamp_of(transform: Transform, footprint: GrassInteractorFootprint) -> Stamp {
    box_stamp(
        &GlobalTransform::from(transform),
        &footprint,
        1.0,
        Vec2::ZERO,
    )
}

/// A stamp's centre, axis, core half extents and rounding, for comparing.
fn shape_of(stamp: &Stamp) -> (Vec2, Vec2, Vec2, f32) {
    (
        stamp.disc.xy(),
        stamp.shape.xy(),
        stamp.shape.zw(),
        stamp.motion.z,
    )
}

fn close(a: Vec2, b: Vec2) -> bool {
    a.distance(b) < 1e-4
}

#[test]
fn an_upright_capsule_is_a_round_stamp() {
    let capsule = GrassInteractorFootprint {
        rounding: 0.4,
        ..GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(0.4, 0.9, 0.4))
    };
    let stamp = stamp_of(Transform::from_xyz(3.0, 0.9, -1.0), capsule);

    assert!(stamp.is_box());
    let (center, _, core, rounding) = shape_of(&stamp);
    assert_eq!(center, Vec2::new(3.0, -1.0));
    assert_eq!(core, Vec2::ZERO, "all rounding: a disc");
    assert_eq!(rounding, 0.4);
    // It pushes out to its radius and a margin.
    assert!(stamp.disc.z > 0.4 && stamp.disc.z < 0.4 + 0.61);
}

#[test]
fn a_box_is_one_stamp_of_its_size_wherever_it_is() {
    let cart = GrassInteractorFootprint::new(Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 1.0, 1.5));
    let stamp = stamp_of(Transform::from_xyz(10.0, 1.0, 5.0), cart);

    let (center, along, core, rounding) = shape_of(&stamp);
    assert_eq!(center, Vec2::new(10.0, 6.0), "the box's own centre, moved");
    assert_eq!(along, Vec2::X);
    assert_eq!(core, Vec2::new(3.0, 1.5));
    assert_eq!(rounding, 0.0);
    assert_eq!(stamp.motion.w, 0.6, "a big box pushes the most aside");
    assert!((stamp.disc.z - (3.0_f32.hypot(1.5) + 0.6)).abs() < 1e-5);
}

#[test]
fn turning_a_box_turns_its_stamp_and_keeps_its_size() {
    let cart = GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(3.0, 1.0, 1.5));
    for yaw in [0.3, FRAC_PI_2, 2.0, -2.5] {
        let turned = Transform::from_rotation(Quat::from_rotation_y(yaw));
        let stamp = stamp_of(turned, cart);
        let (_, along, core, _) = shape_of(&stamp);
        let expected = turned.rotation * Vec3::X;
        // The axis is the long side, either way along it.
        assert!(
            along.dot(Vec2::new(expected.x, expected.z)).abs() > 0.9999,
            "yaw {yaw}: {along}"
        );
        assert!(close(core, Vec2::new(3.0, 1.5)), "yaw {yaw}: {core}");
    }
}

#[test]
fn a_scaled_box_is_stamped_at_its_scaled_size() {
    // A 1 m crate scaled to 2.5 m.
    let crate_box = GrassInteractorFootprint::default();
    let stamp = stamp_of(Transform::from_scale(Vec3::splat(2.5)), crate_box);
    assert!(close(stamp.shape.zw(), Vec2::splat(1.25)));

    // A 1 m cube stretched to a 10 × 0.5 m wall.
    let stamp = stamp_of(Transform::from_scale(Vec3::new(10.0, 2.0, 0.5)), crate_box);
    let (_, along, core, _) = shape_of(&stamp);
    assert_eq!(along, Vec2::X);
    assert!(close(core, Vec2::new(5.0, 0.25)), "{core}");
}

#[test]
fn a_capsule_that_falls_over_is_stamped_along_its_length() {
    // A 4 m log, 0.6 m thick, along its local Y.
    let log = GrassInteractorFootprint {
        rounding: 0.3,
        ..GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(0.3, 2.0, 0.3))
    };
    let standing = stamp_of(Transform::IDENTITY, log);
    assert_eq!(standing.shape.zw(), Vec2::ZERO);

    // It falls along X, then rolls to lie along Z.
    for (fallen, length_along) in [
        (Quat::from_rotation_z(FRAC_PI_2), Vec2::X),
        (Quat::from_rotation_x(FRAC_PI_2), Vec2::Y),
    ] {
        let stamp = stamp_of(Transform::from_rotation(fallen), log);
        let (_, along, core, rounding) = shape_of(&stamp);
        assert!(along.dot(length_along).abs() > 0.9999, "{along}");
        assert!(close(core, Vec2::new(1.7, 0.0)), "{core}");
        assert_eq!(rounding, 0.3);
    }
}

#[test]
fn a_box_tipped_halfway_covers_its_whole_shadow() {
    // A 1 × 6 × 1 m pillar leaning 45° along X: its shadow runs from the
    // tip of one top corner to the opposite bottom corner.
    let pillar = GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(0.5, 3.0, 0.5));
    let stamp = stamp_of(
        Transform::from_rotation(Quat::from_rotation_z(FRAC_PI_4)),
        pillar,
    );
    let shadow = (3.0 + 0.5) * FRAC_PI_4.cos();
    assert!(
        close(stamp.shape.zw(), Vec2::new(shadow, 0.5)),
        "{}",
        stamp.shape
    );
}

#[test]
fn a_huge_box_is_clipped_and_nonsense_stays_finite() {
    let wall = GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(500.0, 1.0, 0.5));
    let stamp = stamp_of(Transform::IDENTITY, wall);
    assert_eq!(stamp.shape.z, GrassSettings::MAX_INTERACTOR_RADIUS);

    let broken = GrassInteractorFootprint {
        center: Vec3::ZERO,
        half_extents: Vec3::new(f32::NAN, f32::INFINITY, -2.0),
        rounding: f32::NAN,
    };
    let stamp = stamp_of(Transform::from_scale(Vec3::new(1.0, 0.0, 1.0)), broken);
    for value in stamp.gpu() {
        assert!(value.is_finite(), "{value}");
    }
    assert!(stamp.is_box());
}

#[test]
fn boxes_keep_their_size_when_selected_and_discs_are_capped() {
    let cart = stamp_of(
        Transform::IDENTITY,
        GrassInteractorFootprint::new(Vec3::ZERO, Vec3::new(6.0, 1.0, 3.0)),
    );
    let kept = select_interactors(
        &[cart, Stamp::disc(0.0, 0.0, 9.0, 1.0)],
        Vec2::ZERO,
        64,
        CELL,
        MAX_RADIUS,
    );
    assert_eq!(kept.len(), 2);
    assert!(kept.contains(&cart), "a box is not cut to the disc limit");
    assert!(kept.iter().any(|stamp| stamp.disc.z == MAX_RADIUS));
}

#[test]
fn a_moving_box_gets_its_direction_of_travel() {
    let mut tracked = Tracked::default();
    assert_eq!(tracked.motion(), Vec2::ZERO, "at rest");
    // Two metres a second along +Z, at 60 frames a second.
    for frame in 1..=30 {
        tracked = tracked.update(Vec2::new(0.0, frame as f32 * 2.0 / 60.0), 1.0 / 60.0);
    }
    let motion = tracked.motion();
    assert!(close(motion, Vec2::Y), "{motion}");

    // Slowly: a little motion, in the same direction.
    let mut slow = Tracked::default();
    for frame in 1..=60 {
        slow = slow.update(Vec2::new(frame as f32 * 0.1 / 60.0, 0.0), 1.0 / 60.0);
    }
    let motion = slow.motion();
    assert!(
        motion.x > 0.15 && motion.x < 0.25 && motion.y.abs() < 1e-6,
        "{motion}"
    );

    // A teleport is not a motion; a paused frame changes nothing.
    let jumped = tracked.update(Vec2::new(100.0, 0.0), 1.0 / 60.0);
    assert_eq!(jumped.motion(), Vec2::ZERO);
    assert_eq!(tracked.update(tracked.at, 0.0).motion(), tracked.motion());
}
