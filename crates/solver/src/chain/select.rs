// Selection layer: which recipe makes an item, and which machine crafts a recipe.
use std::collections::HashMap;

use factorio_grid::prototype::{self, EntityPrototype};

use crate::chain::{Availability, ChainError, MachineFallback, MachinePolicy};
use crate::recipe::{self, Recipe};

/// Every recipe that can be chosen to produce `item`.
///
/// Cheap and pure by design: it runs once per item during chain resolution,
/// so it just walks the registry rather than maintaining a cache that could
/// go stale relative to it.
pub fn candidates_for(item: &str) -> Vec<&'static Recipe> {
    let mut out: Vec<&'static Recipe> = recipe::registry()
        .values()
        .filter(|r| r.results.iter().any(|res| res.name == item))
        .filter(|r| !r.hidden)
        // `starts_with`, not `==`: "recycling" (310 recipes) is not the only
        // recycling category. `scrap-recycling` is
        // "recycling-or-hand-crafting" and outputs 10+ common items
        // (copper-cable, iron-plate, ...); an exact-equality filter lets it
        // through and makes almost every common item look ambiguous.
        .filter(|r| !r.category.starts_with("recycling"))
        // `main_product` only ever demotes: `Some(p)` with `p != item` proves
        // `item` is a secondary result of this recipe. It can never promote —
        // requiring `main_product == item` would drop recipes like
        // uranium-processing, which declares no main_product at all, as a
        // candidate for *either* of its own outputs.
        .filter(|r| !matches!(&r.main_product, Some(p) if p != item))
        .collect();
    // `registry()` is a HashMap; iteration order is otherwise random, which
    // would make `AmbiguousRecipe`'s candidate list differ between runs.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Pick the one recipe that produces `item`: an override wins outright,
/// otherwise there must be exactly one *unlocked* candidate. Never guesses
/// between several — that is always `AmbiguousRecipe`.
///
/// `candidates_for` stays purely structural (see its doc comment and
/// `solve.rs`'s raw-resource check) — availability is applied here, after
/// the structural list is in hand, not inside it. Filtering before counting
/// means a locked alternative can no longer make an item look ambiguous, so
/// availability sometimes *resolves* an `AmbiguousRecipe` that would
/// otherwise need a manual override.
pub fn select_recipe(
    item: &str,
    overrides: &HashMap<String, String>,
    availability: &Availability,
) -> Result<&'static Recipe, ChainError> {
    // An explicit override is a deliberate user statement and bypasses
    // candidate selection (and therefore availability) entirely.
    if let Some(recipe_name) = overrides.get(item) {
        return recipe::get(recipe_name).ok_or_else(|| ChainError::UnknownItem(recipe_name.clone()));
    }

    let candidates = candidates_for(item);
    if candidates.is_empty() {
        // No producer anywhere in the registry: the raw-resource case.
        // `solve` never reaches here for a raw item (its own empty-candidate
        // shortcut handles it first), but a caller that invokes
        // `select_recipe` directly still gets the same answer as before.
        return Err(ChainError::UnreachableBoundary { item: item.to_string() });
    }

    let mut unlocked: Vec<&'static Recipe> =
        candidates.iter().filter(|r| availability.allows_recipe(&r.name)).copied().collect();

    match unlocked.len() {
        1 => Ok(unlocked.remove(0)),
        0 => Err(ChainError::RecipeLocked {
            item: item.to_string(),
            recipes: candidates.into_iter().map(|r| r.name.clone()).collect(),
        }),
        _ => Err(ChainError::AmbiguousRecipe {
            item: item.to_string(),
            candidates: unlocked.into_iter().map(|r| r.name.clone()).collect(),
        }),
    }
}

/// Pick the machine that crafts `recipe`'s category, honouring `policy` and
/// `availability`.
///
/// A named or preferred machine that is locked is an error (`MachineLocked`)
/// rather than a downgrade — consistent with a named machine that cannot
/// craft the category at all already erroring instead of falling back. Under
/// the fastest-available fallback, a locked machine is skipped and the
/// search continues silently: that is the point of the fallback existing —
/// e.g. picking assembling-machine-2 over a locked assembling-machine-3.
pub fn select_machine(
    recipe: &Recipe,
    policy: &MachinePolicy,
    availability: &Availability,
) -> Result<&'static EntityPrototype, ChainError> {
    let category = &recipe.category;

    let named = policy.preferred.get(category).or(match &policy.fallback {
        MachineFallback::Named(m) => Some(m),
        MachineFallback::FastestAvailable => None,
    });

    if let Some(machine) = named {
        // Check order: does the name exist, then can it craft the category,
        // then is it actually craftable — each answers a different question
        // and deserves its own error rather than one check masking another.
        let proto = prototype::lookup(machine)
            .ok_or_else(|| ChainError::UnknownMachine { machine: machine.clone() })?;
        if !proto.crafting_categories.iter().any(|c| c == category) {
            return Err(ChainError::NoMachineForCategory {
                category: category.clone(),
                recipe: recipe.name.clone(),
            });
        }
        return if availability.allows_machine(&proto.name) {
            Ok(proto)
        } else {
            Err(ChainError::MachineLocked {
                machine: proto.name.clone(),
                recipe: recipe.name.clone(),
            })
        };
    }

    // No preference and no named fallback: search for the fastest *unlocked*
    // prototype covering the category. `all_names()` iterates a HashMap, so
    // ties are broken by name to keep the result deterministic.
    let mut best: Option<&'static EntityPrototype> = None;
    for name in prototype::all_names() {
        let Some(proto) = prototype::lookup(name) else { continue };
        if !proto.crafting_categories.iter().any(|c| c == category) {
            continue;
        }
        let Some(speed) = proto.crafting_speed else { continue };
        if !availability.allows_machine(&proto.name) {
            continue;
        }
        best = match best {
            None => Some(proto),
            Some(current) => {
                let current_speed = current.crafting_speed.unwrap_or(0.0);
                if speed > current_speed || (speed == current_speed && proto.name < current.name) {
                    Some(proto)
                } else {
                    Some(current)
                }
            }
        };
    }

    // Nothing craftable and unlocked: report the same error as "no machine
    // covers this category at all" rather than `MachineLocked`, which needs
    // one specific machine name to put in its field and the fallback search
    // never had one — every candidate was ruled out for the same reason, but
    // there is no single offending machine to name.
    best.ok_or_else(|| ChainError::NoMachineForCategory {
        category: category.clone(),
        recipe: recipe.name.clone(),
    })
}

#[cfg(test)]
mod tests;
