use super::*;

const CHUNK: f32 = 16.0;

#[test]
fn hash_values_are_pinned() {
    // Pinned so that any change to the hash, which would move every blade,
    // is deliberate; the WESL `hash` must produce the same numbers.
    assert_eq!(hash(0), 129_708_002);
    assert_eq!(hash(1), 2_831_084_092);
}

#[test]
fn the_tile_is_identical_on_every_call() {
    let a = best_candidate_tile(256, 12, 7);
    let b = best_candidate_tile(256, 12, 7);
    assert_eq!(a, b);
}

#[test]
fn every_prefix_of_the_tile_is_evenly_spread() {
    let tile = blue_noise_tile();
    for prefix in [64usize, 256, 1024] {
        let points = &tile[..prefix];
        // For n points evenly spread over a unit torus the nearest-neighbour
        // distance is about 0.9 / sqrt(n); white noise has many pairs far
        // closer. Require every point to keep at least 40% of that spacing.
        let floor = 0.4 * 0.9 / (prefix as f32).sqrt();
        for (i, a) in points.iter().enumerate() {
            for b in &points[i + 1..] {
                assert!(
                    toroidal_distance_squared(*a, *b).sqrt() >= floor,
                    "two of the first {prefix} points are closer than {floor}"
                );
            }
        }
    }
}

#[test]
fn the_same_inputs_give_the_same_blade() {
    let layout = ChunkLayout::for_density(CHUNK, 60.0);
    let all = layout.cells() * TILE_POINTS;
    for index in [0, 1, 999, all - 1] {
        assert_eq!(
            candidate([3, 9], 42, &layout, index),
            candidate([3, 9], 42, &layout, index)
        );
    }
}

#[test]
fn candidates_stay_inside_their_chunk() {
    let layout = ChunkLayout::for_density(CHUNK, 60.0);
    for index in 0..layout.cells() * TILE_POINTS {
        let c = candidate([1, 2], 5, &layout, index);
        assert!(c.local.cmpge(Vec2::ZERO).all() && c.local.cmplt(Vec2::splat(CHUNK)).all());
    }
}

