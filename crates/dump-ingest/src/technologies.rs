// Maps one dump technology entry onto the shared `Technology` schema.
use std::collections::HashSet;

use serde_json::Value;

use factorio_solver::tech::Technology;

use crate::error::IngestError;
use crate::locale::Locale;

fn require(name: &str, field: &str, value: Option<String>) -> Result<String, IngestError> {
    value.ok_or_else(|| IngestError::MissingTechnologyField { name: name.to_string(), field: field.to_string() })
}

/// Absent or null means "take the Factorio default". A key that is *present*
/// with the wrong JSON type is a hard error rather than a quiet fall-through
/// to that same default — otherwise a shape change in a future dump would
/// look exactly like the documented absent case.
fn defaulted<'a, T>(
    name: &str,
    field: &str,
    raw: &'a Value,
    read: impl Fn(&'a Value) -> Option<T>,
    default: T,
) -> Result<T, IngestError> {
    match raw.get(field).filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(present) => read(present).ok_or_else(|| IngestError::UnexpectedTechnologyFieldType {
            name: name.to_string(),
            field: field.to_string(),
            value: present.to_string(),
        }),
    }
}

/// Accepts a JSON array or an empty object, matching `effects`/`prerequisites`.
/// Factorio dumps an empty Lua table as `{}` because it cannot tell an empty
/// array from an empty map — same trap `de_item_amounts` in `recipe.rs`
/// guards against. A populated object is a shape this tool does not
/// understand, so it falls through to `None` and `defaulted` turns that into
/// an error naming the technology and field.
fn value_array(v: &Value) -> Option<Vec<Value>> {
    match v {
        Value::Array(items) => Some(items.clone()),
        Value::Object(map) if map.is_empty() => Some(Vec::new()),
        _ => None,
    }
}

/// Preserves the dump's own order — it is the game's tech-tree order, and
/// sorting it would hide information rather than reveal any.
fn prerequisites(name: &str, raw: &Value) -> Result<Vec<String>, IngestError> {
    defaulted(name, "prerequisites", raw, value_array, Vec::new())?
        .into_iter()
        .map(|entry| {
            entry.as_str().map(str::to_string).ok_or_else(|| IngestError::UnexpectedTechnologyFieldType {
                name: name.to_string(),
                field: "prerequisites[]".to_string(),
                value: entry.to_string(),
            })
        })
        .collect()
}

/// Walks `effects[]`, keeping only `unlock-recipe` entries (the other
/// effect types — ammo/gun/turret bonuses, productivity modifiers, and so
/// on — outnumber unlocks in the real dump and are silently skipped by
/// design; see `non_unlock_effects_are_ignored`).
///
/// An unlock naming a recipe `known_recipes` does not contain is dropped,
/// not an error: the recipe filter upstream (`recipes::to_recipe`) already
/// discards `parameter: true` placeholders, so an edge pointing at one is
/// legitimately dangling rather than a data problem here.
fn unlocks(name: &str, raw: &Value, known_recipes: &HashSet<String>) -> Result<Vec<String>, IngestError> {
    let mut out = Vec::new();
    for effect in defaulted(name, "effects", raw, value_array, Vec::new())? {
        let obj = effect.as_object().ok_or_else(|| IngestError::UnexpectedTechnologyFieldType {
            name: name.to_string(),
            field: "effects[]".to_string(),
            value: effect.to_string(),
        })?;
        if obj.get("type").and_then(Value::as_str) != Some("unlock-recipe") {
            continue;
        }
        let recipe = match obj.get("recipe") {
            None => {
                return Err(IngestError::MissingTechnologyField {
                    name: name.to_string(),
                    field: "effects[].recipe".to_string(),
                });
            }
            Some(value) => value.as_str().ok_or_else(|| IngestError::UnexpectedTechnologyFieldType {
                name: name.to_string(),
                field: "effects[].recipe".to_string(),
                value: value.to_string(),
            })?,
        };
        if known_recipes.contains(recipe) {
            out.push(recipe.to_string());
        }
    }
    Ok(out)
}

/// Applies every default and shape rule above. Unlike `to_recipe`, this
/// never filters an entry out — a technology with no `unlock-recipe` effect
/// is normal (112 of 275 in the real game), not a placeholder to skip.
pub fn to_technology(
    name: &str,
    raw: &Value,
    locale: &Locale,
    known_recipes: &HashSet<String>,
) -> Result<Technology, IngestError> {
    let display_name =
        require(name, "display_name (technology-locale.json names)", locale.name(name).map(str::to_string))?;

    Ok(Technology {
        name: name.to_string(),
        display_name: Some(display_name),
        prerequisites: prerequisites(name, raw)?,
        unlocks: unlocks(name, raw, known_recipes)?,
    })
}

