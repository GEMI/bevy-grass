# Presets

bevy_grass ships 36 presets, from lawn to prairie to wildflower meadow,
forest-floor moss and ferns, a Baltic bog, a reed marsh, dunes, pampas,
spinifex, frost, kelp, crystal and cel-shaded fields. Each is a `const` on
`GrassStyle` and a `.grass.ron` file in
[`assets/grass_styles`](../assets/grass_styles); [Styles](styles.md) shows
how to change one or load one from a file.

The pictures show every preset, two rows per picture, each on a square of
about 6 m of its own ground (sand, snow, peat, a pond, the seabed) under a
top-down ARPG camera. Run `cargo run --release --example showcase` for page 1;
`--page 2` to `--page 5` draw the rest (`--fixed-time --screenshot PATH` saves
one). To fly around the biome, fantasy and toon presets up close, run
`cargo run --release --example gallery`.

![Eight biome presets in square patches of their own ground, each with its name under it: REED_MARSH in standing water, PAMPAS, ELEPHANT_GRASS, RYE_AND_CORNFLOWERS, COTTON_BOG on dark peat, DUNE_MARRAM on sand, SPINIFEX on red earth and FROSTED_MEADOW on snow](images/showcase_biomes.jpg)

![Eight fantasy and toon presets in square patches, each with its name under it: KELP_BED under water, CORKSCREW_ALIEN, CRYSTAL_SPIRES, BLIGHTED, BURNT_REGROWTH in ash, LAVENDER in rows, CEL_FIELD and COZY_PASTEL](images/showcase_fantasy_toon.jpg)

![Six general presets of grass in square patches, each with its name under it: MEADOW, TALL_PRAIRIE, WHEAT, DRY_SAVANNA, LAWN and STYLIZED](images/showcase_general.jpg)

![Eight seasonal, wild and flowering presets in square patches, each with its name under it: SUMMER_TALL, AUTUMN_TALL, SUMMER_WILD, AUTUMN_WILD, WILDFLOWER_MEADOW, SEED_HEADS, WHITE_FLOWERS and YELLOW_FLOWERS](images/showcase_seasons_wild.jpg)

![Six forest-floor and plant presets in square patches, each with its name under it: FOREST_MOSS, FOREST_FERNS, FOREST_FLOOR, FOREST_BRACKEN, CLOVER and BROAD_LEAF](images/showcase_forest.jpg)

| ![A summer field gone wild: tufts, bare patches, seed heads and a few flowers among tall grass](images/wild.jpg) | ![Wildflower meadow](images/flowers.jpg) | ![The same wild field from a top-down ARPG camera](images/arpg.jpg) |
| --- | --- | --- |

## The groups

| Group | Presets |
| --- | --- |
| `GrassStyle::GENERAL` | `MEADOW` (the default), `TALL_PRAIRIE`, `WHEAT`, `DRY_SAVANNA`, `LAWN`, `STYLIZED` |
| `GrassStyle::SEASONS` | `SUMMER_TALL`, `AUTUMN_TALL`: the same tall field, 0.7–1.3 m, green and brown, to mix or blend between. For a lusher or paler variant, tint a field with `GrassFieldOverrides` |
| `GrassStyle::WILD` | `SUMMER_WILD`, `AUTUMN_WILD`, `WILDFLOWER_MEADOW`: every kind of variation |
| `GrassStyle::FOREST` | `FOREST_MOSS`, `FOREST_FERNS`, `FOREST_FLOOR`, `FOREST_BRACKEN`: short, wide forest-floor plants in clumps, with fallen leaves and ferns mixed in |
| `GrassStyle::PLANTS` | `SEED_HEADS`, `WHITE_FLOWERS`, `YELLOW_FLOWERS`, `CLOVER`, `BROAD_LEAF`: to grow alone, or to mix in as species through the `GrassBlade` of the same name (`GrassBlade::CLOVER`) |
| `GrassStyle::BIOMES` | `COTTON_BOG` (red sphagnum, sedge tussocks, cotton grass, pools), `REED_MARSH` (2 m reeds and cattails), `DUNE_MARRAM` (blue-grey clumps on sand), `RYE_AND_CORNFLOWERS` (rye with cornflowers and poppies), `PAMPAS` (arching fountains under cream plumes), `SPINIFEX` (stiff splayed hummocks), `ELEPHANT_GRASS` (2.4–3.6 m, to hide in), `FROSTED_MEADOW` (stiff, glinting, out of snow), `BURNT_REGROWTH` (charcoal stubble and new shoots), `LAVENDER` (bushes; plant them in rows with a striped density mask) |
| `GrassStyle::FANTASY` | `CRYSTAL_SPIRES` (rigid, faceted, glassy), `CORKSCREW_ALIEN` (ribbons twisted two turns, toon), `KELP_BED` (soft ribbons for under water; pair with a slow, lively wind), `BLIGHTED` (corrupted violet grass with bone-white heads) |
| `GrassStyle::TOON` | `CEL_FIELD` (chunky two-band tufts and flowers), `COZY_PASTEL` (soft pastel grass and flowers, four bands) |

`GrassBlade::COTTON_GRASS`, `CATTAIL`, `CORNFLOWER` and `POPPY` are the
plants the biomes mix in, ready to add to any style as species.

`GrassStyle::preset(name)` finds a preset by its constant's name, and
`GrassStyle::presets()` lists them all with their names.

## Adding a preset to the crate

Every preset is declared once, in [`src/presets/`](../src/presets), one file
per group. Adding one is a single entry in the `preset_group!` block for its
group: the constant, the group's `PRESETS` list, the group array and the
embedded asset are all generated from it, and `BLESS_STYLES=1 cargo test`
writes its `.grass.ron` file in `assets/grass_styles`. There is no second list
to update.
