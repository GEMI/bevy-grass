# Roadmap and scope

bevy_grass does one thing: **it puts grass on the ground, draws it, moves it
(wind and trampling), and lets the game read and change it.** Anything outside
that sentence is out of scope, however useful, so the crate stays small,
fast and easy to keep up to date.

## Keeping up with Bevy

We try to release a bevy_grass for each Bevy minor version **within two weeks
of its release**. The port comes first, whatever else is in progress.

- bevy_grass versions follow Bevy's: bevy_grass 0.20 is for Bevy 0.20, 0.21
  will be for Bevy 0.21. The patch number is ours.
- Only the latest Bevy minor gets new features. Fixes go to the previous one
  while it is reasonable to.
- Each port is checked by the full test suite, the golden images and the
  240 Hz gate before it is tagged ([Testing](testing.md)).
  [Porting to a new Bevy](porting.md) lists every place grass touches Bevy's
  renderer and the order to port them in.
- New fields in the design types (`GrassStyle`, `GrassBlade`,
  `GrassSettings`, `GrassShadows`, `GrassPlugin`, `GrassCamera`,
  `GrassFieldOverrides`) arrive in a minor release
  only, which is breaking anyway, and the CHANGELOG says what to add. Write
  their literals with `..default()` (or `..GrassSettings::HIGH`, a preset)
  and a new field never breaks your code. They are not `#[non_exhaustive]`,
  because that would forbid `..GrassSettings::HIGH` outside the crate.
  `.grass.ron` style files keep loading across releases: a missing field
  takes its default and a renamed one keeps its old name as an alias.

## Next

In order:

1. **Distribution.** bevy_grass is on crates.io. Next: listings on Bevy
   Assets and awesome-bevy, and contributor files: CONTRIBUTING, issue
   templates and good-first-issue labels.
2. **1.0.** Freeze the API, with the performance gate held.

## Done

- **Browser builds** (0.20.5): grass runs in browsers through WebGPU, in 10
  storage buffers per shader stage, with a hosted demo and a wasm build in
  CI. WebGL2 stays unsupported: grass needs compute shaders. Safari waits on
  Bevy: it rejects a shader in Bevy 0.20's standard material. See
  [Running in the browser](web.md).
- **Runtime editing and queries** (0.20.2): clear, paint, mow and
  restore grass through the density mask, brushes that edit as they move,
  regrowth, and density and height at any point on the CPU. See
  [Editing and reading the grass](editing.md).

- **Rotated fields** (0.20.2): a field follows its transform's rotation about
  Y.
- **Mesh terrain** (0.20.2): `GrassGround::Mesh` bakes a mesh into a
  heightmap on the CPU, once, so grass grows on terrain that is not a
  heightmap. See [Fields, terrain and visibility](terrain.md#grow-grass-on-a-mesh).
- **Rapier and Avian** (0.20.2): physics bodies trample the grass in the
  shape of their colliders.

## Non-goals

Requests for these are declined, so the crate stays one thing done well:

- **Other vegetation:** trees, shrubs, bushes, rocks, or scattering arbitrary
  meshes. Use a scattering crate alongside.
- **Terrain:** rendering or generating it. bevy_grass grows on yours.
- **Splatmaps,** or blending several styles across one field. Overlap fields
  with density masks instead.
- **Impostor cards** for distant grass. `FarGrassMaterial` already carries
  the colour and the gusts past the last blade.
- **WebGL2.**
- **Mesh shaders or meshlets** as a requirement. The frame is dominated by
  shading the blades' pixels, which they do not reduce; see the
  [declined proposal](proposals/meshlets-and-clumps.md).
- **Occlusion culling** of chunks behind hills. Culling acts on the 3–5% of
  grass time spent in compute, not on the pixel shading that dominates.
- **Simulating blades with physics.** Physics bodies trampling grass is in
  scope; blades as rigid or soft bodies are not.
- **2D.**
