// Pure logic behind the chain panel: recipe-picker filter/search, machine and
// belt-tier listings, and the item a recipe's picker entry should select.
// None of this touches `egui::Ui`, so it is unit-tested directly.
use factorio_grid::prototype;
use factorio_solver::recipe::Recipe;

/// Human-facing name: `display_name` when present, else the internal name.
/// Shared by recipes and machine prototypes — both have this same shape.
pub fn display_name<'a>(name: &'a str, display_name: &'a Option<String>) -> &'a str {
    display_name.as_deref().unwrap_or(name)
}

/// Default picker filter: excludes hidden recipes and any recycling
/// category. `starts_with`, not `==` — `scrap-recycling`'s category is
/// "recycling-or-hand-crafting", which an exact match would miss, and 310 of
/// 649 recipes are recycling so an unfiltered list is unusable.
pub fn passes_default_filter(recipe: &Recipe) -> bool {
    !recipe.hidden && !recipe.category.starts_with("recycling")
}

/// Case-insensitive substring match of `query` against the recipe's display
/// name (falling back to its internal name).
pub fn matches_query(recipe: &Recipe, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let name = display_name(&recipe.name, &recipe.display_name);
    name.to_lowercase().contains(&query.trim().to_lowercase())
}

/// The picker's full filter chain: recycling/hidden (unless overridden) then
/// the search text, sorted by display name for a stable on-screen order.
pub fn filtered_recipes<'a, I>(recipes: I, query: &str, show_hidden_recycling: bool) -> Vec<&'a Recipe>
where
    I: IntoIterator<Item = &'a Recipe>,
{
    let mut out: Vec<&'a Recipe> = recipes
        .into_iter()
        .filter(|r| show_hidden_recycling || passes_default_filter(r))
        .filter(|r| matches_query(r, query))
        .collect();
    out.sort_by(|a, b| {
        display_name(&a.name, &a.display_name).cmp(display_name(&b.name, &b.display_name))
    });
    out
}

/// The item a recipe's picker entry sets `product` to.
///
/// The goal is an *item*, but the picker lists *recipes*, and 69 of the 329
/// pickable recipes are not named after any of their results — `bioplastic`
/// makes `plastic-bar`, `basic-oil-processing` makes `petroleum-gas`. Using
/// the recipe name for those prefills a string that is not an item at all,
/// and the solve then dead-ends on `UnreachableBoundary`.
///
/// So: the declared main product, else the sole result, else the first
/// result. The last is a prefill for a visible, editable text field — not a
/// solver decision — and beats prefilling something guaranteed to fail.
pub fn recipe_product_item(recipe: &Recipe) -> &str {
    if let Some(main) = recipe.main_product.as_deref() {
        return main;
    }
    recipe.results.first().map_or(recipe.name.as_str(), |r| r.name.as_str())
}

/// The four belt tiers, for the "belts" rate unit.
///
/// Filtered by name, not by `belt_throughput.is_some()`: 18 prototypes carry
/// a throughput because loaders, splitters and underground belts all do, and
/// offering all 18 as "belt tiers" buries the four the user means. They all
/// share their tier's rating anyway, so nothing is lost.
pub fn belt_tier_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = prototype::all_names()
        .into_iter()
        .filter(|n| n.ends_with("transport-belt"))
        .filter(|n| prototype::lookup(n).and_then(|p| p.belt_throughput).is_some())
        .collect();
    // Slowest first — the selector reads as a tier ladder rather than
    // alphabetically, where "express" would precede "fast".
    names.sort_by(|a, b| {
        let speed = |n: &str| prototype::lookup(n).and_then(|p| p.belt_throughput).unwrap_or(0.0);
        speed(a).total_cmp(&speed(b))
    });
    names
}

