// Filters the raw dump down to placeable entities, and derives tile footprints.
use serde_json::Value;

/// One dump entity plus the prototype-type key it was filed under (the type key
/// matters downstream for belt-throughput detection, see `mapping::to_prototype`).
pub struct PlaceableEntity<'a> {
    pub prototype_type: &'a str,
    pub name: &'a str,
    pub value: &'a Value,
}

/// Entities with `flags` containing `"player-creation"` and a non-null
/// `selection_box`. A handful of the results (bottomless-chest, infinity-chest,
/// infinity-pipe, infinity-cargo-wagon, electric-energy-interface,
/// proxy-container, dummy-rail-support) are editor/debug prototypes that
/// legitimately carry the flag. They are intentionally NOT special-cased out:
/// the grid can hold them, and blueprint import already skips unknown entities
/// gracefully, so excluding them here would just be extra filter logic for no
/// behavioral gain.
pub fn placeable_entities(dump: &Value) -> Vec<PlaceableEntity<'_>> {
    crate::dump::iter_entities(dump)
        .into_iter()
        .filter(|(_, _, value)| has_player_creation_flag(value) && has_selection_box(value))
        .map(|(prototype_type, name, value)| PlaceableEntity { prototype_type, name, value })
        .collect()
}

fn has_player_creation_flag(entity: &Value) -> bool {
    entity
        .get("flags")
        .and_then(Value::as_array)
        .is_some_and(|flags| flags.iter().any(|f| f.as_str() == Some("player-creation")))
}

fn has_selection_box(entity: &Value) -> bool {
    entity.get("selection_box").is_some_and(|v| !v.is_null())
}

/// Tile footprint: explicit `tile_width`/`tile_height` win, checked
/// independently per axis (some prototypes, e.g. `railgun-turret`, declare only
/// one). Otherwise `max(1, ceil(collision_box extent - 0.01))`. The `-0.01`
/// absorbs float noise in real dump values (`2.39999999999999996447...`); the
/// floor of 1 keeps sub-tile colliders (inserters, poles, zero-extent robots) at
/// a full tile.
pub fn tile_size(entity: &Value) -> (u32, u32) {
    (axis_size(entity, "tile_width", 0), axis_size(entity, "tile_height", 1))
}

fn axis_size(entity: &Value, declared_key: &str, axis: usize) -> u32 {
    if let Some(explicit) = entity.get(declared_key).and_then(Value::as_u64) {
        return explicit as u32;
    }
    let extent = collision_extent(entity, axis);
    (1.0f64).max((extent - 0.01).ceil()) as u32
}

fn collision_extent(entity: &Value, axis: usize) -> f64 {
    let corner = |index: usize| -> f64 {
        entity
            .get("collision_box")
            .and_then(Value::as_array)
            .and_then(|b| b.get(index))
            .and_then(Value::as_array)
            .and_then(|p| p.get(axis))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    corner(1) - corner(0)
}

/// Parses Factorio's energy-string format into kilowatts, e.g. `"150kW"` ->
/// `150.0`. Every power-bearing entity in the 2.0.77 dump uses `kW`, but the
/// other SI prefixes are accepted so a future game version that switches a
/// building to `MW` keeps its power rather than losing it. `None` means the
/// string was not recognised at all — the caller treats that as a hard error
/// rather than silently dropping the value.
pub fn parse_power_kw(s: &str) -> Option<f64> {
    let magnitude = s.trim().strip_suffix('W')?;
    let (number, to_kilowatts) = match magnitude.strip_suffix(['k', 'M', 'G', 'T']) {
        Some(n) => match magnitude.as_bytes()[magnitude.len() - 1] {
            b'k' => (n, 1.0),
            b'M' => (n, 1_000.0),
            b'G' => (n, 1_000_000.0),
            _ => (n, 1_000_000_000.0),
        },
        // No prefix: a bare watt value.
        None => (magnitude, 0.001),
    };
    number.trim().parse::<f64>().ok().map(|v| v * to_kilowatts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tile_size_prefers_explicit_declaration() {
        // train-stop declares 2x2 despite a 1x1 collision box (trains pass through).
        let e = json!({"tile_width": 2, "tile_height": 2, "collision_box": [[-0.5, -0.5], [0.5, 0.5]]});
        assert_eq!(tile_size(&e), (2, 2));
    }

    #[test]
    fn tile_size_falls_back_to_collision_box() {
        // assembling-machine-2: collision 2.4 -> 3x3
        let e = json!({"collision_box": [[-1.2, -1.2], [1.2, 1.2]]});
        assert_eq!(tile_size(&e), (3, 3));
    }

    #[test]
    fn tile_size_tolerates_float_noise() {
        let e = json!({"collision_box": [[-1.19999999999999996, -1.19999999999999996],
                                          [1.19999999999999996, 1.19999999999999996]]});
        assert_eq!(tile_size(&e), (3, 3));
    }

    #[test]
    fn tile_size_per_axis_fallback() {
        // railgun-turret declares only tile_height.
        let e = json!({"tile_height": 5, "collision_box": [[-1.2, -1.2], [1.2, 1.2]]});
        assert_eq!(tile_size(&e), (3, 5));
    }

    #[test]
    fn tile_size_clamps_subtile_collision_boxes_to_one() {
        // inserters are 0.3 wide but occupy a full tile.
        let e = json!({"collision_box": [[-0.15, -0.15], [0.15, 0.15]]});
        assert_eq!(tile_size(&e), (1, 1));
    }

    #[test]
    fn tile_size_null_declared_dims_fall_back_like_absent() {
        // real dump entities without an explicit override still have JSON `null`
        // tile_width/tile_height keys, not absent keys.
        let e = json!({"tile_width": null, "tile_height": null, "collision_box": [[-1.2, -1.2], [1.2, 1.2]]});
        assert_eq!(tile_size(&e), (3, 3));
    }

    #[test]
    fn placeable_filter_requires_both_flag_and_selection_box() {
        let dump = json!({
            "flag-only": { "a": {"flags": ["player-creation"], "selection_box": null} },
            "box-only": { "b": {"flags": ["placeable-player"], "selection_box": [[-0.5,-0.5],[0.5,0.5]]} },
            "both": { "c": {"flags": ["player-creation"], "selection_box": [[-0.5,-0.5],[0.5,0.5]]} },
        });
        let names: Vec<&str> = placeable_entities(&dump).iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["c"]);
    }

    #[test]
    fn parse_power_handles_observed_formats() {
        assert_eq!(parse_power_kw("150kW"), Some(150.0));
        assert_eq!(parse_power_kw("2000kW"), Some(2000.0));
        assert_eq!(parse_power_kw("0kW"), Some(0.0));
    }

    #[test]
    fn parse_power_converts_other_si_prefixes() {
        assert_eq!(parse_power_kw("5MW"), Some(5_000.0));
        assert_eq!(parse_power_kw("1.5GW"), Some(1_500_000.0));
        assert_eq!(parse_power_kw("60W"), Some(0.06));
    }

    #[test]
    fn parse_power_rejects_unrecognized_formats() {
        assert_eq!(parse_power_kw("garbage"), None);
        assert_eq!(parse_power_kw("150"), None, "no unit at all");
        assert_eq!(parse_power_kw("W"), None, "unit with no magnitude");
        assert_eq!(parse_power_kw("1.21JW"), None, "unknown prefix");
    }
}
