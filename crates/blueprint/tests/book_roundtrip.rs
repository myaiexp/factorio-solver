//! Integration tests: blueprint books, flat and nested, survive decode -> encode -> decode.

mod common;

use common::{assert_json_equivalent, assert_roundtrip};
use factorio_blueprint::fixtures::BLUEPRINT_BOOK;
use factorio_blueprint::decode;

#[test]
fn test_blueprint_book() {
    assert_roundtrip(BLUEPRINT_BOOK);
    assert_json_equivalent(BLUEPRINT_BOOK);

    // Verify it's a book with 3 blueprints
    let data = decode(BLUEPRINT_BOOK).unwrap();
    assert!(data.blueprint.is_none(), "should not be a single blueprint");
    let book = data.blueprint_book.expect("should be a blueprint book");
    assert_eq!(book.blueprints.len(), 3);
    assert_eq!(book.label.as_deref(), Some("Test Book"));
    assert_eq!(book.active_index, 0);

    // Verify individual blueprint labels
    assert_eq!(
        book.blueprints[0]
            .blueprint
            .as_ref()
            .expect("flat leaf")
            .label
            .as_deref(),
        Some("Belt 1")
    );
    assert_eq!(
        book.blueprints[1]
            .blueprint
            .as_ref()
            .expect("flat leaf")
            .label
            .as_deref(),
        Some("Belt 2")
    );
    assert_eq!(
        book.blueprints[2]
            .blueprint
            .as_ref()
            .expect("flat leaf")
            .label
            .as_deref(),
        Some("Underground")
    );
}

#[test]
fn test_nested_blueprint_book_roundtrip() {
    use factorio_blueprint::{Blueprint, BlueprintBook, BlueprintBookEntry, BlueprintData, encode};

    let nested = BlueprintData {
        blueprint_book: Some(BlueprintBook {
            item: "blueprint-book".to_string(),
            label: Some("Outer".to_string()),
            blueprints: vec![
                BlueprintBookEntry {
                    index: 0,
                    blueprint: Some(Blueprint {
                        item: "blueprint".to_string(),
                        label: Some("Leaf".to_string()),
                        version: 281479275675648,
                        ..Default::default()
                    }),
                    blueprint_book: None,
                },
                BlueprintBookEntry {
                    index: 1,
                    blueprint: None,
                    blueprint_book: Some(BlueprintBook {
                        item: "blueprint-book".to_string(),
                        label: Some("Inner".to_string()),
                        blueprints: vec![BlueprintBookEntry {
                            index: 0,
                            blueprint: Some(Blueprint {
                                item: "blueprint".to_string(),
                                label: Some("Nested leaf".to_string()),
                                entities: vec![factorio_blueprint::Entity {
                                    entity_number: 1,
                                    name: "transport-belt".to_string(),
                                    position: factorio_blueprint::Position { x: 0.5, y: 0.5 },
                                    direction: factorio_blueprint::Direction::South,
                                    ..Default::default()
                                }],
                                version: 281479275675648,
                                ..Default::default()
                            }),
                            blueprint_book: None,
                        }],
                        active_index: 0,
                        version: 281479275675648,
                        ..Default::default()
                    }),
                },
                BlueprintBookEntry {
                    index: 2,
                    blueprint: None,
                    blueprint_book: None,
                },
            ],
            active_index: 0,
            version: 281479275675648,
            ..Default::default()
        }),
        ..Default::default()
    };

    let encoded = encode(&nested).expect("encode nested book");
    let decoded = decode(&encoded).expect("decode nested book");
    assert_eq!(nested, decoded);

    let book = decoded.blueprint_book.expect("outer book");
    assert_eq!(book.blueprints.len(), 3);
    assert_eq!(
        book.blueprints[0].blueprint.as_ref().unwrap().label.as_deref(),
        Some("Leaf")
    );
    let inner = book.blueprints[1].blueprint_book.as_ref().unwrap();
    assert_eq!(inner.label.as_deref(), Some("Inner"));
    assert_eq!(
        inner.blueprints[0]
            .blueprint
            .as_ref()
            .unwrap()
            .entities
            .len(),
        1
    );
    assert!(book.blueprints[2].blueprint.is_none());
    assert!(book.blueprints[2].blueprint_book.is_none());
}
