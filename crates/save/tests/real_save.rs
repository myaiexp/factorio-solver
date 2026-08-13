// Opt-in ground-truth test against a real Factorio save.
//
// There is no Factorio install on this machine, so this test is skipped
// unless FACTORIO_SAVE_FIXTURE names a real save file on disk. It is the
// only test in this crate that can ever validate the parser against real
// game data — every other test here exercises FixtureSave, a byte-for-byte
// synthetic construction of the format this crate assumes.
use std::collections::HashSet;
use std::path::Path;

use factorio_save::SaveFile;

/// Recipes the game enables without any research, in vanilla Factorio 2.0.
/// Hardcoded here rather than imported from `factorio-solver`'s recipe dump:
/// this crate must stay a leaf (`solver -> save`, never the reverse), and
/// pulling `default_enabled` from `solver` here would invert that for the
/// sake of one opt-in test. Kept deliberately small and high-confidence —
/// each of these is craftable at the start of a fresh game with no
/// research — since a wrong entry here fails calibration for every save,
/// not just an edge case.
fn default_enabled() -> HashSet<String> {
    [
        "iron-plate",
        "copper-plate",
        "iron-gear-wheel",
        "copper-cable",
        "transport-belt",
        "stone-furnace",
        "electronic-circuit",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[test]
fn real_save_ground_truth() {
    let Ok(path) = std::env::var("FACTORIO_SAVE_FIXTURE") else { return };

    let mut save =
        SaveFile::open(Path::new(&path)).unwrap_or_else(|e| panic!("failed to open {path}: {e}"));

    let version = save.version();
    assert!(
        version.major >= 1,
        "implausible version {}.{}.{} parsed from {path} — parse_header's field-layout \
         guess (see init.rs's doc comment) is likely wrong for this save",
        version.major,
        version.minor,
        version.patch
    );

    // The ONLY assertion anywhere in this crate that checks level-init's
    // *header* layout (the mod list) rather than the independently
    // byte-searched category tables: init.rs documents parse_header and
    // parse_mods as inferred, never confirmed against a real save (no
    // Factorio install exists on the machine that wrote them). If this
    // assertion fails, the header-layout guess is what's wrong — not
    // necessarily anything else in this file, since recipes()/technologies()
    // do not depend on the header parse landing correctly.
    let mods = save.mods();
    assert!(
        mods.iter().any(|m| m == "base"),
        "expected 'base' among the save's mods, got {mods:?} (version {version:?}) — \
         the mod-list layout guess in init::parse_mods is likely wrong"
    );

    let recipe_count = save.recipes().len();
    assert!(
        recipe_count > 100,
        "recipe table has only {recipe_count} entries for version {version:?} — implausibly \
         small for a vanilla-or-larger save; the 'recipe' category-table search in init.rs \
         may have matched the wrong location"
    );

    let defaults = default_enabled();
    let unlocked = save.unlocked_recipes(&defaults).unwrap_or_else(|e| {
        panic!(
            "calibration failed against {path} (version {version:?}, {recipe_count} recipes \
             in the table): {e}\n\
             If this is a version/mod mismatch, regenerate crates/solver/data/recipes.json \
             (which is unrelated to this crate but is the usual root cause of a version drift) \
             or re-check the default_enabled set above against this save's actual game version."
        )
    });

    let cal = save.calibration().expect("calibration() must be Some after a successful decode");
    eprintln!(
        "real_save_ground_truth: path={path} version={version:?} recipe_table_size={recipe_count} \
         calibration={cal:?} unlocked_count={}",
        unlocked.len()
    );

    // Ground truth measured against 2026.zip (2.0.77, 369/659 unlocked) —
    // see the design doc at .claude/plans/2026-08-13-save-derived-availability-design.md.
    assert!(
        unlocked.contains("assembling-machine-2"),
        "expected assembling-machine-2 UNLOCKED on {path} (version {version:?}, \
         calibration {cal:?}, {}/{recipe_count} recipes unlocked) — got locked instead",
        unlocked.len()
    );
    assert!(
        !unlocked.contains("assembling-machine-3"),
        "expected assembling-machine-3 LOCKED on {path} (version {version:?}, \
         calibration {cal:?}, {}/{recipe_count} recipes unlocked) — got unlocked instead",
        unlocked.len()
    );
    assert!(
        !unlocked.contains("oil-refinery"),
        "expected oil-refinery LOCKED on {path} (version {version:?}, calibration {cal:?}, \
         {}/{recipe_count} recipes unlocked) — got unlocked instead",
        unlocked.len()
    );
}