/// Machine prototypes that can craft something at all, for the machine
/// selector — anything with an empty `crafting_categories` can never be
/// picked by `select_machine` and would be a dead entry in the list.
pub fn crafting_machine_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = prototype::all_names()
        .into_iter()
        .filter(|n| prototype::lookup(n).is_some_and(|p| !p.crafting_categories.is_empty()))
        .collect();
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use factorio_solver::recipe;

    #[test]
    fn default_filter_excludes_recycling_and_hidden() {
        let registry = recipe::registry();
        let matches = filtered_recipes(registry.values(), "", false);
        assert!(matches.iter().all(|r| !r.hidden), "hidden recipes leaked through");
        assert!(
            matches.iter().all(|r| !r.category.starts_with("recycling")),
            "recycling recipes leaked through"
        );
        assert!(!matches.is_empty());
    }

    #[test]
    fn toggle_includes_recycling_and_hidden() {
        let registry = recipe::registry();
        let default_count = filtered_recipes(registry.values(), "", false).len();
        let all_count = filtered_recipes(registry.values(), "", true).len();
        assert!(all_count > default_count, "toggle must reveal more recipes, not fewer");
    }

    #[test]
    fn scrap_recycling_specifically_is_excluded_by_default() {
        let registry = recipe::registry();
        let matches = filtered_recipes(registry.values(), "", false);
        assert!(
            !matches.iter().any(|r| r.name == "scrap-recycling"),
            "scrap-recycling's category is 'recycling-or-hand-crafting', an exact-match \
             filter would miss it"
        );
        // ...but it does exist in the registry and does turn up once the
        // toggle is on, proving the default filter is what excluded it.
        let with_toggle = filtered_recipes(registry.values(), "", true);
        assert!(with_toggle.iter().any(|r| r.name == "scrap-recycling"));
    }

    #[test]
    fn search_matches_display_name_case_insensitively() {
        let registry = recipe::registry();
        let circuit = registry.get("electronic-circuit").expect("fixture recipe exists");
        let name = display_name(&circuit.name, &circuit.display_name);
        assert!(matches_query(circuit, &name.to_uppercase()));
        assert!(matches_query(circuit, &name.to_lowercase()));
        assert!(!matches_query(circuit, "zzz-not-a-real-query"));
    }

    #[test]
    fn empty_query_matches_everything() {
        let registry = recipe::registry();
        let circuit = registry.get("electronic-circuit").expect("fixture recipe exists");
        assert!(matches_query(circuit, ""));
        assert!(matches_query(circuit, "   "));
    }

    #[test]
    fn belt_tiers_are_the_four_belts_slowest_first() {
        // Exactly four: loaders, splitters and underground belts also carry a
        // belt_throughput and would otherwise pad this to 18 entries.
        assert_eq!(
            belt_tier_names(),
            vec![
                "transport-belt",
                "fast-transport-belt",
                "express-transport-belt",
                "turbo-transport-belt",
            ]
        );
    }

    #[test]
    fn picker_selects_an_item_even_when_the_recipe_is_not_named_after_it() {
        let registry = recipe::registry();
        // 69 pickable recipes are not named after any result. Prefilling the
        // recipe name for these dead-ends the solve on UnreachableBoundary.
        let bioplastic = registry.get("bioplastic").expect("fixture recipe exists");
        assert_eq!(recipe_product_item(bioplastic), "plastic-bar");

        // The ordinary case still resolves to itself.
        let circuit = registry.get("electronic-circuit").expect("fixture recipe exists");
        assert_eq!(recipe_product_item(circuit), "electronic-circuit");

        // A declared main_product wins over result order.
        let molten = registry.get("molten-iron").expect("fixture recipe exists");
        assert_eq!(recipe_product_item(molten), "molten-iron");
    }

    #[test]
    fn every_pickable_recipe_prefills_a_real_item() {
        // The picker must never hand the solver a string that is not an item.
        // "Is an item" here means: something at least one recipe produces.
        let registry = recipe::registry();
        let items: std::collections::HashSet<&str> =
            registry.values().flat_map(|r| r.results.iter().map(|x| x.name.as_str())).collect();
        for r in filtered_recipes(registry.values(), "", false) {
            let product = recipe_product_item(r);
            assert!(items.contains(product), "{} prefills non-item '{product}'", r.name);
        }
    }

    #[test]
    fn crafting_machines_exclude_non_crafters() {
        let names = crafting_machine_names();
        assert!(names.contains(&"assembling-machine-2"));
        assert!(!names.contains(&"transport-belt"), "belts craft nothing");
    }
}
