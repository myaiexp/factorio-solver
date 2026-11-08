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
    /// Chance this result is actually produced, 0.0–1.0. Absent means 1.0.
    ///
    /// Load-bearing on results, not ingredients: 103 recipes declare it, and
    /// `uranium-processing` gives both outputs `amount: 1` with the entire
    /// 0.007/0.993 split living here. Any rate maths that reads `amount`
    /// alone silently reports a 1:1 ratio for it.
    #[serde(default = "default_probability", skip_serializing_if = "is_certain")]
    pub probability: f64,
}

impl ItemAmount {
    /// Expected units produced/consumed per craft — the only figure rate
    /// maths may use. See the `probability` field.
    pub fn effective_amount(&self) -> f64 {
        self.amount * self.probability
    }
}

fn default_probability() -> f64 {
    1.0
}

fn is_certain(p: &f64) -> bool {
    *p == 1.0
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
    /// The result this recipe is "for", when the game declares one.
    ///
    /// Only 13 of 659 recipes declare it and 5 of those declare it empty
    /// (Factorio's way of saying "no single main product"), which is stored
    /// as `None`. So this is a *demotion* signal only — it can prove a
    /// result is secondary, never that one is primary. Selection is built on
    /// single-result recipes instead; see `chain::select`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_product: Option<String>,
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

#[cfg(test)]
mod tests;
