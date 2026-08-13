use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

// ── Position ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

// ── Color ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<f64>,
}

// ── SignalId ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalId {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub signal_type: Option<String>,
}

// ── Icon ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Icon {
    pub index: u32,
    pub signal: SignalId,
}

// ── Direction ─────────────────────────────────────────────────────────

/// Factorio 2.0 uses 16 directions (0–15). Cardinal directions are at
/// multiples of 4: North=0, East=4, South=8, West=12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    #[default]
    North = 0,
    NorthNorthEast = 1,
    NorthEast = 2,
    EastNorthEast = 3,
    East = 4,
    EastSouthEast = 5,
    SouthEast = 6,
    SouthSouthEast = 7,
    South = 8,
    SouthSouthWest = 9,
    SouthWest = 10,
    WestSouthWest = 11,
    West = 12,
    WestNorthWest = 13,
    NorthWest = 14,
    NorthNorthWest = 15,
}

impl Direction {
    /// Raw Factorio direction byte (0–15 in the 2.0 scheme).
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Option<Direction> {
        match v {
            0 => Some(Direction::North),
            1 => Some(Direction::NorthNorthEast),
            2 => Some(Direction::NorthEast),
            3 => Some(Direction::EastNorthEast),
            4 => Some(Direction::East),
            5 => Some(Direction::EastSouthEast),
            6 => Some(Direction::SouthEast),
            7 => Some(Direction::SouthSouthEast),
            8 => Some(Direction::South),
            9 => Some(Direction::SouthSouthWest),
            10 => Some(Direction::SouthWest),
            11 => Some(Direction::WestSouthWest),
            12 => Some(Direction::West),
            13 => Some(Direction::WestNorthWest),
            14 => Some(Direction::NorthWest),
            15 => Some(Direction::NorthNorthWest),
            _ => None,
        }
    }

    /// Map a Factorio 1.x direction byte (0–7 scheme) into the 2.0 16-way value.
    ///
    /// 1.x N/E/S/W = 0/2/4/6 become 2.0 0/4/8/12 (`value * 2`).
    pub fn from_legacy_u8(v: u8) -> Option<Direction> {
        if v <= 7 {
            Direction::from_u8(v * 2)
        } else {
            None
        }
    }

    /// Upgrade a direction that was decoded under the 2.0 table but originated
    /// as a 1.x cardinal encoding (raw 0/2/4/6 → 2.0 N/E/S/W).
    pub fn upgrade_from_legacy(self) -> Direction {
        Direction::from_legacy_u8(self.as_u8()).unwrap_or(self)
    }
}

/// Factorio packs `version` as `major<<48 | minor<<32 | patch<<16 | dev`.
pub fn factorio_major_version(version: u64) -> u16 {
    (version >> 48) as u16
}

/// Detect Factorio 1.x cardinal direction encoding in a set of decoded values.
///
/// 1.x uses N/E/S/W = 0/2/4/6. After a naive 2.0 decode those become
/// North/NorthEast/East/SouthEast. A pure 2.0 blueprint that only faces
/// North+East is {0, 4} and must **not** be rewritten (4 would become South).
///
/// Rules:
/// - Any direction outside `{0,2,4,6}` ⇒ not pure 1.x encoding (modern 2.0).
/// - Blueprint major version `< 2` ⇒ treat any pure `{0,2,4,6}` set as legacy
///   (covers pure-South raw 4 and North+South-only sets that lack East/West markers).
/// - Blueprint major version `≥ 2` ⇒ only upgrade when a definitive 1.x marker
///   (decoded 2 or 6) is present; ambiguous `{0,4}` stays as 2.0 North+East.
pub fn directions_look_legacy(
    dirs: impl IntoIterator<Item = Direction>,
    version: u64,
) -> bool {
    let mut saw_marker = false;
    let mut any = false;
    for d in dirs {
        any = true;
        match d.as_u8() {
            0 | 4 => {}
            2 | 6 => saw_marker = true,
            _ => return false,
        }
    }
    if !any {
        return false;
    }
    if factorio_major_version(version) < 2 {
        // 1.x always used the 0/2/4/6 cardinal scheme.
        return true;
    }
    // 2.0+: require a definitive East/West marker so pure North+East is kept.
    saw_marker
}

impl Serialize for Direction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for Direction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = u8::deserialize(deserializer)?;
        Direction::from_u8(v).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid direction value: {v}"))
        })
    }
}

fn is_north(d: &Direction) -> bool {
    *d == Direction::North
}

// ── Item filter ───────────────────────────────────────────────────────

/// One filter slot on an inserter, loader or filtered container — Factorio's
/// `BlueprintItemFilter`, mirrored field for field from the runtime docs at
/// the version this crate targets (2.0.77).
///
/// Only `index` is required, and it is **1-based**. Every other field is
/// optional and genuinely absent in practice: a plain item filter is just
/// `{"index": 1, "name": "iron-plate"}`, and omitting `quality`/`comparator`
/// is what makes the filter match an item of *any* quality rather than only
/// normal. `name` is `Option` rather than `String` because a real blueprint
/// may carry a quality-only filter, which must parse rather than error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemFilter {
    pub index: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparator: Option<String>,
}

