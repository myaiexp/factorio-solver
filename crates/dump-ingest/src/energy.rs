// Reads an entity's `energy_source.type` into the shared `EnergySource` enum.
use serde_json::Value;

use factorio_grid::prototype::EnergySource;

use crate::error::IngestError;

/// Absent or null `energy_source` is `None` (belts, poles, chests). A type
/// string this tool does not know is an error naming the entity, not a
/// silent `None` — a missing type would read as "needs no power and no fuel"
/// downstream, the same partial-write-passing-as-complete trap `power_kw`
/// refuses.
pub fn energy_source(name: &str, entity: &Value) -> Result<Option<EnergySource>, IngestError> {
    let Some(source) = entity.get("energy_source").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let kind = source.get("type").and_then(Value::as_str);
    let parsed = match kind {
        Some("electric") => EnergySource::Electric,
        Some("burner") => EnergySource::Burner,
        Some("heat") => EnergySource::Heat,
        Some("fluid") => EnergySource::Fluid,
        Some("void") => EnergySource::Void,
        _ => {
            return Err(IngestError::UnknownEnergySource {
                name: name.to_string(),
                value: source.get("type").map_or_else(|| "<missing>".to_string(), Value::to_string),
            })
        }
    };
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_dump_type_maps_to_its_variant() {
        for (raw, want) in [
            ("electric", EnergySource::Electric),
            ("burner", EnergySource::Burner),
            ("heat", EnergySource::Heat),
            ("fluid", EnergySource::Fluid),
            ("void", EnergySource::Void),
        ] {
            let e = json!({ "energy_source": { "type": raw } });
            assert_eq!(energy_source("x", &e).unwrap(), Some(want), "{raw}");
        }
    }

    #[test]
    fn absent_or_null_energy_source_is_none() {
        assert_eq!(energy_source("belt", &json!({})).unwrap(), None);
        assert_eq!(energy_source("belt", &json!({ "energy_source": null })).unwrap(), None);
    }

    #[test]
    fn an_unknown_or_missing_type_is_an_error_naming_the_entity() {
        for e in [json!({ "energy_source": { "type": "plasma" } }), json!({ "energy_source": {} })] {
            let msg = energy_source("widget", &e).unwrap_err().to_string();
            assert!(msg.contains("widget"), "message should name the entity: {msg}");
        }
    }
}
