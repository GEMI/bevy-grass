use super::*;

#[test]
fn the_deferred_default_is_read_through_reflection() {
    assert!(deferred_by_default(&DefaultOpaqueRendererMethod::deferred()));
    assert!(!deferred_by_default(&DefaultOpaqueRendererMethod::forward()));
    assert!(!deferred_by_default(&DefaultOpaqueRendererMethod::default()));
}

fn layout(label: &'static str) -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor {
        label: label.into(),
        entries: Vec::new(),
    }
}

fn view_layout() -> MeshPipelineViewLayout {
    MeshPipelineViewLayout {
        main_layout: layout("view"),
        binding_array_layout: layout("binding_array"),
        empty_layout: layout("empty"),
    }
}

#[test]
fn the_main_pass_keeps_bevys_view_groups_and_replaces_the_mesh_group() {
    let bevy = [layout("view"), layout("binding_array"), layout("mesh")];
    assert_eq!(
        main_pass_layout(&bevy, view_layout(), layout("grass")),
        [layout("view"), layout("binding_array"), layout("grass")]
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "no longer builds [view, binding array, mesh]")]
fn a_mesh_pipeline_with_its_groups_reordered_fails_loudly() {
    let bevy = [layout("binding_array"), layout("view"), layout("mesh")];
    let _ = main_pass_layout(&bevy, view_layout(), layout("grass"));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "(4 groups)")]
fn a_mesh_pipeline_with_another_group_fails_loudly() {
    let bevy = [
        layout("view"),
        layout("binding_array"),
        layout("mesh"),
        layout("new"),
    ];
    let _ = main_pass_layout(&bevy, view_layout(), layout("grass"));
}

fn names(defs: &[ShaderDefVal]) -> Vec<String> {
    defs.iter()
        .map(|def| match def {
            ShaderDefVal::Bool(name, true) => name.to_string(),
            ShaderDefVal::UInt(name, value) => format!("{name}={value}"),
            other => panic!("unexpected definition {other:?}"),
        })
        .collect()
}

#[test]
fn the_prepass_definitions_are_the_ones_bevys_pbr_modules_read() {
    let depth = MeshPipelineKey::DEPTH_PREPASS;
    assert_eq!(
        names(&prepass_shader_defs(depth, LookFeatures::default())),
        ["PREPASS_PIPELINE", "MATERIAL_BIND_GROUP=3", "DEPTH_PREPASS"]
    );
    assert_eq!(
        names(&prepass_shader_defs(
            depth | MeshPipelineKey::NORMAL_PREPASS | MeshPipelineKey::MOTION_VECTOR_PREPASS,
            LookFeatures::default(),
        )),
        [
            "PREPASS_PIPELINE",
            "MATERIAL_BIND_GROUP=3",
            "DEPTH_PREPASS",
            "NORMAL_PREPASS",
            "MOTION_VECTOR_PREPASS",
            "NORMAL_PREPASS_OR_DEFERRED_PREPASS",
            "MOTION_VECTOR_PREPASS_OR_DEFERRED_PREPASS",
        ]
    );
    // A deferred view sets the "or deferred" pair without the prepasses.
    assert_eq!(
        names(&prepass_shader_defs(
            depth | MeshPipelineKey::DEFERRED_PREPASS,
            LookFeatures::default(),
        )),
        [
            "PREPASS_PIPELINE",
            "MATERIAL_BIND_GROUP=3",
            "DEPTH_PREPASS",
            "DEFERRED_PREPASS",
            "NORMAL_PREPASS_OR_DEFERRED_PREPASS",
            "MOTION_VECTOR_PREPASS_OR_DEFERRED_PREPASS",
        ]
    );
}

#[test]
fn the_prepass_compiles_in_every_look_term_but_toon_shading() {
    for (feature, def) in LookFeatures::TERMS {
        let defs = names(&prepass_shader_defs(
            MeshPipelineKey::DEPTH_PREPASS,
            feature,
        ));
        assert_eq!(
            defs.iter().any(|name| name == def),
            def != "GRASS_TOON",
            "{def}"
        );
    }
}
