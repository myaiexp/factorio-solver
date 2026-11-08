// Blueprint data types: entities, tiles, blueprints, books and the top-level envelope.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::direction::Direction;

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

#[cfg(test)]
mod tests;
