// CLI entry point: reads a Factorio data dump + locale directory, regenerates prototypes.json.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use factorio_grid::prototype::EntityPrototype;
use factorio_solver::recipe::Recipe;
use factorio_solver::tech::Technology;
use serde_json::Value;

mod dump;
mod energy;
mod entities;
mod error;
mod fluid;
mod item_amount;
mod locale;
mod mapping;
mod recipes;
mod technologies;

use error::IngestError;
use locale::Locale;

/// Regenerates `crates/grid/data/prototypes.json`, `crates/solver/data/
/// recipes.json`, and `crates/solver/data/technologies.json` from a Factorio
/// `--dump-data` dump (mod-free `factorio --dump-data`). Run manually when
/// the game updates — never as a build step, since the workspace build must
/// not require a Factorio install.
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
    /// Where to write the regenerated technologies JSON array.
    #[arg(long)]
    out_technologies: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok((proto_count, recipe_count, tech_count)) => {
            eprintln!("wrote {proto_count} prototypes to {}", args.out_prototypes.display());
            eprintln!("wrote {recipe_count} recipes to {}", args.out_recipes.display());
            eprintln!("wrote {tech_count} technologies to {}", args.out_technologies.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(usize, usize, usize), IngestError> {
    let dump = read_dump(&args.dump)?;

    let entity_locale = locale::load_locale(&args.locale_dir, "entity")?;
    let prototypes = build_prototypes(&dump, &entity_locale)?;
    write_prototypes(&args.out_prototypes, &prototypes)?;

    let recipe_locale = locale::load_locale(&args.locale_dir, "recipe")?;
    let recipes = build_recipes(&dump, &recipe_locale)?;
    write_recipes(&args.out_recipes, &recipes)?;

    // The technology unlock filter needs the recipe set the ingest actually
    // kept (parameter placeholders already dropped by `build_recipes`), so
    // it is derived from `recipes` rather than re-reading the raw dump — the
    // recipe filter stays the single source of truth for what counts as real.
    let known_recipes: HashSet<String> = recipes.iter().map(|r| r.name.clone()).collect();
    let technology_locale = locale::load_locale(&args.locale_dir, "technology")?;
    let technologies = build_technologies(&dump, &technology_locale, &known_recipes)?;
    write_technologies(&args.out_technologies, &technologies)?;

    Ok((prototypes.len(), recipes.len(), technologies.len()))
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

/// Maps every technology under the dump's top-level `"technology"` key,
/// sorted by name for a reviewable diff, then validates the whole
/// prerequisite graph in a second pass — a check that needs every
/// technology gathered first, so it cannot live inside `to_technology`.
fn build_technologies(
    dump: &Value,
    locale: &Locale,
    known_recipes: &HashSet<String>,
) -> Result<Vec<Technology>, IngestError> {
    let mut technologies = Vec::new();
    if let Some(entries) = dump.get("technology").and_then(Value::as_object) {
        for (name, raw) in entries {
            technologies.push(technologies::to_technology(name, raw, locale, known_recipes)?);
        }
    }
    technologies.sort_by(|a, b| a.name.cmp(&b.name));
    technologies::validate_prerequisites(&technologies)?;
    Ok(technologies)
}

fn write_technologies(path: &Path, technologies: &[Technology]) -> Result<(), IngestError> {
    let mut json = serde_json::to_string_pretty(technologies).map_err(IngestError::Serialize)?;
    json.push('\n');
    std::fs::write(path, json)
        .map_err(|source| IngestError::WriteOutput { path: path.display().to_string(), source })
}

#[cfg(test)]
mod tests;
