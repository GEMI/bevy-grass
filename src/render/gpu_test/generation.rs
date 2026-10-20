use super::*;

#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_generation_matches_the_cpu_mirror() {
    generation_matches_the_cpu_mirror(crate::GrassSettings::HIGH);
}

/// Every distance, density and width a game can set reaches the shaders: the
/// GPU and the CPU mirror, which reads them back from the view uniform, still
/// agree blade for blade.
#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_generation_matches_the_cpu_mirror_with_other_distances_and_densities() {
    generation_matches_the_cpu_mirror(crate::GrassSettings {
        radius: 80.0,
        lod1_start: 0.1,
        lod2_start: 0.35,
        fade_start: 0.7,
        lod_blend: 9.0,
        lod2_density: 0.7,
        fade_density: 0.3,
        mid_range_density: 0.8,
        max_widening: 1.6,
        ..crate::GrassSettings::HIGH
    });
}

/// Every blade's root in world space, decoded from the first grass view's
/// instances.
fn gpu_roots(app: &mut App) -> Vec<Vec3> {
    let Some((view, words)) = gpu_words(app) else {
        return Vec::new();
    };
    let origin = view.instance_origin;
    let position =
        |q: u32, center: f32, range: f32| ((q as f32 / 65535.0) - 0.5) * 2.0 * range + center;
    words
        .iter()
        .map(|w| {
            Vec3::new(
                position(w[0] & 0xffff, origin.x, origin.w),
                position(w[1] & 0xffff, origin.y, view.limits.w),
                position(w[0] >> 16, origin.z, origin.w),
            )
        })
        .collect()
}

/// A field turned about Y is placed, culled and thinned blade for blade as
/// the CPU mirror turns it, and its blades fill the turned rectangle.
#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_a_turned_field_matches_the_cpu_mirror() {
    let turned = Transform::from_xyz(3.0, 0.5, -4.0).with_rotation(Quat::from_rotation_y(0.6));
    let mut app = headless_app_with(turned, GrassGround::Flat);
    generation_matches_the_cpu_mirror_in(
        {
            wait_for_grass(&mut app);
            let roots = gpu_roots(&mut app);
            let inverse = turned.rotation.inverse();
            let local = |root: Vec3| inverse * (root - turned.translation);
            // Blades lean and tufts pull, but roots stay in the rectangle.
            assert!(roots.iter().all(|&root| {
                let at = local(root);
                at.x.abs() <= 35.05 && at.z.abs() <= 25.05
            }));
            assert!(
                roots
                    .iter()
                    .any(|root| (root.x - 3.0).abs() > 35.5 || (root.z + 4.0).abs() > 25.5),
                "some blades lie outside the unturned rectangle"
            );
            app
        },
        crate::GrassSettings::HIGH,
    );
}

/// A mesh ground is baked to a heightmap: blades stand on the mesh, at its
/// height, and none grow where it does not reach.
#[test]
#[ignore = "needs a GPU: cargo test -- --ignored gpu"]
fn gpu_blades_grow_on_a_mesh_ground_and_only_on_it() {
    let mut app = headless_app();
    // A ledge 2 m up under the field's +X half; the −X half has no ground.
    let ledge = Mesh::from(Plane3d::default().mesh().size(35.0, 50.0))
        .translated_by(Vec3::new(17.5, 2.0, 0.0));
    let ledge = app.world_mut().resource_mut::<Assets<Mesh>>().add(ledge);
    let mut fields = app.world_mut().query::<&mut GrassField>();
    fields.single_mut(app.world_mut()).unwrap().ground = GrassGround::mesh(ledge);
    wait_for_grass(&mut app);
    // The tables are rebuilt from the bake within a frame or two.
    for _ in 0..3 {
        app.update();
    }

    let roots = gpu_roots(&mut app);
    assert!(roots.len() > 1_000, "{} blades", roots.len());
    // The field's centre is at x = 3; the bake has a half-cell (0.25 m)
    // seam where ground meets hole.
    assert!(
        roots.iter().all(|root| root.x > 3.0 - 0.3),
        "no blade grows past the mesh's edge"
    );
    assert!(
        roots.iter().all(|root| (root.y - 2.5).abs() < 0.02),
        "every blade stands on the ledge, 2 m above the field's 0.5"
    );
}
