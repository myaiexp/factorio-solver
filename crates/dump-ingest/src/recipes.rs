// Maps one filtered dump recipe entry onto the shared `Recipe` schema.
use serde_json::Value;

use factorio_solver::recipe::Recipe;

use crate::error::IngestError;
use crate::item_amount::item_amounts;
use crate::locale::Locale;

fn require(name: &str, field: &str, value: Option<String>) -> Result<String, IngestError> {
    value.ok_or_else(|| IngestError::MissingRecipeField { name: name.to_string(), field: field.to_string() })
}

/// Absent or null means "take the Factorio default". A key that is *present*
/// with the wrong JSON type is a hard error rather than a quiet fall-through to
/// that same default — otherwise a shape change in a future dump would look
/// exactly like the documented absent case, which is precisely the failure mode
/// `enabled` is dangerous for.
fn defaulted<'a, T>(
    recipe: &str,
    field: &str,
    raw: &'a Value,
    read: impl Fn(&'a Value) -> Option<T>,
    default: T,
) -> Result<T, IngestError> {
    match raw.get(field).filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(present) => read(present).ok_or_else(|| IngestError::UnexpectedRecipeFieldType {
            name: recipe.to_string(),
            field: field.to_string(),
            value: present.to_string(),
        }),
    }
}

