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