// ── Entity ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub entity_number: u32,
    pub name: String,
    pub position: Position,

    #[serde(default, skip_serializing_if = "is_north")]
    pub direction: Direction,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub connections: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_behavior: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wires: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<serde_json::Value>,

    /// Whether the entity's `filters` are actually applied. Factorio defaults
    /// this to **false**, so a `filters` array on its own is stored and
    /// ignored — an inserter that reads as filtered in the blueprint and
    /// grabs anything in game. Anything writing `filters` must write this too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_filters: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<ItemFilter>>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ── Tile ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub name: String,
    pub position: Position,
}

// ── Blueprint ─────────────────────────────────────────────────────────

fn is_empty_vec<T>(v: &[T]) -> bool {
    v.is_empty()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Blueprint {
    pub item: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_color: Option<Color>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,

    #[serde(default)]
    pub entities: Vec<Entity>,

    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub tiles: Vec<Tile>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub wires: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedules: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub snap_to_grid: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_snapping: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_relative_to_grid: Option<Position>,

    pub version: u64,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ── BlueprintBook ─────────────────────────────────────────────────────

/// One slot in a blueprint book. Factorio allows a leaf `blueprint`, a nested
/// `blueprint_book`, both absent (empty/index-only slot), but not both present
/// in normal game data — we model both as optional to match the wire shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintBookEntry {
    pub index: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint: Option<Blueprint>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint_book: Option<BlueprintBook>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlueprintBook {
    pub item: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_color: Option<Color>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,

    pub blueprints: Vec<BlueprintBookEntry>,
    pub active_index: u32,
    pub version: u64,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// ── BlueprintData (top-level envelope) ────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlueprintData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blueprint: Option<Blueprint>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub blueprint_book: Option<BlueprintBook>,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Direction serde tests

    #[test]
    fn test_direction_serializes_as_u8() {
        let val = serde_json::to_value(Direction::East).unwrap();
        assert_eq!(val, json!(4));
    }

    #[test]
    fn test_direction_deserializes_from_u8() {
        let dir: Direction = serde_json::from_value(json!(8)).unwrap();
        assert_eq!(dir, Direction::South);
    }

    #[test]
    fn test_direction_default_is_north() {
        assert_eq!(Direction::default(), Direction::North);
    }

    #[test]
    fn test_direction_zero_deserializes() {
        let dir: Direction = serde_json::from_value(json!(0)).unwrap();
        assert_eq!(dir, Direction::North);
    }

    #[test]
    fn test_direction_invalid_value_errors() {
        let result = serde_json::from_value::<Direction>(json!(16));
        assert!(result.is_err());

        let result = serde_json::from_value::<Direction>(json!(255));
        assert!(result.is_err());
    }

    #[test]
    fn test_from_legacy_u8_maps_1x_cardinals() {
        assert_eq!(Direction::from_legacy_u8(0), Some(Direction::North));
        assert_eq!(Direction::from_legacy_u8(2), Some(Direction::East));
        assert_eq!(Direction::from_legacy_u8(4), Some(Direction::South));
        assert_eq!(Direction::from_legacy_u8(6), Some(Direction::West));
        assert!(Direction::from_legacy_u8(8).is_none());
    }

    #[test]
    fn test_upgrade_from_legacy_after_naive_2_0_decode() {
        // Codec maps 1.x bytes onto 2.0 variants by raw value; upgrade fixes them.
        assert_eq!(Direction::NorthEast.upgrade_from_legacy(), Direction::East); // 2
        assert_eq!(Direction::East.upgrade_from_legacy(), Direction::South); // 4
        assert_eq!(Direction::SouthEast.upgrade_from_legacy(), Direction::West); // 6
        assert_eq!(Direction::North.upgrade_from_legacy(), Direction::North); // 0
    }

    const VERSION_1_1: u64 = 281479275675648; // major 1
    const VERSION_2_0: u64 = 2u64 << 48;

    #[test]
    fn test_factorio_major_version() {
        assert_eq!(factorio_major_version(VERSION_1_1), 1);
        assert_eq!(factorio_major_version(VERSION_2_0), 2);
        assert_eq!(factorio_major_version(0), 0);
    }

    #[test]
    fn test_directions_look_legacy_version_and_markers() {
        // Definitive 1.x East/West markers — legacy under both majors.
        assert!(directions_look_legacy(
            [Direction::North, Direction::NorthEast],
            VERSION_2_0
        ));
        assert!(directions_look_legacy([Direction::SouthEast], VERSION_2_0));
        assert!(directions_look_legacy([Direction::SouthEast], VERSION_1_1));

        // Ambiguous pure {0,4}: 2.0 keeps as North+East; 1.x upgrades (4→South).
        assert!(!directions_look_legacy(
            [Direction::North, Direction::East],
            VERSION_2_0
        ));
        assert!(directions_look_legacy(
            [Direction::North, Direction::East],
            VERSION_1_1
        ));

        // Pure South (raw 4 only): same disambiguation.
        assert!(!directions_look_legacy([Direction::East], VERSION_2_0));
        assert!(directions_look_legacy([Direction::East], VERSION_1_1));

        // Any true 2.0-only value rejects legacy mode regardless of version.
        assert!(!directions_look_legacy(
            [Direction::NorthEast, Direction::South],
            VERSION_1_1
        ));
        assert!(!directions_look_legacy(
            [Direction::NorthEast, Direction::South],
            VERSION_2_0
        ));
        assert!(!directions_look_legacy(std::iter::empty(), VERSION_1_1));
    }

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
}
