use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fmt;

// ── Position ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    fn from_u8(v: u8) -> Option<Direction> {
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

    /// Returns the opposite direction (180 degree rotation).
    /// North ↔ South, East ↔ West, etc.
    pub fn opposite(&self) -> Self {
        let v = (*self as u8) ^ 8;
        Direction::from_u8(v).unwrap()
    }

    /// Rotates the direction clockwise by one step (22.5 degrees).
    pub fn rotate_cw(&self) -> Self {
        let v = (*self as u8 + 1) % 16;
        Direction::from_u8(v).unwrap()
    }

    /// Rotates the direction counter-clockwise by one step (22.5 degrees).
    pub fn rotate_ccw(&self) -> Self {
        let v = (*self as u8 + 15) % 16;
        Direction::from_u8(v).unwrap()
    }

    /// Returns true if this is a cardinal direction (North, East, South, or West).
    pub fn is_cardinal(&self) -> bool {
        (*self as u8).is_multiple_of(4)
    }
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

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Direction::North => "North",
            Direction::NorthNorthEast => "NNE",
            Direction::NorthEast => "NE",
            Direction::EastNorthEast => "ENE",
            Direction::East => "East",
            Direction::EastSouthEast => "ESE",
            Direction::SouthEast => "SE",
            Direction::SouthSouthEast => "SSE",
            Direction::South => "South",
            Direction::SouthSouthWest => "SSW",
            Direction::SouthWest => "SW",
            Direction::WestSouthWest => "WSW",
            Direction::West => "West",
            Direction::WestNorthWest => "WNW",
            Direction::NorthWest => "NW",
            Direction::NorthNorthWest => "NNW",
        };
        write!(f, "{}", s)
    }
}

fn is_north(d: &Direction) -> bool {
    *d == Direction::North
}

// ── Entity ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintBookEntry {
    pub index: u32,
    pub blueprint: Blueprint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    // Direction utility method tests

    #[test]
    fn test_direction_opposite() {
        assert_eq!(Direction::North.opposite(), Direction::South);
        assert_eq!(Direction::East.opposite(), Direction::West);
        assert_eq!(Direction::South.opposite(), Direction::North);
        assert_eq!(Direction::West.opposite(), Direction::East);
        assert_eq!(Direction::NorthEast.opposite(), Direction::SouthWest);
        assert_eq!(Direction::SouthWest.opposite(), Direction::NorthEast);
    }

    #[test]
    fn test_direction_rotate_cw() {
        assert_eq!(Direction::North.rotate_cw(), Direction::NorthNorthEast);
        assert_eq!(Direction::NorthNorthEast.rotate_cw(), Direction::NorthEast);
        assert_eq!(Direction::East.rotate_cw(), Direction::EastSouthEast);
        assert_eq!(Direction::NorthNorthWest.rotate_cw(), Direction::North);
    }

    #[test]
    fn test_direction_rotate_ccw() {
        assert_eq!(Direction::North.rotate_ccw(), Direction::NorthNorthWest);
        assert_eq!(Direction::NorthNorthEast.rotate_ccw(), Direction::North);
        assert_eq!(Direction::East.rotate_ccw(), Direction::EastNorthEast);
        assert_eq!(Direction::South.rotate_ccw(), Direction::SouthSouthEast);
    }

    #[test]
    fn test_direction_rotate_full_circle() {
        let mut dir = Direction::North;
        for _ in 0..16 {
            dir = dir.rotate_cw();
        }
        assert_eq!(dir, Direction::North);

        let mut dir = Direction::East;
        for _ in 0..16 {
            dir = dir.rotate_ccw();
        }
        assert_eq!(dir, Direction::East);
    }

    #[test]
    fn test_direction_is_cardinal() {
        assert!(Direction::North.is_cardinal());
        assert!(Direction::East.is_cardinal());
        assert!(Direction::South.is_cardinal());
        assert!(Direction::West.is_cardinal());

        assert!(!Direction::NorthNorthEast.is_cardinal());
        assert!(!Direction::NorthEast.is_cardinal());
        assert!(!Direction::EastNorthEast.is_cardinal());
        assert!(!Direction::SouthWest.is_cardinal());
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
            direction: Direction::North,
            entity_type: None,
            recipe: None,
            connections: None,
            control_behavior: None,
            items: None,
            wires: None,
            tags: None,
            extra: HashMap::new(),
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
                label_color: None,
                description: None,
                icons: None,
                entities: vec![],
                tiles: vec![],
                wires: None,
                schedules: None,
                snap_to_grid: None,
                absolute_snapping: None,
                position_relative_to_grid: None,
                version: 281479275675648,
                extra: HashMap::new(),
            }),
            blueprint_book: None,
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
            blueprint: None,
            blueprint_book: Some(BlueprintBook {
                item: "blueprint-book".to_string(),
                label: Some("My Book".to_string()),
                label_color: None,
                description: None,
                icons: None,
                blueprints: vec![BlueprintBookEntry {
                    index: 0,
                    blueprint: Blueprint {
                        item: "blueprint".to_string(),
                        label: None,
                        label_color: None,
                        description: None,
                        icons: None,
                        entities: vec![],
                        tiles: vec![],
                        wires: None,
                        schedules: None,
                        snap_to_grid: None,
                        absolute_snapping: None,
                        position_relative_to_grid: None,
                        version: 281479275675648,
                        extra: HashMap::new(),
                    },
                }],
                active_index: 0,
                version: 281479275675648,
                extra: HashMap::new(),
            }),
        };

        let json_str = serde_json::to_string(&data).unwrap();
        let roundtripped: BlueprintData = serde_json::from_str(&json_str).unwrap();
        assert_eq!(data, roundtripped);
    }

    #[test]
    fn test_blueprint_entities_always_emitted() {
        let bp = Blueprint {
            item: "blueprint".to_string(),
            label: None,
            label_color: None,
            description: None,
            icons: None,
            entities: vec![],
            tiles: vec![],
            wires: None,
            schedules: None,
            snap_to_grid: None,
            absolute_snapping: None,
            position_relative_to_grid: None,
            version: 0,
            extra: HashMap::new(),
        };

        let val = serde_json::to_value(&bp).unwrap();
        // entities should always be present even when empty
        assert_eq!(val.get("entities"), Some(&json!([])));
        // tiles should be omitted when empty
        assert!(val.get("tiles").is_none());
    }
}
