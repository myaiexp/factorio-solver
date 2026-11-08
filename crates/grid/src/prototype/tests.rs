// Tests for the prototype registry: lookup, rotation sizes, and the dump-derived fields.
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
    assert_eq!(p.energy_source, None);
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

/// The energy source is read from the dump, so the burner/electric split
/// that `EntityCategory` cannot express (a stone furnace and an electric
/// furnace are both `Furnace`) is visible per prototype. Belts and poles
/// have no energy source at all.
#[test]
fn energy_source_separates_burner_from_electric_machines() {
    for (name, want) in [
        ("stone-furnace", Some(EnergySource::Burner)),
        ("steel-furnace", Some(EnergySource::Burner)),
        ("burner-inserter", Some(EnergySource::Burner)),
        ("electric-furnace", Some(EnergySource::Electric)),
        ("assembling-machine-2", Some(EnergySource::Electric)),
        ("inserter", Some(EnergySource::Electric)),
        ("heat-exchanger", Some(EnergySource::Heat)),
        ("offshore-pump", Some(EnergySource::Void)),
        ("transport-belt", None),
        ("medium-electric-pole", None),
    ] {
        assert_eq!(lookup(name).unwrap().energy_source, want, "{name}");
    }
    assert!(lookup("stone-furnace").unwrap().burns_fuel());
    assert!(!lookup("electric-furnace").unwrap().burns_fuel());
    assert!(lookup("electric-furnace").unwrap().on_electric_network());
    assert!(!lookup("transport-belt").unwrap().on_electric_network());
}
