// Parses a recipe's `ingredients`/`results` field into `ItemAmount`s.
use serde_json::Value;

use factorio_solver::recipe::{ItemAmount, ItemKind};

use crate::error::IngestError;

/// Reads a recipe's `ingredients`/`results` field. Factorio serialises an empty
/// Lua table as `{}` because it cannot distinguish an empty array from an
/// empty map (e.g. `biter-egg`'s ingredient-free `ingredients: {}`), so an
/// empty object is accepted alongside an array and null/absent — all yielding
/// an empty vec. A *populated* object is a shape this tool does not
/// understand and is a hard error, not a silent empty vec.
pub fn item_amounts(recipe: &str, field: &str, value: Option<&Value>) -> Result<Vec<ItemAmount>, IngestError> {
    match value.filter(|v| !v.is_null()) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items.iter().map(|item| item_amount(recipe, field, item)).collect(),
        Some(Value::Object(map)) if map.is_empty() => Ok(Vec::new()),
        Some(_) => {
            Err(IngestError::UnexpectedIngredientShape { name: recipe.to_string(), field: field.to_string() })
        }
    }
}

/// Every ingredient/result entry in the dump carries `name`, `type` and
/// `amount`, plus an optional `probability`. Remaining keys (`temperature`,
/// `ignored_by_stats`, ...) exist but are not modelled and are ignored.
fn item_amount(recipe: &str, field: &str, entry: &Value) -> Result<ItemAmount, IngestError> {
    let name = entry.get("name").and_then(Value::as_str).ok_or_else(|| IngestError::MissingRecipeField {
        name: recipe.to_string(),
        field: format!("{field}[].name"),
    })?;
    let type_str = entry.get("type").and_then(Value::as_str).ok_or_else(|| IngestError::MissingRecipeField {
        name: recipe.to_string(),
        field: format!("{field}[].type"),
    })?;
    let kind = match type_str {
        "item" => ItemKind::Item,
        "fluid" => ItemKind::Fluid,
        other => return Err(IngestError::UnknownItemType { name: recipe.to_string(), value: other.to_string() }),
    };
    let amount = entry.get("amount").and_then(Value::as_f64).ok_or_else(|| IngestError::MissingRecipeField {
        name: recipe.to_string(),
        field: format!("{field}[].amount"),
    })?;
    let probability = probability(recipe, field, entry)?;
    Ok(ItemAmount { name: name.to_string(), kind, amount, probability })
}

