use factorio_blueprint::Direction;

// ── Entity prototype ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EntityPrototype {
    pub name: &'static str,
    pub tile_width: u32,
    pub tile_height: u32,
}

/// Effective (width, height) after rotation. Non-square entities swap
/// dimensions on East/West orientations.
pub fn effective_size(proto: &EntityPrototype, direction: Direction) -> (u32, u32) {
    match direction {
        Direction::East | Direction::West => (proto.tile_height, proto.tile_width),
        _ => (proto.tile_width, proto.tile_height),
    }
}

/// Lookup prototype by entity name. Returns None for unknown/modded entities.
pub fn lookup(name: &str) -> Option<&'static EntityPrototype> {
    PROTOTYPES.iter().find(|p| p.name == name)
}

/// All registered prototype names.
pub fn all_names() -> Vec<&'static str> {
    PROTOTYPES.iter().map(|p| p.name).collect()
}

// ── Prototype data table ─────────────────────────────────────────────

static PROTOTYPES: &[EntityPrototype] = &[
    // Belts (1x1)
    EntityPrototype { name: "transport-belt", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "fast-transport-belt", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "express-transport-belt", tile_width: 1, tile_height: 1 },
    // Underground belts (1x1)
    EntityPrototype { name: "underground-belt", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "fast-underground-belt", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "express-underground-belt", tile_width: 1, tile_height: 1 },
    // Splitters (2x1 — swap on rotation)
    EntityPrototype { name: "splitter", tile_width: 2, tile_height: 1 },
    EntityPrototype { name: "fast-splitter", tile_width: 2, tile_height: 1 },
    EntityPrototype { name: "express-splitter", tile_width: 2, tile_height: 1 },
    // Inserters (1x1)
    EntityPrototype { name: "inserter", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "fast-inserter", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "long-handed-inserter", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "stack-inserter", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "bulk-inserter", tile_width: 1, tile_height: 1 },
    // Assembling machines (3x3)
    EntityPrototype { name: "assembling-machine-1", tile_width: 3, tile_height: 3 },
    EntityPrototype { name: "assembling-machine-2", tile_width: 3, tile_height: 3 },
    EntityPrototype { name: "assembling-machine-3", tile_width: 3, tile_height: 3 },
    // Furnaces
    EntityPrototype { name: "stone-furnace", tile_width: 2, tile_height: 2 },
    EntityPrototype { name: "steel-furnace", tile_width: 2, tile_height: 2 },
    EntityPrototype { name: "electric-furnace", tile_width: 3, tile_height: 3 },
    // Chemical processing
    EntityPrototype { name: "chemical-plant", tile_width: 3, tile_height: 3 },
    EntityPrototype { name: "oil-refinery", tile_width: 5, tile_height: 5 },
    // Pipes (1x1)
    EntityPrototype { name: "pipe", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "pipe-to-ground", tile_width: 1, tile_height: 1 },
    // Electric poles
    EntityPrototype { name: "small-electric-pole", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "medium-electric-pole", tile_width: 1, tile_height: 1 },
    EntityPrototype { name: "big-electric-pole", tile_width: 2, tile_height: 2 },
    EntityPrototype { name: "substation", tile_width: 2, tile_height: 2 },
    // Misc
    EntityPrototype { name: "beacon", tile_width: 3, tile_height: 3 },
    EntityPrototype { name: "small-lamp", tile_width: 1, tile_height: 1 },
    // Combinators
    EntityPrototype { name: "arithmetic-combinator", tile_width: 1, tile_height: 2 },
    EntityPrototype { name: "decider-combinator", tile_width: 1, tile_height: 2 },
    EntityPrototype { name: "constant-combinator", tile_width: 1, tile_height: 1 },
];

// ── Tests ────────────────────────────────────────────────────────────

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
        assert_eq!(names.len(), PROTOTYPES.len());
        assert!(names.len() >= 30, "expected ~30 prototypes, got {}", names.len());
    }

    #[test]
    fn test_all_prototypes_valid() {
        for proto in PROTOTYPES {
            assert!(proto.tile_width >= 1, "{} has invalid width", proto.name);
            assert!(proto.tile_height >= 1, "{} has invalid height", proto.name);
        }
    }
}
