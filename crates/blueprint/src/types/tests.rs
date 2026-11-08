// Tests for the blueprint data types: entity fields, filters, unknown-field survival, books.
use super::*;
use serde_json::json;

// Entity tests

#[test]
fn test_entity_with_all_fields() {
    let entity = Entity {
        entity_number: 1,
        name: "assembling-machine-2".to_string(),
        position: Position { x: 0.5, y: 0.5 },
        direction: Direction::East,
        entity_type: Some("input".to_string()),
        recipe: Some("iron-gear-wheel".to_string()),
        connections: Some(json!({"1": {"red": [{"entity_id": 2}]}})),
        control_behavior: Some(json!({"circuit_condition": {"comparator": ">"}})),
        items: Some(json!({"speed-module": 2})),
        wires: Some(json!([[1, 2, 3, 4]])),
        tags: Some(json!({"custom": "value"})),
        use_filters: Some(true),
        filters: Some(vec![ItemFilter {
            index: 1,
            name: Some("iron-plate".to_string()),
            quality: None,
            comparator: None,
        }]),
        extra: HashMap::new(),
    };

    let json_str = serde_json::to_string(&entity).unwrap();
    let roundtripped: Entity = serde_json::from_str(&json_str).unwrap();
    assert_eq!(entity, roundtripped);
}

#[test]
fn test_entity_none_fields_omitted() {
    let entity = Entity {
        entity_number: 1,
        name: "transport-belt".to_string(),
        position: Position { x: 1.5, y: 2.5 },
        ..Default::default()
    };

    let val = serde_json::to_value(&entity).unwrap();
    let obj = val.as_object().unwrap();

    // Direction North should be omitted
    assert!(!obj.contains_key("direction"));
    // Optional None fields should be absent (not null)
    assert!(!obj.contains_key("type"));
    assert!(!obj.contains_key("recipe"));
    assert!(!obj.contains_key("connections"));
    assert!(!obj.contains_key("control_behavior"));
    assert!(!obj.contains_key("items"));
    assert!(!obj.contains_key("wires"));
    assert!(!obj.contains_key("tags"));
    // An unfiltered entity must serialize exactly as it did before filters
    // existed — every blueprint the generator already emits depends on it.
    assert!(!obj.contains_key("use_filters"));
    assert!(!obj.contains_key("filters"));
}

/// The wire format, pinned against Factorio's `BlueprintItemFilter`
/// (lua-api 2.0.77): `index` is required and 1-based, everything else is
/// omitted when absent — so a plain item filter is exactly two keys.
#[test]
fn test_item_filter_omits_absent_quality_and_comparator() {
    let f = ItemFilter {
        index: 1,
        name: Some("uranium-238".to_string()),
        quality: None,
        comparator: None,
    };
    assert_eq!(
        serde_json::to_value(&f).unwrap(),
        json!({"index": 1, "name": "uranium-238"})
    );
}

/// `name` is optional in Factorio's own schema, so a quality-only filter
/// out of a real blueprint has to parse rather than error.
#[test]
fn test_item_filter_without_a_name_parses() {
    let f: ItemFilter =
        serde_json::from_str(r#"{"index": 2, "quality": "legendary", "comparator": ">="}"#)
            .unwrap();
    assert_eq!(f.index, 2);
    assert_eq!(f.name, None);
    assert_eq!(f.quality.as_deref(), Some("legendary"));
}

#[test]
fn test_entity_unknown_fields_preserved() {
    let json_str = r#"{
        "entity_number": 1,
        "name": "test-entity",
        "position": {"x": 0.0, "y": 0.0},
        "some_modded_field": "hello",
        "another_unknown": 42
    }"#;

    let entity: Entity = serde_json::from_str(json_str).unwrap();
    assert_eq!(
        entity.extra.get("some_modded_field"),
        Some(&json!("hello"))
    );
    assert_eq!(entity.extra.get("another_unknown"), Some(&json!(42)));

    // Round-trip preserves them
    let re_serialized = serde_json::to_value(&entity).unwrap();
    let obj = re_serialized.as_object().unwrap();
    assert_eq!(obj.get("some_modded_field"), Some(&json!("hello")));
    assert_eq!(obj.get("another_unknown"), Some(&json!(42)));
}