#[test]
fn rank_never_decreases_with_the_candidate_index() {
    let layout = ChunkLayout::for_density(CHUNK, 60.0);
    let ranks: Vec<f32> = (0..layout.cells() * TILE_POINTS)
        .map(|i| candidate([0, 0], 0, &layout, i).rank)
        .collect();
    assert!(ranks.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn the_needed_prefix_holds_every_kept_blade() {
    let layout = ChunkLayout::for_density(CHUNK, 80.0);
    for keep in [0.0, 0.01, 0.137, 0.5, 0.999, 1.0] {
        let needed = layout.candidates_needed(keep);
        for index in needed..layout.cells() * TILE_POINTS {
            assert!(
                candidate([4, 4], 9, &layout, index).rank >= keep,
                "keep {keep}, index {index}"
            );
        }
    }
}

#[test]
fn lowering_density_only_removes_blades_in_rank_order() {
    let layout = ChunkLayout::for_density(CHUNK, 80.0);
    let kept = |density: f32| -> Vec<u32> {
        let keep = keep_fraction(density, 1.0, &layout);
        (0..layout.cells() * TILE_POINTS)
            .filter(|&i| candidate([0, 0], 1, &layout, i).rank < keep)
            .collect()
    };
    let mut previous = kept(80.0);
    for density in [60.0, 40.0, 20.0, 5.0, 0.0] {
        let now = kept(density);
        assert!(
            now.iter().all(|i| previous.contains(i)),
            "a blade appeared at {density}"
        );
        previous = now;
    }
    assert!(previous.is_empty());
}

#[test]
fn every_style_density_is_reached() {
    for tenth in 1..2000 {
        let density = tenth as f32 / 10.0;
        let layout = ChunkLayout::for_density(CHUNK, density);
        assert!(
            layout.candidate_density >= density * (1.0 - 1e-5),
            "{density}: {layout:?}"
        );
        let drawn = keep_fraction(density, 1.0, &layout) * layout.candidate_density;
        assert!(
            (drawn - density).abs() < 1e-3 * density,
            "{density}: draws {drawn}"
        );
    }
}

#[test]
fn falloff_is_continuous_and_never_increases() {
    let lod = DistanceLod::for_radius(100.0);
    let mut previous = lod.falloff(0.0);
    assert_eq!(previous, 1.0);
    let mut d = 0.0;
    while d <= 110.0 {
        let now = lod.falloff(d);
        assert!(now <= previous + 1e-6, "falloff rose at {d}");
        assert!((previous - now).abs() < 0.02, "falloff jumps at {d}");
        previous = now;
        d += 0.1;
    }
    assert_eq!(lod.falloff(100.0), 0.0);
    // The architecture's table (50–100% across LOD 1, 15–50% across
    // LOD 2), halved from LOD 1's band on.
    assert!((lod.falloff(17.0) - 1.0).abs() < 1e-6);
    assert!((lod.falloff(50.0) - 0.25).abs() < 1e-6);
    assert!((lod.falloff(90.0) - 0.075).abs() < 1e-6);
}

#[test]
fn lod_switches_only_inside_the_dither_band() {
    let lod = DistanceLod::for_radius(100.0);
    for dither in [0.0, 0.3, 0.7, 0.999] {
        assert_eq!(lod.lod(17.9, dither), 0);
        assert_eq!(lod.lod(22.1, dither), 1);
        assert_eq!(lod.lod(47.9, dither), 1);
        assert_eq!(lod.lod(52.1, dither), 2);
    }
    // Inside the band, a blade's LOD never goes back down as it recedes.
    for dither in [0.1, 0.5, 0.9] {
        let lods: Vec<u32> = (0..100)
            .map(|i| lod.lod(18.0 + i as f32 * 0.04, dither))
            .collect();
        assert!(lods.windows(2).all(|w| w[0] <= w[1]));
    }
}

#[test]
fn coverage_widening_matches_the_architecture() {
    assert_eq!(coverage_width(1.0, 2.5), 1.0);
    assert!((coverage_width(0.5, 2.5) - 2f32.sqrt()).abs() < 1e-6);
    assert_eq!(coverage_width(0.01, 2.5), 2.5);
    // A game can allow more or less widening.
    assert_eq!(coverage_width(0.01, 1.0), 1.0);
    assert_eq!(coverage_width(0.01, 3.0), 3.0);
    assert!((coverage_width(0.25, 3.0) - 2.0).abs() < 1e-6);
}

#[test]
fn clumps_are_patches_about_a_metre_across() {
    // Neighbouring roots mostly share a clump; roots metres apart rarely do.
    let same = |gap: f32| {
        (0..2000)
            .filter(|&i| {
                let p = Vec2::new(unit(hash(i)) * 200.0, unit(hash(i ^ 9)) * 200.0);
                clump_of(p) == clump_of(p + Vec2::new(gap, 0.0))
            })
            .count() as f32
            / 2000.0
    };
    assert!(same(0.1) > 0.8, "{}", same(0.1));
    assert!(same(3.0) < 0.1, "{}", same(3.0));
    // Every tint is used.
    let mut seen = [false; 64];
    for i in 0..20_000 {
        seen[clump_of(Vec2::new(unit(hash(i)) * 300.0, unit(hash(i ^ 3)) * 300.0)) as usize] = true;
    }
    assert!(seen.iter().all(|&s| s));
    // The table repeats seamlessly every 64 m.
    for i in 0..2000 {
        let p = Vec2::new(unit(hash(i)) * 64.0, unit(hash(i ^ 5)) * 64.0);
        assert_eq!(clump_of(p), clump_of(p + Vec2::new(64.0, -128.0)));
    }
}

#[test]
fn packing_round_trips_within_quantization() {
    let blade = Blade {
        position: Vec3::new(12.345, -3.21, -101.5),
        facing: 0.3,
        height: 0.75,
        width: 0.2,
        seed: 201,
        lod: 2,
        field: 17,
        clump: 45,
        bend: Vec2::new(-0.5, 0.25),
        previous_bend: Vec2::new(-0.47, 0.21),
    };
    let extent = 128.0;
    let back = unpack(pack(&blade, extent), extent);
    let horizontal = |v: Vec3| Vec2::new(v.x, v.z);
    assert!(
        (horizontal(back.position) - horizontal(blade.position))
            .abs()
            .max_element()
            < 2.0 * extent / 65_535.0
    );
    assert!((back.position.y - blade.position.y).abs() < 2.0 * HEIGHT_RANGE / 65_535.0);
    assert!((back.facing - blade.facing).abs() <= 1.0 / 256.0);
    assert!((back.height - blade.height).abs() <= 0.5 / 255.0);
    assert!((back.width - blade.width).abs() <= 0.5 / 255.0);
    assert_eq!(
        (back.seed, back.lod, back.field, back.clump),
        (blade.seed, blade.lod, blade.field, blade.clump)
    );
    assert!((back.bend - blade.bend).abs().max_element() <= 1.0 / 4095.0 + 1e-6);
    assert!(
        (back.previous_bend - blade.previous_bend)
            .abs()
            .max_element()
            <= 1.0 / 4095.0 + 0.5 / BEND_DELTA_STEPS + 1e-6
    );
}

#[test]
fn a_large_change_in_bend_is_clamped_not_wrapped() {
    for (bend, previous) in [(0.0, 0.9), (0.0, -0.9), (1.0, -1.0), (-1.0, 1.0)] {
        let (now, back) = unpack_bend(pack_bend(bend, previous));
        // Half a 12-bit step, 1 / 4095, and a little for rounding.
        assert!((now - bend).abs() <= 1.0 / 4095.0 + 1e-6);
        // The furthest step toward last frame's bend.
        assert!((back - previous).abs() < (previous - bend).abs());
        assert!((back - now).abs() <= 8.0 / BEND_DELTA_STEPS);
    }
}

fn curve_length(points: &[Vec3; 3]) -> f32 {
    let steps = 256;
    (0..steps)
        .map(|i| {
            let a = bezier(points, i as f32 / steps as f32);
            let b = bezier(points, (i + 1) as f32 / steps as f32);
            a.distance(b)
        })
        .sum()
}

#[test]
fn a_bent_blade_keeps_its_length() {
    for height in [0.1, 0.5, 1.4] {
        for bend in [0.0, 0.3, 0.6, 0.9, 1.5] {
            for lean in [0.0, 0.2] {
                let curve = blade_curve(height, lean, Vec2::new(bend, 0.0), 0.0);
                let length = curve_length(&curve);
                assert!(
                    (length - height).abs() <= height * 0.01,
                    "height {height}, bend {bend}, lean {lean}: length {length}"
                );
            }
        }
    }
}

#[test]
fn a_drooping_blade_arches_over_and_keeps_its_length() {
    for height in [0.1, 0.5, 1.4] {
        for droop in [0.05, 0.25, 0.5, 1.0] {
            for (lean, bend) in [(0.02, 0.0), (0.1, 0.0), (0.3, 0.0), (0.1, 0.6), (0.0, -0.9)] {
                let curve = blade_curve(height, lean, Vec2::new(bend, 0.0), droop);
                // The length correction assumes a gentle arch; a
                // drooping blade's tighter curve comes out within 6%,
                // which blades varying ±50% in height hide.
                let length = curve_length(&curve);
                assert!(
                    (length - height).abs() <= height * 0.06,
                    "height {height}, droop {droop}: length {length}"
                );
                // From half on, the tip hangs below the blade's highest
                // point.
                let top = (0..=64)
                    .map(|i| bezier(&curve, i as f32 / 64.0).y)
                    .fold(0.0, f32::max);
                assert!(droop < 0.5 || curve[2].y < top - height * 0.05 * droop);
            }
        }
    }
    // A blade with no lean or bend has no side to droop toward: it stays
    // upright.
    assert_eq!(
        blade_curve(1.0, 0.0, Vec2::ZERO, 0.6),
        blade_curve(1.0, 0.0, Vec2::ZERO, 0.0)
    );
}

#[test]
fn tufts_are_the_nearest_jittered_centres() {
    // Tuft centres sit one per tuft spacing, jittered, and every point
    // belongs to its nearest centre, give or take the table's rounding.
    for i in 0..4000 {
        let q = Vec2::new(unit(hash(i)), unit(hash(i ^ 7))) * 200.0 - 100.0;
        let (centre, _) = tuft_at(q);
        assert!(q.distance(centre) < 1.5, "{q} -> {centre}");
        let entry = (q * TUFT_ENTRIES_PER_TUFT as f32).floor() + 0.5;
        let entry = entry / TUFT_ENTRIES_PER_TUFT as f32;
        let (entry_centre, _) = tuft_at(entry);
        assert!(entry_centre.distance(centre) < 1e-4);
    }
    // The table repeats every 64 tufts.
    let (a, ra) = tuft_at(Vec2::new(3.3, 7.9));
    let (b, rb) = tuft_at(Vec2::new(3.3 + 64.0, 7.9 - 128.0));
    assert!((b - a - Vec2::new(64.0, -128.0)).length() < 1e-3);
    assert_eq!(ra, rb);
}

#[test]
fn bare_patches_cover_their_share() {
    let noise = gust_noise(GUST_SIZE);
    for share in [0.05, 0.2, 0.5] {
        let threshold = bare_threshold(share);
        let bare = (0..20_000)
            .filter(|&i| {
                let at = Vec2::new(unit(hash(i ^ 0x51)), unit(hash(i ^ 0x52))) * 500.0;
                bare_noise(&noise, at, 3.0) < threshold
            })
            .count() as f32
            / 20_000.0;
        assert!((bare - share).abs() < 0.03, "{share}: {bare}");
    }
    assert!(bare_threshold(0.0) <= bare_threshold(0.3));
}

#[test]
fn species_take_their_share_scattered_or_clustered() {
    // Two species besides the style's own blades: 10% and 3%.
    for clustering in [0.0, 0.8, 1.0] {
        let chances = [(0.1, clustering), (0.03 / 0.9, clustering)];
        let mut counts = [0usize; 3];
        let blades = 200_000;
        for i in 0..blades {
            let clump = hash(i ^ 0x77) & 63;
            counts[pick_species(hash(i), clump, &chances)] += 1;
        }
        let share = |n: usize| n as f32 / blades as f32;
        assert!(
            (share(counts[1]) - 0.1).abs() < 0.01,
            "{clustering}: {counts:?}"
        );
        assert!(
            (share(counts[2]) - 0.03).abs() < 0.005,
            "{clustering}: {counts:?}"
        );
    }
    // Clustered, a species fills some clumps and leaves the rest.
    let chances = [(0.1, 1.0)];
    let mut per_clump = [0usize; 64];
    for i in 0..64_000 {
        let clump = i % 64;
        if pick_species(hash(i), clump, &chances) == 1 {
            per_clump[clump as usize] += 1;
        }
    }
    let empty = per_clump.iter().filter(|&&n| n == 0).count();
    assert!(empty >= 56, "{per_clump:?}");
}

#[test]
fn lods_have_the_planned_vertex_and_triangle_counts() {
    let expected = [(15, 13), (7, 5), (3, 1)];
    for (rows, (vertices, triangles)) in GrassSettings::HIGH.blade_rows.iter().zip(expected) {
        let indices = lod_indices(*rows);
        assert_eq!(indices.len() / 3, triangles);
        assert_eq!(indices.iter().max().copied().unwrap() + 1, vertices);
        assert!(vertices < LOD_VERTEX_STRIDE);
    }
    // The most rows a game may ask for still fit the stride.
    let most = lod_indices(GrassSettings::MAX_BLADE_ROWS);
    assert!(most.iter().max().copied().unwrap() < LOD_VERTEX_STRIDE);
}

#[test]
fn width_range_covers_variation_times_widening() {
    assert!(WIDTH_RANGE >= 1.25 * coverage_width(0.0, GrassSettings::MAX_WIDENING));
}
#[test]
fn trample_pushes_round_trip_within_quantization() {
    for i in 0..1000 {
        let angle = unit(hash(i)) * TAU - std::f32::consts::PI;
        let push = Vec2::from_angle(angle) * unit(hash(i ^ 0x55));
        let back = trample_unpack(trample_pack(push));
        assert!(back.distance(push) < 2e-3, "{push} came back as {back}");
    }
}

#[test]
fn the_larger_packed_word_is_the_stronger_push() {
    for i in 0..1000 {
        let a = Vec2::from_angle(unit(hash(i)) * TAU) * unit(hash(i ^ 1));
        let b = Vec2::from_angle(unit(hash(i ^ 2)) * TAU) * unit(hash(i ^ 3));
        let (pa, pb) = (trample_pack(a), trample_pack(b));
        if (a.length() - b.length()).abs() > 1.0 / 1023.0 {
            assert_eq!(pa > pb, a.length() > b.length(), "{a} vs {b}");
        }
    }
}

#[test]
fn every_cell_of_a_window_has_its_own_slot() {
    for origin in [IVec2::ZERO, IVec2::new(-300, 17), IVec2::new(1000, -1)] {
        let mut seen = vec![false; (TRAMPLE_TEXELS * TRAMPLE_TEXELS) as usize];
        for z in 0..TRAMPLE_TEXELS {
            for x in 0..TRAMPLE_TEXELS {
                let cell = origin + IVec2::new(x, z);
                let slot = trample_slot(cell);
                assert!(!seen[slot as usize], "two cells share slot {slot}");
                seen[slot as usize] = true;
                let slot_xy =
                    IVec2::new(slot as i32 % TRAMPLE_TEXELS, slot as i32 / TRAMPLE_TEXELS);
                assert_eq!(trample_cell(slot_xy, origin), cell);
                assert!(in_trample_window(cell, origin));
            }
        }
    }
    assert!(!in_trample_window(IVec2::new(256, 0), IVec2::ZERO));
    assert!(!in_trample_window(IVec2::new(0, -1), IVec2::ZERO));
}
