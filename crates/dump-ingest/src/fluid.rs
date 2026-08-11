// Extracts pipe connections from an entity's `fluid_box`/`fluid_boxes` fields.
use serde_json::Value;

use factorio_grid::prototype::{FluidConnection, FluidConnectionType};

/// Reads both dump shapes — `fluid_boxes` (plural array, e.g. chemical-plant) and
/// `fluid_box` (singular object, e.g. pump) — and concatenates their
/// `pipe_connections`. No verified entity in the real dump carries both fields at
/// once, but reading plural-then-singular keeps the output order deterministic
/// either way.
pub fn fluid_connections(entity: &Value) -> Vec<FluidConnection> {
    let mut boxes: Vec<&Value> = Vec::new();
    if let Some(arr) = entity.get("fluid_boxes").and_then(Value::as_array) {
        boxes.extend(arr.iter());
    }
    if let Some(single) = entity.get("fluid_box").filter(|v| !v.is_null()) {
        boxes.push(single);
    }
    boxes.into_iter().flat_map(connections_in_box).collect()
}

fn connections_in_box(fluid_box: &Value) -> Vec<FluidConnection> {
    let box_level_type = connection_type_from(fluid_box.get("production_type"));
    let Some(conns) = fluid_box.get("pipe_connections").and_then(Value::as_array) else {
        return Vec::new();
    };

    conns
        .iter()
        .map(|c| {
            let position = c.get("position").and_then(Value::as_array);
            let dx = position.and_then(|p| p.first()).and_then(Value::as_f64).unwrap_or(0.0);
            let dy = position.and_then(|p| p.get(1)).and_then(Value::as_f64).unwrap_or(0.0);
            let connection_type = c
                .get("flow_direction")
                .and_then(Value::as_str)
                .map(hyphenated_to_connection_type)
                .unwrap_or_else(|| box_level_type.clone());
            FluidConnection { dx, dy, connection_type }
        })
        .collect()
}

fn connection_type_from(value: Option<&Value>) -> FluidConnectionType {
    value.and_then(Value::as_str).map(hyphenated_to_connection_type).unwrap_or(FluidConnectionType::InputOutput)
}

/// `flow_direction`/`production_type` use hyphenated strings (`"input-output"`)
/// while the Rust enum's serde rename is snake_case (`input_output`) — matched
/// explicitly here rather than round-tripped through serde. `"none"` and any
/// unrecognized value fall back to `InputOutput` (a plain pipe accepts both).
fn hyphenated_to_connection_type(s: &str) -> FluidConnectionType {
    match s {
        "input" => FluidConnectionType::Input,
        "output" => FluidConnectionType::Output,
        _ => FluidConnectionType::InputOutput,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_plural_fluid_boxes_shape() {
        let entity = json!({
            "fluid_boxes": [
                { "production_type": "input", "pipe_connections": [
                    { "flow_direction": "input", "position": [-1.0, -1.0] }
                ]},
                { "production_type": "output", "pipe_connections": [
                    { "flow_direction": "output", "position": [1.0, 1.0] }
                ]},
            ]
        });
        let conns = fluid_connections(&entity);
        assert_eq!(conns.len(), 2);
        assert_eq!(conns[0], FluidConnection { dx: -1.0, dy: -1.0, connection_type: FluidConnectionType::Input });
        assert_eq!(conns[1], FluidConnection { dx: 1.0, dy: 1.0, connection_type: FluidConnectionType::Output });
    }

    #[test]
    fn reads_singular_fluid_box_shape() {
        let entity = json!({
            "fluid_box": {
                "pipe_connections": [
                    { "flow_direction": "output", "position": [0.0, -0.5] },
                    { "flow_direction": "input", "position": [0.0, 0.5] },
                ]
            }
        });
        let conns = fluid_connections(&entity);
        assert_eq!(conns.len(), 2);
        assert_eq!(conns[0].connection_type, FluidConnectionType::Output);
        assert_eq!(conns[1].connection_type, FluidConnectionType::Input);
    }

    #[test]
    fn fluid_connections_read_both_field_shapes() {
        let entity = json!({
            "fluid_boxes": [
                { "production_type": "input", "pipe_connections": [
                    { "flow_direction": "input", "position": [-1.0, -1.0] }
                ]},
            ],
            "fluid_box": {
                "pipe_connections": [
                    { "flow_direction": "output", "position": [0.0, 0.5] },
                ]
            }
        });
        let conns = fluid_connections(&entity);
        assert_eq!(conns.len(), 2, "both fluid_boxes and fluid_box entries should be counted");
    }

    #[test]
    fn fluid_connection_type_prefers_flow_direction_over_production_type() {
        let entity = json!({
            "fluid_box": {
                "production_type": "input",
                "pipe_connections": [
                    { "flow_direction": "output", "position": [0.0, 0.0] }
                ]
            }
        });
        let conns = fluid_connections(&entity);
        assert_eq!(conns[0].connection_type, FluidConnectionType::Output);
    }

    #[test]
    fn fluid_connection_type_falls_back_to_production_type_when_flow_direction_absent() {
        let entity = json!({
            "fluid_box": {
                "production_type": "output",
                "pipe_connections": [ { "position": [0.0, 0.0] } ]
            }
        });
        let conns = fluid_connections(&entity);
        assert_eq!(conns[0].connection_type, FluidConnectionType::Output);
    }

    #[test]
    fn fluid_connection_type_defaults_to_input_output_when_both_absent() {
        // storage-tank shape: no top-level production_type, no per-connection flow_direction.
        let entity = json!({
            "fluid_box": {
                "pipe_connections": [ { "direction": 0, "position": [-1.0, -1.0] } ]
            }
        });
        let conns = fluid_connections(&entity);
        assert_eq!(conns[0].connection_type, FluidConnectionType::InputOutput);
    }

    #[test]
    fn fluid_connection_type_maps_none_production_type_to_input_output() {
        let entity = json!({
            "fluid_box": {
                "production_type": "none",
                "pipe_connections": [ { "position": [0.0, 0.0] } ]
            }
        });
        let conns = fluid_connections(&entity);
        assert_eq!(conns[0].connection_type, FluidConnectionType::InputOutput);
    }

    #[test]
    fn no_fluid_fields_yields_no_connections() {
        let entity = json!({ "name": "transport-belt" });
        assert!(fluid_connections(&entity).is_empty());
    }
}
