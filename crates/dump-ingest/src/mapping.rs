// Maps one filtered dump entity onto the shared `EntityPrototype` schema.
use serde_json::Value;

use factorio_grid::prototype::EntityPrototype;

use crate::entities::{parse_power_kw, tile_size};
use crate::error::IngestError;
use crate::fluid::fluid_connections;
use crate::locale::Locale;

/// Prototype-type keys whose `.speed` becomes `belt_throughput`. Combat/
/// construction/logistic/capture robots also carry `.speed` but are filed under
/// other prototype-type keys and must not get a throughput — this list, not the
/// mere presence of `.speed`, is the gate.
const BELT_FAMILY: &[&str] =
    &["transport-belt", "underground-belt", "splitter", "loader", "linked-belt", "lane-splitter"];

fn icon_path(entity: &Value) -> Option<String> {
    entity
        .get("icon")
        .and_then(Value::as_str)
        .or_else(|| {
            entity
                .get("icons")
                .and_then(Value::as_array)
                .and_then(|icons| icons.first())
                .and_then(|first| first.get("icon"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

fn xy_pair(entity: &Value, key: &str) -> Option<(f64, f64)> {
    let arr = entity.get(key).and_then(Value::as_array)?;
    Some((arr.first().and_then(Value::as_f64)?, arr.get(1).and_then(Value::as_f64)?))
}

/// Absent `energy_usage` means the entity draws no power and stays `None`. A
/// value that is *present* but unrecognised is an error, not a silent `None`:
/// the whole point of this tool is that a partial write must never pass for a
/// complete one.
fn power_kw(name: &str, entity: &Value) -> Result<Option<f64>, IngestError> {
    let Some(raw) = entity.get("energy_usage").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let text = raw.as_str().ok_or_else(|| IngestError::UnparseablePower {
        name: name.to_string(),
        value: raw.to_string(),
    })?;
    parse_power_kw(text).map(Some).ok_or_else(|| IngestError::UnparseablePower {
        name: name.to_string(),
        value: text.to_string(),
    })
}

/// Pole supply area, gated on the prototype type rather than on the key being
/// present — `beacon` declares `supply_area_distance` too, where it means the
/// module *effect* radius and nothing to do with power. Ungated, a layout would
/// read a beacon as a 6x6 power pole and emit an unpowered block. Same trap as
/// `belt_throughput` vs. robot `.speed` above.
fn supply_area_distance(prototype_type: &str, entity: &Value) -> Option<f64> {
    (prototype_type == "electric-pole")
        .then(|| entity.get("supply_area_distance").and_then(Value::as_f64))
        .flatten()
}

fn require(name: &str, field: &str, value: Option<String>) -> Result<String, IngestError> {
    value.ok_or_else(|| IngestError::MissingField { name: name.to_string(), field: field.to_string() })
}

/// Maps one dump entity onto the shared `EntityPrototype` schema.
///
/// `collision_box`, `display_name` (from `entity-locale.json`), and the icon
/// path (`icon` or `icons[0].icon`) are required: every one of the 169 real
/// placeable entities has all three, so a future entity missing one fails
/// loudly by name instead of silently writing an incomplete prototype. Every
/// other field is genuinely optional in the dump (crafting machines vs. belts
/// vs. inserters expose disjoint field sets) and defaults per the field table
/// in the ingest plan.
pub fn to_prototype(
    prototype_type: &str,
    name: &str,
    entity: &Value,
    locale: &Locale,
) -> Result<EntityPrototype, IngestError> {
    if !entity.get("collision_box").is_some_and(|v| !v.is_null()) {
        return Err(IngestError::MissingField { name: name.to_string(), field: "collision_box".to_string() });
    }
    let (tile_width, tile_height) = tile_size(entity);

    let display_name = require(name, "display_name (entity-locale.json names)", locale.name(name).map(str::to_string))?;
    let icon_path = require(name, "icon path (icon or icons[0].icon)", icon_path(entity))?;

    let belt_throughput = if BELT_FAMILY.contains(&prototype_type) {
        entity.get("speed").and_then(Value::as_f64).map(|speed| speed * 480.0)
    } else {
        None
    };

    Ok(EntityPrototype {
        name: name.to_string(),
        tile_width,
        tile_height,
        crafting_speed: entity.get("crafting_speed").and_then(Value::as_f64),
        power_kw: power_kw(name, entity)?,
        module_slots: entity.get("module_slots").and_then(Value::as_u64).unwrap_or(0) as u8,
        fluid_connections: fluid_connections(entity),
        belt_throughput,
        display_name: Some(display_name),
        icon_path: Some(icon_path),
        icon_size: entity.get("icon_size").and_then(Value::as_u64).map(|v| v as u32),
        crafting_categories: entity
            .get("crafting_categories")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        underground_max_distance: entity.get("max_distance").and_then(Value::as_u64).map(|v| v as u32),
        pickup_position: xy_pair(entity, "pickup_position"),
        insert_position: xy_pair(entity, "insert_position"),
        supply_area_distance: supply_area_distance(prototype_type, entity),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn locale_with(name: &str, display: &str) -> Locale {
        Locale::from_names([(name, display)])
    }

    #[test]
    fn belt_throughput_derives_from_speed() {
        let locale = locale_with("transport-belt", "Transport belt");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.5,-0.5],[0.5,0.5]],
            "collision_box": [[-0.4,-0.4],[0.4,0.4]], "speed": 0.03125,
            "icon": "__base__/graphics/icons/transport-belt.png"
        });
        let proto = to_prototype("transport-belt", "transport-belt", &e, &locale).unwrap();
        assert_eq!(proto.belt_throughput, Some(15.0));
    }

    #[test]
    fn belt_throughput_absent_for_robots_despite_speed() {
        let locale = locale_with("logistic-robot", "Logistic robot");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.4,-0.4],[0.4,0.4]],
            "collision_box": [[-0.15,-0.15],[0.15,0.15]], "speed": 0.06,
            "icon": "__base__/graphics/icons/logistic-robot.png"
        });
        let proto = to_prototype("logistic-robot", "logistic-robot", &e, &locale).unwrap();
        assert_eq!(proto.belt_throughput, None);
    }

    #[test]
    fn power_is_parsed_from_energy_usage() {
        let locale = locale_with("assembling-machine-2", "Assembling machine 2");
        let e = json!({
            "collision_box": [[-1.2,-1.2],[1.2,1.2]], "energy_usage": "150kW",
            "icon": "__base__/graphics/icons/assembling-machine-2.png"
        });
        let proto = to_prototype("assembling-machine", "assembling-machine-2", &e, &locale).unwrap();
        assert_eq!(proto.power_kw, Some(150.0));
    }

    #[test]
    fn absent_energy_usage_is_none_not_an_error() {
        let locale = locale_with("transport-belt", "Transport belt");
        let e = json!({
            "collision_box": [[-0.4,-0.4],[0.4,0.4]],
            "icon": "__base__/graphics/icons/transport-belt.png"
        });
        let proto = to_prototype("transport-belt", "transport-belt", &e, &locale).unwrap();
        assert_eq!(proto.power_kw, None);
    }

    #[test]
    fn unparseable_energy_usage_is_an_error_not_a_silent_none() {
        let locale = locale_with("odd-machine", "Odd machine");
        let e = json!({
            "collision_box": [[-0.5,-0.5],[0.5,0.5]], "energy_usage": "150 joules per tick",
            "icon": "__base__/graphics/icons/odd-machine.png"
        });
        let err = to_prototype("assembling-machine", "odd-machine", &e, &locale).unwrap_err();
        assert!(matches!(err, IngestError::UnparseablePower { .. }));
        let msg = err.to_string();
        assert!(msg.contains("odd-machine") && msg.contains("joules"), "got: {msg}");
    }

    #[test]
    fn missing_collision_box_is_an_error_naming_the_entity() {
        let locale = locale_with("mystery-thing", "Mystery thing");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.5,-0.5],[0.5,0.5]],
            "icon": "__base__/graphics/icons/mystery-thing.png"
        });
        let err = to_prototype("simple-entity", "mystery-thing", &e, &locale).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("mystery-thing"), "error should name the entity: {msg}");
        assert!(matches!(err, IngestError::MissingField { .. }));
    }

    #[test]
    fn missing_display_name_is_an_error_naming_the_entity() {
        let locale = Locale::empty();
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.5,-0.5],[0.5,0.5]],
            "collision_box": [[-0.5,-0.5],[0.5,0.5]],
            "icon": "__base__/graphics/icons/no-locale-entity.png"
        });
        let err = to_prototype("simple-entity", "no-locale-entity", &e, &locale).unwrap_err();
        assert!(err.to_string().contains("no-locale-entity"));
    }

    #[test]
    fn missing_icon_is_an_error_naming_the_entity() {
        let locale = locale_with("iconless", "Iconless");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.5,-0.5],[0.5,0.5]],
            "collision_box": [[-0.5,-0.5],[0.5,0.5]]
        });
        let err = to_prototype("simple-entity", "iconless", &e, &locale).unwrap_err();
        assert!(err.to_string().contains("iconless"));
    }

    #[test]
    fn icon_falls_back_to_icons_array() {
        let locale = locale_with("multi-icon-thing", "Multi icon thing");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.5,-0.5],[0.5,0.5]],
            "collision_box": [[-0.5,-0.5],[0.5,0.5]],
            "icons": [{"icon": "__base__/graphics/icons/multi-icon-thing.png"}, {"icon": "overlay.png"}]
        });
        let proto = to_prototype("simple-entity", "multi-icon-thing", &e, &locale).unwrap();
        assert_eq!(proto.icon_path.as_deref(), Some("__base__/graphics/icons/multi-icon-thing.png"));
    }

    #[test]
    fn pickup_and_insert_positions_are_read_as_xy_pairs() {
        let locale = locale_with("inserter", "Inserter");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.4,-0.35],[0.4,0.45]],
            "collision_box": [[-0.15,-0.15],[0.15,0.15]],
            "icon": "__base__/graphics/icons/inserter.png",
            "pickup_position": [0, -1], "insert_position": [0, 1.2]
        });
        let proto = to_prototype("inserter", "inserter", &e, &locale).unwrap();
        assert_eq!(proto.pickup_position, Some((0.0, -1.0)));
        assert_eq!(proto.insert_position, Some((0.0, 1.2)));
    }

    #[test]
    fn supply_area_distance_is_read_for_poles_and_absent_elsewhere() {
        let locale = locale_with("medium-electric-pole", "Medium electric pole");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.4,-0.4],[0.4,0.4]],
            "collision_box": [[-0.15,-0.15],[0.15,0.15]],
            "icon": "__base__/graphics/icons/medium-electric-pole.png",
            // The dump carries both; only the supply area belongs here.
            "supply_area_distance": 3.5, "maximum_wire_distance": 9
        });
        let proto = to_prototype("electric-pole", "medium-electric-pole", &e, &locale).unwrap();
        assert_eq!(proto.supply_area_distance, Some(3.5));

        let locale = locale_with("transport-belt", "Transport belt");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-0.5,-0.5],[0.5,0.5]],
            "collision_box": [[-0.4,-0.4],[0.4,0.4]],
            "icon": "__base__/graphics/icons/transport-belt.png"
        });
        let proto = to_prototype("transport-belt", "transport-belt", &e, &locale).unwrap();
        assert_eq!(proto.supply_area_distance, None);
    }

    #[test]
    fn beacon_supply_area_distance_is_not_a_power_supply_area() {
        // The beacon declares the same key for its module effect radius. Taking
        // it at face value would make every beacon look like a power pole.
        let locale = locale_with("beacon", "Beacon");
        let e = json!({
            "flags": ["player-creation"], "selection_box": [[-1.5,-1.5],[1.5,1.5]],
            "collision_box": [[-1.2,-1.2],[1.2,1.2]],
            "icon": "__base__/graphics/icons/beacon.png",
            "supply_area_distance": 3
        });
        let proto = to_prototype("beacon", "beacon", &e, &locale).unwrap();
        assert_eq!(proto.supply_area_distance, None);
    }
}
