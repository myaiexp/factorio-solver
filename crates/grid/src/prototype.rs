use std::collections::HashMap;
use std::sync::OnceLock;

use factorio_blueprint::Direction;

// ── Fluid connection types ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FluidConnectionType {
    Input,
    Output,
    InputOutput,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FluidConnection {
    pub dx: f64,
    pub dy: f64,
    pub connection_type: FluidConnectionType,
}

// ── Entity prototype ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EntityPrototype {
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crafting_speed: Option<f64>,
    /// Power *consumption*, from the dump's `energy_usage`. Generators have
    /// none — their output lives in three different fields depending on
    /// prototype type and is not derived yet. Do not read this as "power
    /// produced"; the old hand-written table did, which would make a solver
    /// summing loads count a solar panel as a 60 kW draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_kw: Option<f64>,
    #[serde(default)]
    pub module_slots: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fluid_connections: Vec<FluidConnection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub belt_throughput: Option<f64>,

    // ── Dump-derived fields ───────────────────────────────────────────
    // Every one is `default`: the registry file predates them, and a
    // hand-written fixture may omit any of them.
    /// Localised name from `entity-locale.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Mod-relative icon path, e.g. `__base__/graphics/icons/x.png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
    /// Edge length of the icon's first mip level. Absent → callers use 64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<u32>,
    /// Recipe categories this machine can craft (empty for non-crafters).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crafting_categories: Vec<String>,
    /// Underground belt / pipe span, in tiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underground_max_distance: Option<u32>,
    /// Inserter pickup offset relative to the entity centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pickup_position: Option<(f64, f64)>,
    /// Inserter drop-off offset relative to the entity centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_position: Option<(f64, f64)>,
    /// Electric pole supply-area *half-width*, in tiles: the area reaches
    /// `± this` from the pole's centre, so a medium pole's 3.5 covers 7 tiles.
    /// Not the wire reach — `maximum_wire_distance` is a different, larger
    /// number (medium pole: 3.5 supply vs 9 wire) and the two are easy to
    /// confuse into a layout that looks connected but is unpowered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply_area_distance: Option<f64>,
}

impl EntityPrototype {
    /// The rectangle this pole energises when placed with its footprint's
    /// top-left at `top_left`, as inclusive tile bounds
    /// `(min_x, min_y, max_x, max_y)`. `None` for anything that is not a pole.
    ///
    /// Factorio powers an entity whose own footprint *overlaps* this area — it
    /// does not require containment — so callers test intersection, not
    /// enclosure.
    pub fn supply_area(&self, top_left: (i32, i32)) -> Option<(f64, f64, f64, f64)> {
        let d = self.supply_area_distance?;
        let (cx, cy) = (
            top_left.0 as f64 + self.tile_width as f64 / 2.0,
            top_left.1 as f64 + self.tile_height as f64 / 2.0,
        );
        Some((cx - d, cy - d, cx + d, cy + d))
    }
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

    // Fuller pins on the dump-derived data — footprints, throughput tiers, the
    // storage-tank 1 -> 4 connection correction — live in
    // `tests/prototype_regression.rs`.
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

    #[test]
    fn existing_prototypes_json_still_loads() {
        // The committed registry predates every dump-derived field.
        let p = lookup("transport-belt").expect("transport-belt present");
        assert_eq!(p.belt_throughput, Some(15.0));
    }

    #[test]
    fn new_fields_default_when_absent() {
        let p: EntityPrototype =
            serde_json::from_str(r#"{"name":"x","tile_width":1,"tile_height":1}"#).unwrap();
        assert_eq!(p.display_name, None);
        assert_eq!(p.icon_path, None);
        assert_eq!(p.icon_size, None);
        assert!(p.crafting_categories.is_empty());
        assert_eq!(p.underground_max_distance, None);
        assert_eq!(p.pickup_position, None);
        assert_eq!(p.insert_position, None);
        assert_eq!(p.supply_area_distance, None);
    }

    #[test]
    fn poles_carry_their_supply_area_distance() {
        // The four real poles, and the reason the field cannot be guessed from
        // the pole's "size": big-electric-pole is the 2x2 long-*wire* pole with
        // the *smallest* supply area of the four.
        assert_eq!(lookup("small-electric-pole").unwrap().supply_area_distance, Some(2.5));
        assert_eq!(lookup("medium-electric-pole").unwrap().supply_area_distance, Some(3.5));
        assert_eq!(lookup("big-electric-pole").unwrap().supply_area_distance, Some(2.0));
        assert_eq!(lookup("substation").unwrap().supply_area_distance, Some(9.0));
        assert_eq!(lookup("assembling-machine-2").unwrap().supply_area_distance, None);
    }

    #[test]
    fn supply_area_is_centred_on_the_pole_footprint() {
        // 1x1 medium pole at (10, 10): centre (10.5, 10.5), ±3.5.
        let medium = lookup("medium-electric-pole").unwrap();
        assert_eq!(medium.supply_area((10, 10)), Some((7.0, 7.0, 14.0, 14.0)));

        // 2x2 substation at (0, 0): centre (1, 1), ±9 — the footprint size is
        // part of the maths, not just the distance.
        let sub = lookup("substation").unwrap();
        assert_eq!(sub.supply_area((0, 0)), Some((-8.0, -8.0, 10.0, 10.0)));

        assert_eq!(lookup("transport-belt").unwrap().supply_area((0, 0)), None);
    }

    #[test]
    fn new_fields_parse_when_present() {
        let p: EntityPrototype = serde_json::from_str(
            r#"{"name":"inserter","tile_width":1,"tile_height":1,
                "display_name":"Inserter",
                "icon_path":"__base__/graphics/icons/inserter.png",
                "icon_size":64,
                "crafting_categories":["crafting"],
                "underground_max_distance":5,
                "pickup_position":[0.0,-1.0],
                "insert_position":[0.0,1.2]}"#,
        )
        .unwrap();
        assert_eq!(p.display_name.as_deref(), Some("Inserter"));
        assert_eq!(p.icon_size, Some(64));
        assert_eq!(p.crafting_categories, vec!["crafting".to_string()]);
        assert_eq!(p.underground_max_distance, Some(5));
        assert_eq!(p.pickup_position, Some((0.0, -1.0)));
        assert_eq!(p.insert_position, Some((0.0, 1.2)));
    }
}
