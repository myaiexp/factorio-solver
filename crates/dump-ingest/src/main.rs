// CLI entry point: reads a Factorio data dump + locale directory, regenerates prototypes.json.
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use factorio_grid::prototype::EntityPrototype;
use factorio_solver::recipe::Recipe;
use serde_json::Value;

mod dump;
mod entities;
mod error;
mod fluid;
mod item_amount;
mod locale;
mod mapping;
mod recipes;

use error::IngestError;
use locale::Locale;

/// Regenerates `crates/grid/data/prototypes.json` and `crates/solver/data/
/// recipes.json` from a Factorio `--dump-data` dump (mod-free
/// `factorio --dump-data`). Run manually when the game updates — never as a
/// build step, since the workspace build must not require a Factorio install.
#[derive(Parser)]
#[command(name = "dump-ingest")]
struct Args {
    /// Path to data-raw-dump.json.
    #[arg(long)]
    dump: PathBuf,
    /// Directory containing the locale JSON files (entity-locale.json, ...).
    #[arg(long)]
    locale_dir: PathBuf,
    /// Where to write the regenerated prototypes JSON array.
    #[arg(long)]
    out_prototypes: PathBuf,
    /// Where to write the regenerated recipes JSON array.
    #[arg(long)]
    out_recipes: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok((proto_count, recipe_count)) => {
            eprintln!("wrote {proto_count} prototypes to {}", args.out_prototypes.display());
            eprintln!("wrote {recipe_count} recipes to {}", args.out_recipes.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(usize, usize), IngestError> {
    let dump = read_dump(&args.dump)?;

    let entity_locale = locale::load_locale(&args.locale_dir, "entity")?;
    let prototypes = build_prototypes(&dump, &entity_locale)?;
    write_prototypes(&args.out_prototypes, &prototypes)?;

    let recipe_locale = locale::load_locale(&args.locale_dir, "recipe")?;
    let recipes = build_recipes(&dump, &recipe_locale)?;
    write_recipes(&args.out_recipes, &recipes)?;

    Ok((prototypes.len(), recipes.len()))
}

fn read_dump(path: &Path) -> Result<Value, IngestError> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| IngestError::ReadDump { path: path.display().to_string(), source })?;
    serde_json::from_str(&text)
        .map_err(|source| IngestError::ParseDump { path: path.display().to_string(), source })
}

/// Filters + maps every placeable entity, sorted by name for a reviewable diff.
fn build_prototypes(dump: &Value, locale: &Locale) -> Result<Vec<EntityPrototype>, IngestError> {
    let mut prototypes = Vec::new();
    for entity in entities::placeable_entities(dump) {
        prototypes.push(mapping::to_prototype(entity.prototype_type, entity.name, entity.value, locale)?);
    }
    prototypes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(prototypes)
}

fn write_prototypes(path: &Path, prototypes: &[EntityPrototype]) -> Result<(), IngestError> {
    let mut json = serde_json::to_string_pretty(prototypes).map_err(IngestError::Serialize)?;
    json.push('\n');
    std::fs::write(path, json)
        .map_err(|source| IngestError::WriteOutput { path: path.display().to_string(), source })
}

/// Maps every recipe under the dump's top-level `"recipe"` key, dropping the
/// `parameter: true` placeholders (`to_recipe` returns `None` for those),
/// sorted by name for a reviewable diff.
fn build_recipes(dump: &Value, locale: &Locale) -> Result<Vec<Recipe>, IngestError> {
    let mut recipes = Vec::new();
    if let Some(entries) = dump.get("recipe").and_then(Value::as_object) {
        for (name, raw) in entries {
            if let Some(recipe) = recipes::to_recipe(name, raw, locale)? {
                recipes.push(recipe);
            }
        }
    }
    recipes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(recipes)
}

fn write_recipes(path: &Path, recipes: &[Recipe]) -> Result<(), IngestError> {
    let mut json = serde_json::to_string_pretty(recipes).map_err(IngestError::Serialize)?;
    json.push('\n');
    std::fs::write(path, json)
        .map_err(|source| IngestError::WriteOutput { path: path.display().to_string(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI_DUMP: &str = include_str!("../tests/fixtures/mini-dump.json");

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn end_to_end_mini_dump_produces_expected_prototypes() {
        let dump: Value = serde_json::from_str(MINI_DUMP).unwrap();
        let locale = locale::load_locale(&fixtures_dir(), "entity").unwrap();
        let prototypes = build_prototypes(&dump, &locale).unwrap();

        let mut names: Vec<&str> = prototypes.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["chemical-plant", "inserter", "logistic-robot", "transport-belt"],
            "character (no player-creation flag) and iron-ore (null selection_box) must be excluded"
        );

        let belt = prototypes.iter().find(|p| p.name == "transport-belt").unwrap();
        assert_eq!((belt.tile_width, belt.tile_height), (1, 1));
        assert_eq!(belt.belt_throughput, Some(15.0));
        assert_eq!(belt.display_name.as_deref(), Some("Transport belt"));
        assert_eq!(belt.icon_path.as_deref(), Some("__base__/graphics/icons/transport-belt.png"));

        let robot = prototypes.iter().find(|p| p.name == "logistic-robot").unwrap();
        assert_eq!(robot.belt_throughput, None, "robots have .speed but aren't belts");

        let plant = prototypes.iter().find(|p| p.name == "chemical-plant").unwrap();
        assert_eq!(plant.fluid_connections.len(), 2);
        assert_eq!(plant.tile_width, 3);
        assert_eq!(plant.module_slots, 4);

        let inserter = prototypes.iter().find(|p| p.name == "inserter").unwrap();
        assert_eq!(inserter.pickup_position, Some((0.0, -1.0)));
        assert_eq!(inserter.insert_position, Some((0.0, 1.2)));
    }

    #[test]
    fn output_is_sorted_by_name() {
        let dump: Value = serde_json::from_str(MINI_DUMP).unwrap();
        let locale = locale::load_locale(&fixtures_dir(), "entity").unwrap();
        let prototypes = build_prototypes(&dump, &locale).unwrap();
        let names: Vec<&str> = prototypes.iter().map(|p| p.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn cli_args_parse_expected_flags() {
        let args = Args::parse_from([
            "dump-ingest",
            "--dump",
            "d.json",
            "--locale-dir",
            "loc",
            "--out-prototypes",
            "out.json",
            "--out-recipes",
            "recipes.json",
        ]);
        assert_eq!(args.dump, PathBuf::from("d.json"));
        assert_eq!(args.locale_dir, PathBuf::from("loc"));
        assert_eq!(args.out_prototypes, PathBuf::from("out.json"));
        assert_eq!(args.out_recipes, PathBuf::from("recipes.json"));
    }

    #[test]
    fn missing_dump_file_is_a_read_error() {
        let err = read_dump(Path::new("/nonexistent/does-not-exist.json")).unwrap_err();
        assert!(matches!(err, IngestError::ReadDump { .. }));
    }

    #[test]
    fn malformed_dump_json_is_a_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad-dump.json");
        std::fs::write(&path, "{ not json").unwrap();

        let err = read_dump(&path).unwrap_err();
        assert!(matches!(err, IngestError::ParseDump { .. }));
    }

    #[test]
    fn written_output_round_trips_back_into_prototypes() {
        // The tool serializes the real EntityPrototype so the emitted file
        // cannot drift from grid's consuming serde definition — assert that
        // directly rather than trusting the derive.
        let dump: Value = serde_json::from_str(MINI_DUMP).unwrap();
        let locale = locale::load_locale(&fixtures_dir(), "entity").unwrap();
        let prototypes = build_prototypes(&dump, &locale).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prototypes.json");
        write_prototypes(&path, &prototypes).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'), "output must end with a newline");
        assert!(!text.contains("null"), "absent optional fields must be omitted, not null");
        let back: Vec<EntityPrototype> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, prototypes);
    }

    #[test]
    fn end_to_end_mini_dump_produces_expected_recipes() {
        let dump: Value = serde_json::from_str(MINI_DUMP).unwrap();
        let locale = locale::load_locale(&fixtures_dir(), "recipe").unwrap();
        let recipes = build_recipes(&dump, &locale).unwrap();

        // Already-sorted order doubles as the sort-by-name assertion.
        let names: Vec<&str> = recipes.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["biter-egg", "electronic-circuit", "iron-plate", "scrap-recycling", "sulfur"],
            "parameter-0 (parameter: true) must be filtered out; output must be sorted by name"
        );

        let iron_plate = recipes.iter().find(|r| r.name == "iron-plate").unwrap();
        assert!(iron_plate.enabled, "absent enabled must default to true");
        assert_eq!(iron_plate.category, "crafting", "absent category must default to crafting");
        assert_eq!(iron_plate.energy_required, 0.5, "absent energy_required must default to 0.5");

        let circuit = recipes.iter().find(|r| r.name == "electronic-circuit").unwrap();
        assert!(!circuit.enabled, "explicit enabled: false must be preserved");
        assert_eq!(circuit.category, "electronics");

        let egg = recipes.iter().find(|r| r.name == "biter-egg").unwrap();
        assert!(egg.ingredients.is_empty(), "ingredients: {{}} must become an empty vec");
        assert_eq!(egg.results.len(), 1);
        assert_eq!(egg.results[0].amount, 5.0);
        assert_eq!(egg.energy_required, 10.0);

        let recycling = recipes.iter().find(|r| r.name == "scrap-recycling").unwrap();
        assert!(recycling.hidden, "hidden recipes must be kept, with the flag preserved");

        let sulfur = recipes.iter().find(|r| r.name == "sulfur").unwrap();
        assert!(
            sulfur.ingredients.iter().any(|i| i.kind == factorio_solver::recipe::ItemKind::Fluid),
            "sulfur's water ingredient must keep its fluid kind"
        );

        for recipe in &recipes {
            assert!(recipe.display_name.is_some(), "{} must have a display_name", recipe.name);
        }
    }

    #[test]
    fn written_recipes_round_trip_back_into_recipes() {
        // Same rationale as written_output_round_trips_back_into_prototypes:
        // serialize the real Recipe type so the emitted file cannot drift from
        // solver's consuming serde definition.
        let dump: Value = serde_json::from_str(MINI_DUMP).unwrap();
        let locale = locale::load_locale(&fixtures_dir(), "recipe").unwrap();
        let recipes = build_recipes(&dump, &locale).unwrap();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recipes.json");
        write_recipes(&path, &recipes).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'), "output must end with a newline");
        let back: Vec<Recipe> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, recipes);
    }
}
