// Tests for recipe parsing defaults, probabilities, serde round trips and the committed registry.
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
fn absent_probability_is_certain() {
    let r: Recipe = serde_json::from_str(
        r#"{"name":"x","results":[{"type":"item","name":"iron-plate","amount":2}]}"#,
    )
    .unwrap();
    assert_eq!(r.results[0].probability, 1.0);
    assert_eq!(r.results[0].effective_amount(), 2.0);
}

#[test]
fn effective_amount_weights_by_probability() {
    // uranium-processing's entire 235/238 split lives in `probability`;
    // both results declare `amount: 1`.
    let r: Recipe = serde_json::from_str(
        r#"{"name":"uranium-processing","results":[
             {"type":"item","name":"uranium-235","amount":1,"probability":0.007},
             {"type":"item","name":"uranium-238","amount":1,"probability":0.993}]}"#,
    )
    .unwrap();
    assert_eq!(r.results[0].amount, r.results[1].amount, "amount alone cannot tell them apart");
    assert!((r.results[0].effective_amount() - 0.007).abs() < 1e-12);
    assert!((r.results[1].effective_amount() - 0.993).abs() < 1e-12);
}

#[test]
fn certain_probability_is_omitted_from_output() {
    // Keeps the regenerated data file to the 103 recipes that need it.
    let r: Recipe =
        serde_json::from_str(r#"{"name":"x","results":[{"type":"item","name":"y","amount":1}]}"#)
            .unwrap();
    assert!(!serde_json::to_string(&r).unwrap().contains("probability"));
}

#[test]
fn main_product_defaults_to_none_and_round_trips() {
    let bare: Recipe = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
    assert_eq!(bare.main_product, None);
    assert!(!serde_json::to_string(&bare).unwrap().contains("main_product"));

    let declared: Recipe =
        serde_json::from_str(r#"{"name":"molten-iron","main_product":"molten-iron"}"#).unwrap();
    assert_eq!(declared.main_product.as_deref(), Some("molten-iron"));
}

#[test]
fn registry_carries_probability_and_main_product() {
    let u = get("uranium-processing").expect("uranium-processing present");
    let u235 = u.results.iter().find(|r| r.name == "uranium-235").unwrap();
    assert!((u235.probability - 0.007).abs() < 1e-9, "got {}", u235.probability);
    assert_eq!(get("molten-iron").unwrap().main_product.as_deref(), Some("molten-iron"));
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