/// Absent or null means certain (1.0). A key that is *present* but not a
/// number is a hard error rather than a quiet fall-through to 1.0: for
/// `uranium-processing` this field is the only thing distinguishing its two
/// outputs, so defaulting past a shape change would silently turn a 0.7%
/// chance into a certainty.
fn probability(recipe: &str, field: &str, entry: &Value) -> Result<f64, IngestError> {
    let Some(raw) = entry.get("probability").filter(|v| !v.is_null()) else {
        return Ok(1.0);
    };
    let p = raw.as_f64().ok_or_else(|| IngestError::UnexpectedRecipeFieldType {
        name: recipe.to_string(),
        field: format!("{field}[].probability"),
        value: raw.to_string(),
    })?;
    if !(0.0..=1.0).contains(&p) {
        return Err(IngestError::UnexpectedRecipeFieldType {
            name: recipe.to_string(),
            field: format!("{field}[].probability"),
            value: format!("{p} (expected 0.0..=1.0)"),
        });
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn array_ingredients_are_parsed() {
        let v = json!([{"type":"item","name":"iron-plate","amount":2}]);
        let out = item_amounts("x", "ingredients", Some(&v)).unwrap();
        assert_eq!(
            out,
            vec![ItemAmount { name: "iron-plate".into(), kind: ItemKind::Item, amount: 2.0, probability: 1.0 }]
        );
    }

    #[test]
    fn absent_probability_is_certain() {
        let v = json!([{"type":"item","name":"iron-plate","amount":2}]);
        assert_eq!(item_amounts("x", "results", Some(&v)).unwrap()[0].probability, 1.0);
    }

    #[test]
    fn probability_is_read_and_weights_the_yield() {
        // uranium-processing: both results are amount 1; the whole split is here.
        let v = json!([
            {"type":"item","name":"uranium-235","amount":1,"probability":0.007},
            {"type":"item","name":"uranium-238","amount":1,"probability":0.993}
        ]);
        let out = item_amounts("uranium-processing", "results", Some(&v)).unwrap();
        assert_eq!(out[0].probability, 0.007);
        assert!((out[0].effective_amount() - 0.007).abs() < 1e-12);
        assert!((out[1].effective_amount() - 0.993).abs() < 1e-12);
    }

    #[test]
    fn null_probability_is_certain() {
        let v = json!([{"type":"item","name":"x","amount":1,"probability":null}]);
        assert_eq!(item_amounts("x", "results", Some(&v)).unwrap()[0].probability, 1.0);
    }

    #[test]
    fn wrongly_typed_probability_is_an_error_not_a_silent_certainty() {
        let v = json!([{"type":"item","name":"x","amount":1,"probability":"0.007"}]);
        let err = item_amounts("odd-recipe", "results", Some(&v)).unwrap_err();
        assert!(matches!(err, IngestError::UnexpectedRecipeFieldType { .. }));
        assert!(err.to_string().contains("odd-recipe") && err.to_string().contains("probability"));
    }

    #[test]
    fn out_of_range_probability_is_an_error() {
        let v = json!([{"type":"item","name":"x","amount":1,"probability":1.5}]);
        assert!(matches!(
            item_amounts("odd-recipe", "results", Some(&v)).unwrap_err(),
            IngestError::UnexpectedRecipeFieldType { .. }
        ));
    }

    #[test]
    fn empty_object_becomes_empty_vec() {
        let v = json!({});
        assert!(item_amounts("biter-egg", "ingredients", Some(&v)).unwrap().is_empty());
    }

    #[test]
    fn null_becomes_empty_vec() {
        let v = json!(null);
        assert!(item_amounts("x", "ingredients", Some(&v)).unwrap().is_empty());
    }

    #[test]
    fn absent_becomes_empty_vec() {
        assert!(item_amounts("x", "ingredients", None).unwrap().is_empty());
    }

    #[test]
    fn populated_object_is_an_error_naming_the_recipe() {
        let v = json!({"iron-plate": 2});
        let err = item_amounts("weird-recipe", "ingredients", Some(&v)).unwrap_err();
        assert!(matches!(err, IngestError::UnexpectedIngredientShape { .. }));
        assert!(err.to_string().contains("weird-recipe"));
    }

    #[test]
    fn fluid_type_is_recognised() {
        let v = json!([{"type":"fluid","name":"water","amount":50}]);
        let out = item_amounts("x", "ingredients", Some(&v)).unwrap();
        assert_eq!(out[0].kind, ItemKind::Fluid);
    }

    #[test]
    fn unknown_type_is_an_error_naming_the_recipe() {
        let v = json!([{"type":"virtual","name":"signal","amount":1}]);
        let err = item_amounts("odd-recipe", "ingredients", Some(&v)).unwrap_err();
        assert!(matches!(err, IngestError::UnknownItemType { .. }));
        assert!(err.to_string().contains("odd-recipe"));
    }

    #[test]
    fn missing_name_on_an_entry_is_an_error_naming_the_recipe() {
        let v = json!([{"type":"item","amount":1}]);
        let err = item_amounts("no-name-recipe", "ingredients", Some(&v)).unwrap_err();
        assert!(matches!(err, IngestError::MissingRecipeField { .. }));
        assert!(err.to_string().contains("no-name-recipe"));
    }

    #[test]
    fn missing_amount_on_an_entry_is_an_error_naming_the_recipe() {
        let v = json!([{"type":"item","name":"iron-plate"}]);
        let err = item_amounts("no-amount-recipe", "ingredients", Some(&v)).unwrap_err();
        assert!(matches!(err, IngestError::MissingRecipeField { .. }));
        assert!(err.to_string().contains("no-amount-recipe"));
    }
}
