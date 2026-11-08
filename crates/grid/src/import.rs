use factorio_blueprint::{directions_look_legacy, Blueprint};

use crate::error::GridError;
use crate::grid::Grid;

// ── Import result types ─────────────────────────────────────────────

/// Result of importing a blueprint into a grid.
#[derive(Debug)]
pub struct ImportResult {
    /// The populated grid with all recognized entities placed.
    pub grid: Grid,
    /// Entities that could not be placed (unknown prototype, etc.).
    pub skipped: Vec<SkippedEntity>,
}

/// An entity from the blueprint that was skipped during import.
#[derive(Debug)]
pub struct SkippedEntity {
    /// The entity_number from the blueprint.
    pub entity_number: u32,
    /// The entity name (prototype name).
    pub name: String,
    /// Why the entity was skipped.
    pub reason: String,
}

// ── Import function ─────────────────────────────────────────────────

/// Build a Grid from a decoded Blueprint.
///
/// Iterates all entities in the blueprint, looks up each prototype via
/// `crate::prototype::lookup()`, and calls `grid.place()`. Unknown
/// prototypes are gracefully skipped and collected in `ImportResult.skipped`.
///
/// Factorio 1.x cardinal directions (N/E/S/W = 0/2/4/6) are upgraded to the
/// 2.0 scheme when the blueprint's version/direction set looks legacy —
/// otherwise non-square footprints (combinators, splitters) collide and route
/// wrong. Major version `< 2` always upgrades pure cardinal sets (including
/// pure-South and North+South-only); major `≥ 2` only upgrades when a
/// definitive East/West marker is present so true 2.0 North+East is kept.
///
/// This function never panics on valid blueprints — real Factorio blueprints
/// contain non-overlapping entities, and unknown entities are simply skipped.
pub fn from_blueprint(blueprint: &Blueprint) -> ImportResult {
    let mut grid = Grid::new();
    let mut skipped = Vec::new();

    let legacy = directions_look_legacy(
        blueprint.entities.iter().map(|e| e.direction),
        blueprint.version,
    );

    for entity in &blueprint.entities {
        let direction = if legacy {
            entity.direction.upgrade_from_legacy()
        } else {
            entity.direction
        };

        match grid.place(
            &entity.name,
            &entity.position,
            direction,
            entity.recipe.clone(),
            entity.entity_type.clone(),
        ) {
            Ok(id) => {
                // Names only — the grid models no quality — and a quality-only
                // filter slot therefore contributes nothing rather than an
                // empty name. `use_filters` is not consulted: a blueprint that
                // carries filter names with the flag off still describes those
                // slots, and export writes the flag back on from scratch.
                let names: Vec<String> = entity
                    .filters
                    .iter()
                    .flatten()
                    .filter_map(|f| f.name.clone())
                    .collect();
                if !names.is_empty() {
                    grid.set_filters(id, names)
                        .expect("the id was just returned by place");
                }
            }
            Err(GridError::UnknownPrototype(name)) => {
                skipped.push(SkippedEntity {
                    entity_number: entity.entity_number,
                    name: entity.name.clone(),
                    reason: format!("unknown prototype: {name}"),
                });
            }
            Err(e) => {
                skipped.push(SkippedEntity {
                    entity_number: entity.entity_number,
                    name: entity.name.clone(),
                    reason: format!("{e}"),
                });
            }
        }
    }

    ImportResult { grid, skipped }
}

#[cfg(test)]
mod tests;
