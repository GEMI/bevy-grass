# Shadows and rendering paths

## Shadows

Grass receives shadows from every light. It casts them for one directional
light, set by `GrassSettings::shadows` (`Off`, `Near`, `Full`), and a blade is
only a centimetre or two wide, so how that light's cascades are set up decides
whether blade shadows look right. Use a shallow first cascade, and mark the
light grass should follow with `GrassSun`:

```rust
commands.spawn((
    DirectionalLight { shadow_maps_enabled: true, ..default() },
    CascadeShadowConfigBuilder {
        num_cascades: 3,
        first_cascade_far_bound: 12.0,
        maximum_distance: 60.0,
        ..default()
    }
    .build(),
    GrassSun,
));
```

- **Why 12 m.** Grass draws its shadows into the first cascade (`Near`) or the
  first two (`Full`). A shadow texel is about `1.5 × first cascade depth ÷
  shadow map size`: with one cascade over 60 m in a 2048 map that is 4 to
  5 cm, wider than a blade, so each blade's shadow switches on and off as it
  sways across a texel centre and blade shadows blink. At 12 m a texel is
  under 1 cm.
- **Which light.** With a sun and a moon there are two directional lights.
  Without `GrassSun`, grass follows the first one that casts shadows (the one
  spawned first); with it, the marked one, and a marked light that casts no
  shadows means grass casts none. A deferred camera also treats the marked
  light as the sun.
- **The log helps.** The plugin warns, once, when the first cascade is deeper
  than the narrowest blades allow (it says how deep it can be), when several
  lights cast shadows and none is marked, when a marked light does not cast
  shadows, and when more than one light is marked.

## Rendering paths

| Camera has | Grass is drawn into |
| --- | --- |
| Nothing extra | The main opaque pass |
| `DepthPrepass`, `NormalPrepass`, `MotionVectorPrepass` | The prepass too: SSAO, TAA, SSR and anything else reading it see grass |
| `DeferredPrepass`, with `DefaultOpaqueRendererMethod::deferred()` | The G-buffer, lit by Bevy's deferred lighting pass |

Deferred grass matches forward grass closely. It approximates translucency
less well, and toon bands are forward-only. Thin blades shimmer without
anti-aliasing; TAA is recommended, and grass supplies exact motion vectors
for it. The [`passes`](../examples/passes.rs) example shows each path.

## Bind groups and the storage-buffer budget

Every grass pipeline binds at most 10 storage buffers in a shader stage: what
98% of WebGPU devices offer (the spec promises 8). A device that offers fewer
draws no grass and logs one warning; `GrassSupport` says why. A unit test keeps
every pipeline within 10, and the GPU tests run on a device lowered to 10.

| Pipeline | Group 0 | Group 1 | Group 2 | Storage buffers |
| --- | --- | --- | --- | --- |
| `cull`, `generate` | fields, chunks, scene, tile, clumps, tufts | view, visible slices, counters, instances, trample cells | none | 10 |
| `size_dispatch`, `write_draw_args` | the same 6 | view, counters, draw arguments | dispatch arguments (`size_dispatch`) | 9 |
| `trample_decay`, `trample_stamp` | the same 6 | view, trample cells, interactors | none | 8 |
| Grass draws | Bevy's view groups | Bevy's | frame, view, instances, fields, noise, wind | 2, in the vertex stage only |

The scene buffer holds every density mask and then every heightmap, so the
two share one binding. The draws' storage buffers are visible to the vertex
stage only: the blade's look is worked out per vertex, and the fragment
shaders read only the grass view uniform, so grass adds nothing to Bevy's own
fragment-stage storage buffers.

`generate` is exactly at the budget. A feature that needs another buffer
there has two ways to make room: one buffer for the tile, clump and tuft
tables (frees 2), or the visible slices at the start of the instance buffer
(frees 1).
