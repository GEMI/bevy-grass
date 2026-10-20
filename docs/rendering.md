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
