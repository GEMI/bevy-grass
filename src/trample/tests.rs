use super::*;

/// The default cell and interactor radius.
const CELL: f32 = 0.25;
const MAX_RADIUS: f32 = 2.0;

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
    let kept = select_interactors(&all, Vec2::ZERO, 10, CELL, MAX_RADIUS);
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
    let kept = select_interactors(&all, Vec2::ZERO, 64, CELL, MAX_RADIUS);
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
    let kept = select_interactors(&all, Vec2::ZERO, 64, CELL, 8.0);
    assert_eq!(
        kept,
        vec![Vec4::new(0.0, 0.0, 6.0, 1.0)],
        "6 m fits under 8"
    );
    let kept = select_interactors(&all, Vec2::ZERO, 64, CELL, 1.0);
    assert_eq!(kept, vec![Vec4::new(0.0, 0.0, 1.0, 1.0)], "cut to 1 m");
    let kept = select_interactors(&all, Vec2::ZERO, 64, 2.0, 1.0);
    assert_eq!(kept.len(), 2, "a coarser window reaches further");
}
