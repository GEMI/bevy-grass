use std::fmt::Write as _;

use super::*;

/// The lines around the generated table in `docs/presets.md`.
const TABLE_START: &str =
    "<!-- preset numbers: written by `BLESS_STYLES=1 cargo test`, do not edit -->";
const TABLE_END: &str = "<!-- end of preset numbers -->";

/// The table of every preset's sizes, from the constants, so the page a game
/// sizes its grass from cannot drift from the code.
fn preset_table() -> String {
    let mut table = String::from(
        "| Preset | Blade height (m) | Tallest plant (m) | Blades per m² | Spacing (cm) | Plants mixed in |\n\
         | --- | --- | --- | --- | --- | --- |\n",
    );
    for (name, style) in all_presets() {
        let tallest = style
            .species
            .iter()
            .map(|species| species.blade.height.y)
            .fold(style.blade.height.y, f32::max);
        let plants = match style.species.len() {
            0 => "none".to_owned(),
            count => {
                let share: f32 = style.species.iter().map(|species| species.share).sum();
                format!("{count}, {:.0}% of blades", share.min(1.0) * 100.0)
            }
        };
        writeln!(
            table,
            "| `{name}` | {:.2}–{:.2} | {tallest:.2} | {:.0} | {:.1} | {plants} |",
            style.blade.height.x,
            style.blade.height.y,
            style.density,
            style.spacing() * 100.0,
        )
        .expect("writing to a String cannot fail");
    }
    table
}

#[test]
fn the_preset_numbers_in_the_docs_match_the_constants() {
    // `BLESS_STYLES=1 cargo test` rewrites the table from the constants.
    let path = format!("{}/docs/presets.md", env!("CARGO_MANIFEST_DIR"));
    // The crates.io package leaves `docs/` out; there is nothing to check.
    let Ok(page) = std::fs::read_to_string(&path) else {
        return;
    };
    let start = page
        .find(TABLE_START)
        .expect("docs/presets.md has the table's start line")
        + TABLE_START.len();
    let end = page
        .find(TABLE_END)
        .expect("docs/presets.md has the table's end line");
    let written = page[start..end].trim();
    let table = preset_table();
    if std::env::var_os("BLESS_STYLES").is_some() {
        let blessed = format!("{}\n{}\n{}", &page[..start], table, &page[end..]);
        std::fs::write(&path, blessed).unwrap();
        return;
    }
    assert_eq!(
        written,
        table.trim(),
        "docs/presets.md is out of date: run `BLESS_STYLES=1 cargo test`"
    );
}