/// Cross-reference check that needs the whole set, so it cannot live inside
/// `to_technology` — that function is called once per entry, before its
/// siblings exist to check against. Errors on the first prerequisite naming
/// a technology absent from `technologies`: a broken graph should fail loudly
/// here, at the one point the whole set is in hand, rather than under-report
/// silently the first time something walks the tree.
pub fn validate_prerequisites(technologies: &[Technology]) -> Result<(), IngestError> {
    let known: HashSet<&str> = technologies.iter().map(|t| t.name.as_str()).collect();
    for tech in technologies {
        for prereq in &tech.prerequisites {
            if !known.contains(prereq.as_str()) {
                return Err(IngestError::UnknownPrerequisite {
                    technology: tech.name.clone(),
                    prerequisite: prereq.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn locale_with(name: &str, display: &str) -> Locale {
        Locale::from_names([(name, display)])
    }

    fn recipes(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn unlock_recipe_effects_become_unlocks() {
        let locale = locale_with("foundry", "Foundry");
        let raw = json!({"effects": [{"type":"unlock-recipe","recipe":"casting-iron"}]});
        let t = to_technology("foundry", &raw, &locale, &recipes(&["casting-iron"])).unwrap();
        assert_eq!(t.unlocks, vec!["casting-iron"]);
    }

    #[test]
    fn non_unlock_effects_are_ignored() {
        let locale = locale_with("mining-productivity-1", "Mining productivity 1");
        let raw = json!({"effects": [{"type":"mining-drill-productivity-bonus","modifier":0.1}]});
        let t = to_technology("mining-productivity-1", &raw, &locale, &recipes(&[])).unwrap();
        assert!(t.unlocks.is_empty(), "112 real technologies rely on this being allowed, not an error");
    }

    #[test]
    fn an_unlock_naming_an_unknown_recipe_is_dropped_not_an_error() {
        let locale = locale_with("steel-axe", "Steel axe");
        let raw = json!({"effects": [{"type":"unlock-recipe","recipe":"parameter-0"}]});
        let t = to_technology("steel-axe", &raw, &locale, &recipes(&[])).unwrap();
        assert!(t.unlocks.is_empty());
    }

    #[test]
    fn absent_null_and_empty_table_effects_are_all_empty() {
        let locale = locale_with("x", "X");
        for raw in [json!({}), json!({"effects": null}), json!({"effects": {}})] {
            let t = to_technology("x", &raw, &locale, &recipes(&[])).unwrap();
            assert!(t.unlocks.is_empty(), "raw: {raw}");
        }
    }

    #[test]
    fn a_populated_object_where_an_array_is_expected_is_an_error() {
        let locale = locale_with("x", "X");

        let effects_err =
            to_technology("x", &json!({"effects": {"foo": "bar"}}), &locale, &recipes(&[])).unwrap_err();
        assert!(matches!(effects_err, IngestError::UnexpectedTechnologyFieldType { .. }));
        assert!(effects_err.to_string().contains("effects"), "got: {effects_err}");

        let prereq_err =
            to_technology("x", &json!({"prerequisites": {"foo": "bar"}}), &locale, &recipes(&[])).unwrap_err();
        assert!(matches!(prereq_err, IngestError::UnexpectedTechnologyFieldType { .. }));
        assert!(prereq_err.to_string().contains("prerequisites"), "got: {prereq_err}");
    }

    #[test]
    fn an_effect_with_a_non_string_recipe_is_an_error_naming_the_technology() {
        let locale = locale_with("weird-tech", "Weird tech");
        let raw = json!({"effects": [{"type":"unlock-recipe","recipe":7}]});
        let err = to_technology("weird-tech", &raw, &locale, &recipes(&[])).unwrap_err();
        assert!(err.to_string().contains("weird-tech"), "got: {err}");
    }

    #[test]
    fn display_name_comes_from_the_locale() {
        let locale = locale_with("automation-3", "Automation 3");
        let t = to_technology("automation-3", &json!({}), &locale, &recipes(&[])).unwrap();
        assert_eq!(t.display_name.as_deref(), Some("Automation 3"));
    }

    #[test]
    fn a_missing_locale_entry_is_an_error_naming_the_technology() {
        let locale = Locale::empty();
        let err = to_technology("no-locale-tech", &json!({}), &locale, &recipes(&[])).unwrap_err();
        assert!(err.to_string().contains("no-locale-tech"), "got: {err}");
    }

    #[test]
    fn prerequisites_are_preserved_in_dump_order() {
        let locale = locale_with("foundry", "Foundry");
        let raw = json!({"prerequisites": ["tungsten-carbide", "calcite-processing"]});
        let t = to_technology("foundry", &raw, &locale, &recipes(&[])).unwrap();
        assert_eq!(t.prerequisites, vec!["tungsten-carbide", "calcite-processing"]);
    }

    fn tech_with_prereqs(name: &str, prerequisites: &[&str]) -> Technology {
        Technology {
            name: name.to_string(),
            display_name: None,
            prerequisites: prerequisites.iter().map(|s| s.to_string()).collect(),
            unlocks: Vec::new(),
        }
    }

    #[test]
    fn validate_prerequisites_errors_on_a_dangling_prerequisite() {
        let technologies = vec![tech_with_prereqs("foundry", &["calcite-processing"])];
        let err = validate_prerequisites(&technologies).unwrap_err();
        assert!(matches!(err, IngestError::UnknownPrerequisite { .. }));
        let msg = err.to_string();
        assert!(msg.contains("foundry") && msg.contains("calcite-processing"), "got: {msg}");
    }

    #[test]
    fn validate_prerequisites_passes_a_fully_resolved_graph() {
        let technologies =
            vec![tech_with_prereqs("calcite-processing", &[]), tech_with_prereqs("foundry", &["calcite-processing"])];
        assert!(validate_prerequisites(&technologies).is_ok());
    }

    #[test]
    fn validate_prerequisites_passes_a_technology_with_no_prerequisites() {
        let technologies = vec![tech_with_prereqs("automation", &[])];
        assert!(validate_prerequisites(&technologies).is_ok());
    }
}