#[test]
fn test_entity_type_rename() {
    let json_str = r#"{
        "entity_number": 1,
        "name": "underground-belt",
        "position": {"x": 0.0, "y": 0.0},
        "type": "input"
    }"#;

    let entity: Entity = serde_json::from_str(json_str).unwrap();
    assert_eq!(entity.entity_type, Some("input".to_string()));

    let val = serde_json::to_value(&entity).unwrap();
    assert_eq!(val.get("type"), Some(&json!("input")));
    // Should NOT have "entity_type" in JSON
    assert!(val.get("entity_type").is_none());
}

// Blueprint / BlueprintData tests

#[test]
fn test_blueprint_data_with_blueprint() {
    let data = BlueprintData {
        blueprint: Some(Blueprint {
            item: "blueprint".to_string(),
            label: Some("Test".to_string()),
            version: 281479275675648,
            ..Default::default()
        }),
        ..Default::default()
    };

    let json_str = serde_json::to_string(&data).unwrap();
    let roundtripped: BlueprintData = serde_json::from_str(&json_str).unwrap();
    assert_eq!(data, roundtripped);

    // Verify structure
    let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert!(val.get("blueprint").is_some());
    assert!(val.get("blueprint_book").is_none());
}

#[test]
fn test_blueprint_data_with_book() {
    let data = BlueprintData {
        blueprint_book: Some(BlueprintBook {
            item: "blueprint-book".to_string(),
            label: Some("My Book".to_string()),
            blueprints: vec![BlueprintBookEntry {
                index: 0,
                blueprint: Some(Blueprint {
                    item: "blueprint".to_string(),
                    version: 281479275675648,
                    ..Default::default()
                }),
                blueprint_book: None,
            }],
            active_index: 0,
            version: 281479275675648,
            ..Default::default()
        }),
        ..Default::default()
    };

    let json_str = serde_json::to_string(&data).unwrap();
    let roundtripped: BlueprintData = serde_json::from_str(&json_str).unwrap();
    assert_eq!(data, roundtripped);
}

#[test]
fn test_nested_blueprint_book_entry_decodes() {
    // Real Factorio books can nest entries that carry blueprint_book instead
    // of blueprint (or empty index-only slots).
    let json = r#"{
        "blueprint_book": {
            "item": "blueprint-book",
            "label": "Outer",
            "active_index": 0,
            "version": 281479275675648,
            "blueprints": [
                {
                    "index": 0,
                    "blueprint": {
                        "item": "blueprint",
                        "label": "Leaf",
                        "entities": [],
                        "version": 281479275675648
                    }
                },
                {
                    "index": 1,
                    "blueprint_book": {
                        "item": "blueprint-book",
                        "label": "Inner",
                        "active_index": 0,
                        "version": 281479275675648,
                        "blueprints": [
                            {
                                "index": 0,
                                "blueprint": {
                                    "item": "blueprint",
                                    "label": "Nested leaf",
                                    "entities": [],
                                    "version": 281479275675648
                                }
                            }
                        ]
                    }
                },
                { "index": 2 }
            ]
        }
    }"#;

    let data: BlueprintData = serde_json::from_str(json).unwrap();
    let book = data.blueprint_book.as_ref().expect("outer book");
    assert_eq!(book.blueprints.len(), 3);

    let leaf = book.blueprints[0]
        .blueprint
        .as_ref()
        .expect("index 0 is a leaf blueprint");
    assert_eq!(leaf.label.as_deref(), Some("Leaf"));
    assert!(book.blueprints[0].blueprint_book.is_none());

    let inner = book.blueprints[1]
        .blueprint_book
        .as_ref()
        .expect("index 1 is a nested book");
    assert_eq!(inner.label.as_deref(), Some("Inner"));
    assert!(book.blueprints[1].blueprint.is_none());
    assert_eq!(
        inner.blueprints[0]
            .blueprint
            .as_ref()
            .unwrap()
            .label
            .as_deref(),
        Some("Nested leaf")
    );

    // Empty index-only slot.
    assert!(book.blueprints[2].blueprint.is_none());
    assert!(book.blueprints[2].blueprint_book.is_none());

    // Round-trip through serde keeps the nested shape.
    let again: BlueprintData =
        serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
    assert_eq!(data, again);
}

#[test]
fn test_blueprint_entities_always_emitted() {
    let bp = Blueprint {
        item: "blueprint".to_string(),
        ..Default::default()
    };

    let val = serde_json::to_value(&bp).unwrap();
    // entities should always be present even when empty
    assert_eq!(val.get("entities"), Some(&json!([])));
    // tiles should be omitted when empty
    assert!(val.get("tiles").is_none());
}
