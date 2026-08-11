// Recipe model + registry, loaded from the dump-derived recipes.json
use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Deserializer, Serialize};

// ── Item / fluid amounts ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Item,
    Fluid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemAmount {
    pub name: String,
    /// Serialized as `type`, matching the dump's own key.
    #[serde(rename = "type")]
    pub kind: ItemKind,
    pub amount: f64,
}

// ── Recipe ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default = "default_category")]
    pub category: String,
    /// Craft time in seconds at speed 1.
    #[serde(default = "default_energy_required")]
    pub energy_required: f64,
    #[serde(default, deserialize_with = "de_item_amounts")]
    pub ingredients: Vec<ItemAmount>,
    #[serde(default, deserialize_with = "de_item_amounts")]
    pub results: Vec<ItemAmount>,
    /// Available without research. Absent in the dump means `true` — the
    /// opposite of `bool::default()`, which would mark 242 recipes
    /// (iron-plate, transport-belt, stone-furnace…) research-locked.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Hidden from the player's crafting menu. Kept, not filtered: Space
    /// Age's recycling recipes are hidden but real.
    #[serde(default)]
    pub hidden: bool,
}

fn default_true() -> bool {
    true
}

fn default_category() -> String {
    "crafting".to_string()
}

fn default_energy_required() -> f64 {
    0.5
}

/// Accepts a JSON array, an empty object, or null.
///
/// Factorio dumps an empty Lua table as `{}` because it cannot tell an
/// empty array from an empty map, and `biter-egg` is a real recipe that
/// hits this path. `#[serde(default)]` alone is not enough: it covers a
/// *missing* key, not a key present with a mismatched type.
fn de_item_amounts<'de, D>(deserializer: D) -> Result<Vec<ItemAmount>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Shape {
        List(Vec<ItemAmount>),
        Table(serde_json::Map<String, serde_json::Value>),
    }

    match Option::<Shape>::deserialize(deserializer)? {
        None => Ok(Vec::new()),
        Some(Shape::List(items)) => Ok(items),
        Some(Shape::Table(map)) if map.is_empty() => Ok(Vec::new()),
        // A populated object is not an empty Lua table — it is a shape we
        // do not understand, and swallowing it would silently drop real
        // ingredients.
        Some(Shape::Table(map)) => Err(serde::de::Error::custom(format!(
            "expected an array or an empty table, got an object with keys {:?}",
            map.keys().collect::<Vec<_>>()
        ))),
    }
}

// ── OnceLock registry ─────────────────────────────────────────────────

static REGISTRY: OnceLock<HashMap<String, Recipe>> = OnceLock::new();

fn load_registry() -> HashMap<String, Recipe> {
    let json = include_str!("../data/recipes.json");
    let recipes: Vec<Recipe> = serde_json::from_str(json)
        .expect("recipes.json must be valid JSON matching the Recipe schema");
    recipes.into_iter().map(|r| (r.name.clone(), r)).collect()
}

/// The full recipe registry, loaded once from the committed data file.
pub fn registry() -> &'static HashMap<String, Recipe> {
    REGISTRY.get_or_init(load_registry)
}

/// Lookup a recipe by internal name. Returns None for unknown/modded recipes.
pub fn get(name: &str) -> Option<&'static Recipe> {
    registry().get(name)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_defaults_to_true_when_absent() {
        let r: Recipe = serde_json::from_str(
            r#"{"name":"iron-plate","category":"smelting","energy_required":3.2,
                "ingredients":[],"results":[]}"#,
        )
        .unwrap();
        assert!(r.enabled, "absent `enabled` means available, not research-locked");
    }

    #[test]
    fn explicit_enabled_false_is_respected() {
        let r: Recipe = serde_json::from_str(
            r#"{"name":"electronic-circuit","enabled":false,"ingredients":[],"results":[]}"#,
        )
        .unwrap();
        assert!(!r.enabled);
    }

    #[test]
    fn category_and_energy_defaults() {
        let r: Recipe =
            serde_json::from_str(r#"{"name":"x","ingredients":[],"results":[]}"#).unwrap();
        assert_eq!(r.category, "crafting");
        assert_eq!(r.energy_required, 0.5);
    }

    #[test]
    fn empty_lua_table_object_parses_as_empty_vec() {
        // Factorio dumps an empty Lua table as `{}`. biter-egg is a REAL
        // recipe that is genuinely ingredient-free and hits this path.
        let r: Recipe = serde_json::from_str(
            r#"{"name":"biter-egg","ingredients":{},
                "results":[{"type":"item","name":"biter-egg","amount":5}]}"#,
        )
        .unwrap();
        assert!(r.ingredients.is_empty());
        assert_eq!(r.results.len(), 1);
        assert_eq!(r.results[0].amount, 5.0);
    }

    #[test]
    fn null_ingredients_parses_as_empty_vec() {
        let r: Recipe = serde_json::from_str(
            r#"{"name":"parameter-0","ingredients":null,"results":null}"#,
        )
        .unwrap();
        assert!(r.ingredients.is_empty() && r.results.is_empty());
    }

    #[test]
    fn missing_ingredients_key_parses_as_empty_vec() {
        let r: Recipe = serde_json::from_str(r#"{"name":"parameter-0"}"#).unwrap();
        assert!(r.ingredients.is_empty() && r.results.is_empty());
    }

    #[test]
    fn populated_object_ingredients_is_an_error_not_silence() {
        let err = serde_json::from_str::<Recipe>(
            r#"{"name":"x","ingredients":{"iron-plate":2},"results":[]}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("iron-plate"), "got: {err}");
    }

    #[test]
    fn fluid_ingredients_keep_their_kind() {
        let r: Recipe = serde_json::from_str(
            r#"{"name":"x","ingredients":[{"type":"fluid","name":"water","amount":50}],
                "results":[]}"#,
        )
        .unwrap();
        assert!(matches!(r.ingredients[0].kind, ItemKind::Fluid));
    }

    #[test]
    fn round_trips_through_serde() {
        let original: Recipe = serde_json::from_str(
            r#"{"name":"iron-gear-wheel","category":"crafting","energy_required":0.5,
                "ingredients":[{"type":"item","name":"iron-plate","amount":2}],
                "results":[{"type":"item","name":"iron-gear-wheel","amount":1}],
                "enabled":true,"hidden":false}"#,
        )
        .unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let back: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
    }

    #[test]
    fn registry_loads_committed_data() {
        let r = get("iron-gear-wheel").expect("iron-gear-wheel present");
        assert_eq!(r.name, "iron-gear-wheel");
        assert!(!registry().is_empty());
    }
}
