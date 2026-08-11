// End-to-end guarantees for the block generator: a real plan in, a grid and
// a blueprint string out, and both survive the round trip a player actually
// exercises (paste into the game, or re-import for another pass).
use factorio_blueprint::{factorio_major_version, BlueprintData};
use factorio_solver::layout::{generate, validate, BLUEPRINT_VERSION};
use factorio_solver::testsupport::{default_cfg, green_circuit_plan};

#[test]
fn generated_blueprint_round_trips() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    // NOTE: to_blueprint returns a `Blueprint`, but encode takes `&BlueprintData`.
    // It must be wrapped — a bare Blueprint will not compile.
    let data = BlueprintData {
        blueprint: Some(factorio_grid::to_blueprint(&grid, Some("test".into()), BLUEPRINT_VERSION)),
        blueprint_book: None,
    };
    let s = factorio_blueprint::encode(&data).unwrap();
    let back = factorio_blueprint::decode(&s).unwrap();
    let regrid = factorio_grid::from_blueprint(back.blueprint.as_ref().unwrap());
    assert!(regrid.skipped.is_empty(), "no entity may be dropped on round-trip");
    assert_eq!(regrid.grid.entity_count(), grid.entity_count());

    // Equal counts alone would pass a blueprint whose directions or recipes
    // got scrambled in transit — a real risk given `from_blueprint` decides
    // whether to upgrade legacy directions from the stamped version. Compare
    // the actual multiset of (prototype, position, direction, recipe): order
    // is not guaranteed to survive re-import, so sort before comparing.
    let mut before: Vec<(String, i32, i32, u8, Option<String>)> = grid
        .entities()
        .map(|e| (e.prototype_name.to_string(), e.top_left.x, e.top_left.y, e.direction.as_u8(), e.recipe.clone()))
        .collect();
    let mut after: Vec<(String, i32, i32, u8, Option<String>)> = regrid
        .grid
        .entities()
        .map(|e| (e.prototype_name.to_string(), e.top_left.x, e.top_left.y, e.direction.as_u8(), e.recipe.clone()))
        .collect();
    before.sort();
    after.sort();
    assert_eq!(before, after, "position, direction and recipe must all survive the round trip");
}

#[test]
fn validate_accepts_the_green_circuit_block() {
    let plan = green_circuit_plan();
    let cfg = default_cfg();
    let grid = generate(&plan, &cfg).unwrap();

    assert!(factorio_solver::layout::coverage_gaps(&grid).is_empty());

    let v = validate(&grid, &plan, &cfg).unwrap();
    assert!(v.warnings.is_empty(), "the design's own worked example should not warn: {:?}", v.warnings);
}

#[test]
fn emitted_blueprint_string_starts_with_the_version_byte_and_decodes() {
    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    let data = BlueprintData {
        blueprint: Some(factorio_grid::to_blueprint(&grid, Some("green circuits".into()), BLUEPRINT_VERSION)),
        blueprint_book: None,
    };
    let s = factorio_blueprint::encode(&data).unwrap();
    assert!(s.starts_with('0'), "blueprint strings are version-byte prefixed: {}", &s[..1.min(s.len())]);

    let back = factorio_blueprint::decode(&s).unwrap();
    let blueprint = back.blueprint.expect("a blueprint (not a book) went in");
    assert_eq!(blueprint.entities.len(), grid.entity_count());
}

/// The version stamped into every generated blueprint must read back as
/// Factorio 2.0: a lower major would make `from_blueprint` treat our own
/// 2.0-encoded directions as 1.x cardinals and rewrite them on import.
#[test]
fn blueprint_version_reads_back_as_factorio_2() {
    assert_eq!(factorio_major_version(BLUEPRINT_VERSION), 2);

    let grid = generate(&green_circuit_plan(), &default_cfg()).unwrap();
    let blueprint = factorio_grid::to_blueprint(&grid, None, BLUEPRINT_VERSION);
    assert_eq!(blueprint.version, BLUEPRINT_VERSION);
    assert_eq!(factorio_major_version(blueprint.version), 2);
}
