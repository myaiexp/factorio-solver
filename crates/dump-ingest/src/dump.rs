// Walks the raw dump's `{prototype_type: {entity_name: entity}}` nested shape.
use serde_json::Value;

/// Every `(prototype_type, entity_name, entity_value)` triple in the dump.
/// Top-level values that aren't JSON objects are skipped rather than causing a
/// panic here — real dumps are all-object, but a corrupt/partial one should not
/// crash the walk (individual entities are still validated strictly downstream,
/// in `entities::to_prototype`).
pub fn iter_entities(dump: &Value) -> Vec<(&str, &str, &Value)> {
    let Some(top) = dump.as_object() else { return Vec::new() };

    let mut out = Vec::new();
    for (prototype_type, entities) in top {
        let Some(entities) = entities.as_object() else { continue };
        for (name, entity) in entities {
            out.push((prototype_type.as_str(), name.as_str(), entity));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn walks_nested_type_to_name_to_entity_shape() {
        let dump = json!({
            "transport-belt": { "transport-belt": {"speed": 0.03125} },
            "inserter": { "inserter": {} },
        });
        let mut entries = iter_entities(&dump);
        entries.sort_by_key(|(_, name, _)| *name);

        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].0, entries[0].1), ("inserter", "inserter"));
        assert_eq!((entries[1].0, entries[1].1), ("transport-belt", "transport-belt"));
    }

    #[test]
    fn skips_non_object_top_level_values_defensively() {
        let dump = json!({
            "some-string-value": "not an object",
            "transport-belt": { "transport-belt": {} },
        });
        let entries = iter_entities(&dump);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, "transport-belt");
    }

    #[test]
    fn passes_through_non_object_entity_values_for_downstream_filtering() {
        // Only the top-level {type: {name: entity}} shape is validated here; an
        // individual entity that isn't itself an object still comes through as a
        // triple — the null-safe `.get()` accessors in `entities`/`mapping`
        // naturally exclude it (no "flags" on a bare string), so duplicating
        // that check here would just be redundant defensive code.
        let dump = json!({
            "weird-type": { "not-an-entity": "just a string" },
        });
        let entries = iter_entities(&dump);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], ("weird-type", "not-an-entity", &json!("just a string")));
    }

    #[test]
    fn non_object_top_level_dump_yields_nothing() {
        let dump = json!("not even a dict");
        assert!(iter_entities(&dump).is_empty());
    }
}
