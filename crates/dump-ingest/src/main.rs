// CLI entry point: reads a Factorio data dump + locale directory, regenerates prototypes.json.
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use factorio_grid::prototype::EntityPrototype;
use serde_json::Value;

mod dump;
mod entities;
mod error;
mod fluid;
mod locale;
mod mapping;

use error::IngestError;
use locale::Locale;

/// Regenerates `crates/grid/data/prototypes.json` from a Factorio `--dump-data`
/// dump (mod-free `factorio --dump-data`). Run manually when the game updates —
/// never as a build step, since the workspace build must not require a
/// Factorio install.
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
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(count) => {
            eprintln!("wrote {count} prototypes to {}", args.out_prototypes.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<usize, IngestError> {
    let dump = read_dump(&args.dump)?;
    let locale = locale::load_locale(&args.locale_dir, "entity")?;
    let prototypes = build_prototypes(&dump, &locale)?;
    write_prototypes(&args.out_prototypes, &prototypes)?;
    Ok(prototypes.len())
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
        let args = Args::parse_from(["dump-ingest", "--dump", "d.json", "--locale-dir", "loc", "--out-prototypes", "out.json"]);
        assert_eq!(args.dump, PathBuf::from("d.json"));
        assert_eq!(args.locale_dir, PathBuf::from("loc"));
        assert_eq!(args.out_prototypes, PathBuf::from("out.json"));
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
}