/// Applies every default and shape rule below. Returns `None` for
/// `parameter: true` recipes (`parameter-0`..`parameter-9` — UI signal
/// placeholders, not craftable) — a documented filter, not an error.
pub fn to_recipe(name: &str, raw: &Value, locale: &Locale) -> Result<Option<Recipe>, IngestError> {
    if defaulted(name, "parameter", raw, Value::as_bool, false)? {
        return Ok(None);
    }

    let display_name = require(name, "display_name (recipe-locale.json names)", locale.name(name).map(str::to_string))?;

    Ok(Some(Recipe {
        name: name.to_string(),
        display_name: Some(display_name),
        category: defaulted(name, "category", raw, Value::as_str, "crafting")?.to_string(),
        energy_required: defaulted(name, "energy_required", raw, Value::as_f64, 0.5)?,
        ingredients: item_amounts(name, "ingredients", raw.get("ingredients"))?,
        results: item_amounts(name, "results", raw.get("results"))?,
        // Absent means available, NOT research-locked — the opposite of
        // bool::default(). 242 of 659 recipes rely on this.
        enabled: defaulted(name, "enabled", raw, Value::as_bool, true)?,
        hidden: defaulted(name, "hidden", raw, Value::as_bool, false)?,
        // Empty string is Factorio's "this recipe has no single main
        // product" (5 of the 13 declarations), which is the same thing as
        // not declaring one at all.
        main_product: match defaulted(name, "main_product", raw, Value::as_str, "")? {
            "" => None,
            product => Some(product.to_string()),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn locale_with(name: &str, display: &str) -> Locale {
        Locale::from_names([(name, display)])
    }

    #[test]
    fn absent_enabled_becomes_true() {
        let locale = locale_with("iron-plate", "Iron plate");
        let raw = json!({"category":"smelting","energy_required":3.2,"ingredients":[],"results":[]});
        assert!(to_recipe("iron-plate", &raw, &locale).unwrap().unwrap().enabled);
    }

    #[test]
    fn explicit_enabled_false_is_preserved() {
        let locale = locale_with("electronic-circuit", "Electronic circuit");
        let raw = json!({"enabled": false, "ingredients": [], "results": []});
        assert!(!to_recipe("electronic-circuit", &raw, &locale).unwrap().unwrap().enabled);
    }

    #[test]
    fn absent_category_becomes_crafting() {
        let locale = locale_with("bulk-inserter", "Bulk inserter");
        let raw = json!({"ingredients":[],"results":[]});
        assert_eq!(to_recipe("bulk-inserter", &raw, &locale).unwrap().unwrap().category, "crafting");
    }

    #[test]
    fn absent_energy_required_becomes_half_a_second() {
        let locale = locale_with("x", "X");
        let raw = json!({"ingredients":[],"results":[]});
        assert_eq!(to_recipe("x", &raw, &locale).unwrap().unwrap().energy_required, 0.5);
    }

    #[test]
    fn object_ingredients_become_empty() {
        let locale = locale_with("biter-egg", "Biter egg");
        let raw = json!({"ingredients":{},"results":[{"type":"item","name":"biter-egg","amount":5}]});
        let r = to_recipe("biter-egg", &raw, &locale).unwrap().unwrap();
        assert!(r.ingredients.is_empty());
        assert_eq!(r.results[0].amount, 5.0);
    }

    #[test]
    fn null_and_absent_ingredients_become_empty() {
        let locale = locale_with("x", "X");
        let raw = json!({"ingredients": null, "results": null});
        let r = to_recipe("x", &raw, &locale).unwrap().unwrap();
        assert!(r.ingredients.is_empty() && r.results.is_empty());

        let raw2 = json!({});
        let r2 = to_recipe("x", &raw2, &locale).unwrap().unwrap();
        assert!(r2.ingredients.is_empty() && r2.results.is_empty());
    }

    #[test]
    fn populated_object_ingredients_is_an_error_naming_the_recipe() {
        let locale = locale_with("weird-recipe", "Weird recipe");
        let raw = json!({"ingredients": {"iron-plate": 2}, "results": []});
        let err = to_recipe("weird-recipe", &raw, &locale).unwrap_err();
        assert!(err.to_string().contains("weird-recipe"));
    }

    #[test]
    fn parameter_recipes_are_skipped() {
        let locale = Locale::empty();
        let raw = json!({"parameter": true});
        assert!(to_recipe("parameter-0", &raw, &locale).unwrap().is_none());
    }

    #[test]
    fn hidden_recipes_are_kept_with_flag() {
        let locale = locale_with("some-recycling", "Some recycling");
        let raw = json!({"hidden": true, "ingredients": [], "results": []});
        assert!(to_recipe("some-recycling", &raw, &locale).unwrap().unwrap().hidden);
    }

    #[test]
    fn fluid_ingredients_keep_their_kind() {
        let locale = locale_with("sulfur", "Sulfur");
        let raw = json!({"ingredients": [{"type":"fluid","name":"water","amount":30}], "results": []});
        let r = to_recipe("sulfur", &raw, &locale).unwrap().unwrap();
        assert_eq!(r.ingredients[0].kind, factorio_solver::recipe::ItemKind::Fluid);
    }

    #[test]
    fn unknown_item_type_is_an_error_naming_the_recipe() {
        let locale = locale_with("odd-recipe", "Odd recipe");
        let raw = json!({"ingredients": [{"type":"virtual","name":"signal","amount":1}], "results": []});
        let err = to_recipe("odd-recipe", &raw, &locale).unwrap_err();
        assert!(err.to_string().contains("odd-recipe"));
    }

    #[test]
    fn missing_locale_entry_is_an_error_naming_the_recipe() {
        let locale = Locale::empty();
        let raw = json!({"ingredients": [], "results": []});
        let err = to_recipe("no-locale-recipe", &raw, &locale).unwrap_err();
        assert!(err.to_string().contains("no-locale-recipe"));
    }

    #[test]
    fn null_valued_fields_take_the_documented_defaults() {
        let locale = locale_with("x", "X");
        let raw = json!({"enabled": null, "category": null, "energy_required": null, "hidden": null});
        let r = to_recipe("x", &raw, &locale).unwrap().unwrap();
        assert!(r.enabled);
        assert_eq!(r.category, "crafting");
        assert_eq!(r.energy_required, 0.5);
        assert!(!r.hidden);
    }

    #[test]
    fn wrongly_typed_enabled_is_an_error_not_the_default() {
        // The dangerous case: a shape change must not read as "absent", which
        // would silently make the recipe look available.
        let locale = locale_with("x", "X");
        let raw = json!({"enabled": "true", "ingredients": [], "results": []});
        let err = to_recipe("x", &raw, &locale).unwrap_err();
        assert!(matches!(err, IngestError::UnexpectedRecipeFieldType { .. }));
        assert!(err.to_string().contains("enabled"), "got: {err}");
    }

    #[test]
    fn wrongly_typed_category_and_energy_are_errors() {
        let locale = locale_with("x", "X");
        for raw in [json!({"category": 7}), json!({"energy_required": "3.2"})] {
            assert!(matches!(
                to_recipe("x", &raw, &locale).unwrap_err(),
                IngestError::UnexpectedRecipeFieldType { .. }
            ));
        }
    }

    #[test]
    fn absent_main_product_is_none() {
        let locale = locale_with("uranium-processing", "Uranium processing");
        let raw = json!({"ingredients":[],"results":[]});
        assert_eq!(to_recipe("uranium-processing", &raw, &locale).unwrap().unwrap().main_product, None);
    }

    #[test]
    fn empty_main_product_is_none_not_an_empty_string() {
        // Factorio writes "" for "no single main product". Storing that
        // verbatim would make it look like a declaration for an item named "".
        let locale = locale_with("metallic-asteroid-crushing", "Metallic asteroid crushing");
        let raw = json!({"main_product":"","ingredients":[],"results":[]});
        let r = to_recipe("metallic-asteroid-crushing", &raw, &locale).unwrap().unwrap();
        assert_eq!(r.main_product, None);
    }

    #[test]
    fn declared_main_product_is_preserved() {
        let locale = locale_with("molten-iron", "Molten iron");
        let raw = json!({"main_product":"molten-iron","ingredients":[],"results":[]});
        let r = to_recipe("molten-iron", &raw, &locale).unwrap().unwrap();
        assert_eq!(r.main_product.as_deref(), Some("molten-iron"));
    }

    #[test]
    fn wrongly_typed_main_product_is_an_error() {
        let locale = locale_with("x", "X");
        let raw = json!({"main_product": 7, "ingredients": [], "results": []});
        assert!(matches!(
            to_recipe("x", &raw, &locale).unwrap_err(),
            IngestError::UnexpectedRecipeFieldType { .. }
        ));
    }

    #[test]
    fn result_probability_survives_the_mapping() {
        let locale = locale_with("uranium-processing", "Uranium processing");
        let raw = json!({"ingredients":[{"type":"item","name":"uranium-ore","amount":10}],
                         "results":[{"type":"item","name":"uranium-235","amount":1,"probability":0.007},
                                    {"type":"item","name":"uranium-238","amount":1,"probability":0.993}]});
        let r = to_recipe("uranium-processing", &raw, &locale).unwrap().unwrap();
        assert_eq!(r.results[0].probability, 0.007);
        assert_eq!(r.results[1].probability, 0.993);
    }

    #[test]
    fn display_name_comes_from_locale() {
        let locale = locale_with("iron-gear-wheel", "Iron gear wheel");
        let raw = json!({"ingredients": [], "results": []});
        let r = to_recipe("iron-gear-wheel", &raw, &locale).unwrap().unwrap();
        assert_eq!(r.display_name.as_deref(), Some("Iron gear wheel"));
    }
}
