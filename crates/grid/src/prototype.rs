use std::collections::HashMap;
use std::sync::OnceLock;

use factorio_blueprint::Direction;

// ── Fluid connection types ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FluidConnectionType {
    Input,
    Output,
    InputOutput,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FluidConnection {
    pub dx: f64,
    pub dy: f64,
    pub connection_type: FluidConnectionType,
}

// ── Entity prototype ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
pub struct EntityPrototype {
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    #[serde(default)]
    pub crafting_speed: Option<f64>,
    #[serde(default)]
    pub power_kw: Option<f64>,
    #[serde(default)]
    pub module_slots: u8,
    #[serde(default)]
    pub fluid_connections: Vec<FluidConnection>,
    #[serde(default)]
    pub belt_throughput: Option<f64>,
}

/// Effective (width, height) after rotation. Non-square entities swap
/// dimensions on East/West orientations.
pub fn effective_size(proto: &EntityPrototype, direction: Direction) -> (u32, u32) {
    match direction {
        Direction::East | Direction::West => (proto.tile_height, proto.tile_width),
        _ => (proto.tile_width, proto.tile_height),
    }
}

// ── OnceLock registry ─────────────────────────────────────────────────

static REGISTRY: OnceLock<HashMap<String, EntityPrototype>> = OnceLock::new();

fn load_registry() -> HashMap<String, EntityPrototype> {
    let json = include_str!("../data/prototypes.json");
    let prototypes: Vec<EntityPrototype> = serde_json::from_str(json)
        .expect("prototypes.json must be valid JSON matching EntityPrototype schema");
    prototypes.into_iter().map(|p| (p.name.clone(), p)).collect()
}

/// Lookup prototype by entity name. Returns None for unknown/modded entities.
pub fn lookup(name: &str) -> Option<&'static EntityPrototype> {
    REGISTRY.get_or_init(load_registry).get(name)
}

/// All registered prototype names.
pub fn all_names() -> Vec<&'static str> {
    REGISTRY.get_or_init(load_registry).keys().map(|s| s.as_str()).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_known_entity() {
        let proto = lookup("transport-belt").expect("should find transport-belt");
        assert_eq!(proto.tile_width, 1);
        assert_eq!(proto.tile_height, 1);
    }

    #[test]
    fn test_lookup_unknown_entity() {
        assert!(lookup("modded-thing").is_none());
    }

    #[test]
    fn test_effective_size_square() {
        let proto = lookup("assembling-machine-2").unwrap();
        assert_eq!(effective_size(proto, Direction::North), (3, 3));
        assert_eq!(effective_size(proto, Direction::East), (3, 3));
        assert_eq!(effective_size(proto, Direction::South), (3, 3));
        assert_eq!(effective_size(proto, Direction::West), (3, 3));
    }

    #[test]
    fn test_effective_size_splitter_rotation() {
        let proto = lookup("splitter").unwrap();
        assert_eq!(effective_size(proto, Direction::North), (2, 1));
        assert_eq!(effective_size(proto, Direction::East), (1, 2));
        assert_eq!(effective_size(proto, Direction::South), (2, 1));
        assert_eq!(effective_size(proto, Direction::West), (1, 2));
    }

    #[test]
    fn test_effective_size_combinator_rotation() {
        let proto = lookup("arithmetic-combinator").unwrap();
        assert_eq!(effective_size(proto, Direction::North), (1, 2));
        assert_eq!(effective_size(proto, Direction::East), (2, 1));
    }

    #[test]
    fn test_all_names_count() {
        let names = all_names();
        assert!(names.len() >= 79, "expected >= 79 prototypes, got {}", names.len());
    }

    #[test]
    fn test_all_prototypes_valid() {
        let registry = REGISTRY.get_or_init(load_registry);
        for proto in registry.values() {
            assert!(proto.tile_width >= 1, "{} has invalid width", proto.name);
            assert!(proto.tile_height >= 1, "{} has invalid height", proto.name);
        }
    }

    #[test]
    fn test_registry_loads_all_prototypes() {
        let count = all_names().len();
        assert!(count >= 79, "expected at least 79 prototypes, got {count}");
    }

    #[test]
    fn test_enriched_fields_assembler() {
        let proto = lookup("assembling-machine-2").unwrap();
        assert_eq!(proto.crafting_speed, Some(0.75));
        assert_eq!(proto.power_kw, Some(150.0));
        assert_eq!(proto.module_slots, 2);
    }

    #[test]
    fn test_belt_throughput() {
        let tb = lookup("transport-belt").unwrap();
        assert_eq!(tb.belt_throughput, Some(15.0));
        let ftb = lookup("fast-transport-belt").unwrap();
        assert_eq!(ftb.belt_throughput, Some(30.0));
        let ins = lookup("inserter").unwrap();
        assert_eq!(ins.belt_throughput, None);
    }

    #[test]
    fn test_fluid_connections_chemical_plant() {
        let proto = lookup("chemical-plant").unwrap();
        assert_eq!(proto.fluid_connections.len(), 4);
    }

    #[test]
    fn test_no_fluid_connections_belt() {
        let proto = lookup("transport-belt").unwrap();
        assert!(proto.fluid_connections.is_empty());
    }
}
