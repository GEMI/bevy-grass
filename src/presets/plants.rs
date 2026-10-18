use super::{GrassBlade, GrassStyle};

preset_group! {
/// The plants to mix in as species.
PLANTS {
    /// Thin stems with pale seed heads, like timothy or foxtail.
    SEED_HEADS => Self {
        blade: GrassBlade::SEED_HEADS,
        ..Self::SUMMER_TALL
    };
    /// White flowers on slender stems, like oxeye daisies.
    WHITE_FLOWERS => Self {
        blade: GrassBlade::WHITE_FLOWERS,
        density: 30.0,
        ..Self::SUMMER_TALL
    };
    /// Small yellow flowers, like buttercups.
    YELLOW_FLOWERS => Self {
        blade: GrassBlade::YELLOW_FLOWERS,
        ..Self::WHITE_FLOWERS
    };
    /// Low, broad, dark leaves lying over, like clover.
    CLOVER => Self {
        blade: GrassBlade::CLOVER,
        density: 120.0,
        ..Self::SUMMER_TALL
    };
    /// Wide, folded leaves that arch over, like plantain or dock.
    BROAD_LEAF => Self {
        blade: GrassBlade::BROAD_LEAF,
        density: 30.0,
        ..Self::SUMMER_TALL
    };
}
}
