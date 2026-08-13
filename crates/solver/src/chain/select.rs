// Selection layer: which recipe makes an item, and which machine crafts a recipe.
use std::collections::{BTreeSet, HashMap};

use factorio_grid::prototype::{self, EntityPrototype};

use crate::availability::{self, Availability};
use crate::chain::{ChainError, MachineFallback, MachinePolicy};
use crate::recipe::{self, Recipe};
use crate::tech;

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

/// The subset of `candidates_for` that `availability` allows — the gated
/// filter applied AFTER the existing hidden/recycling/main-product ones.
///
/// Deliberately does not replace `candidates_for`: a caller that needs to
/// know "is this genuinely a raw resource" (no recipe at all, ever) must ask
/// the ungated function, or a locked intermediate would be indistinguishable
/// from ore. See `chain::solve`'s raw-resource split for why that matters.
pub fn available_candidates_for(item: &str, availability: &Availability) -> Vec<&'static Recipe> {
    candidates_for(item).into_iter().filter(|r| availability.allows(r)).collect()
}

/// Technology names that would unlock some candidate recipe for `item`,
/// sorted and deduped. May be empty — plenty of locked recipes are behind no
/// technology at all.
///
/// `pub(super)` rather than fully private: `chain::solve` needs the same
/// explanation for its own raw-resource-vs-locked split, and duplicating
/// this scan there would risk the two explanations drifting apart.
pub(super) fn unlockers_of(candidates: &[&'static Recipe]) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for r in candidates {
        names.extend(tech::unlockers_for(&r.name).into_iter().map(str::to_string));
    }
    names.into_iter().collect()
}

/// Pick the one recipe that produces `item`: an override wins outright,
/// otherwise there must be exactly one candidate `availability` allows.
/// Never guesses between several — that is always `AmbiguousRecipe`.
pub fn select_recipe(
    item: &str,
    overrides: &HashMap<String, String>,
    availability: &Availability,
) -> Result<&'static Recipe, ChainError> {
    // An override is an explicit instruction from the user, not a search
    // result — it already bypasses the hidden/recycling/main-product
    // filters, and availability is no different: naming a locked recipe here
    // means they meant it. Do not "fix" this into an availability check.
    if let Some(recipe_name) = overrides.get(item) {
        return recipe::get(recipe_name).ok_or_else(|| ChainError::UnknownItem(recipe_name.clone()));
    }

    let mut available = available_candidates_for(item, availability);
    match available.len() {
        1 => Ok(available.remove(0)),
        n if n > 1 => Err(ChainError::AmbiguousRecipe {
            item: item.to_string(),
            candidates: available.into_iter().map(|r| r.name.clone()).collect(),
        }),
        _ => {
            // Zero available candidates splits two ways: genuinely no recipe
            // (unchanged `UnreachableBoundary`) vs. a recipe this player just
            // cannot build yet (`NotUnlocked`, naming what would unlock it).
            let candidates = candidates_for(item);
            if candidates.is_empty() {
                Err(ChainError::UnreachableBoundary { item: item.to_string() })
            } else {
                Err(ChainError::NotUnlocked {
                    item: item.to_string(),
                    unlocked_by: unlockers_of(&candidates),
                })
            }
        }
    }
}

/// True if `(name, speed)` should replace `current` under the "faster wins,
/// ties broken by lower name" rule. Factored out so the gated and ungated
/// searches in `select_machine` share one tie-break instead of two copies
/// that could quietly diverge.
fn beats(name: &str, speed: f64, current: Option<&'static EntityPrototype>) -> bool {
    match current {
        None => true,
        Some(cur) => {
            let cur_speed = cur.crafting_speed.unwrap_or(0.0);
            speed > cur_speed || (speed == cur_speed && name < cur.name.as_str())
        }
    }
}

/// Pick the machine that crafts `recipe`'s category, honouring `policy` and
/// `availability`.
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
        let proto = prototype::lookup(machine)
            .ok_or_else(|| ChainError::UnknownMachine { machine: machine.clone() })?;
        // A machine that cannot craft the category is fatal regardless of
        // research, so that check stays first. Only once the category is
        // right do we ask whether the player can actually build it —
        // substituting a different machine here would silently break the
        // caller's explicit "build it all in X" instruction.
        if !proto.crafting_categories.iter().any(|c| c == category) {
            return Err(ChainError::NoMachineForCategory {
                category: category.clone(),
                recipe: recipe.name.clone(),
            });
        }
        return if availability.allows_machine(machine) {
            Ok(proto)
        } else {
            Err(ChainError::MachineNotUnlocked {
                machine: machine.clone(),
                unlocked_by: availability::machine_unlockers(machine),
            })
        };
    }

    // No preference and no named fallback: search for the fastest prototype
    // covering the category, tracking the gated and ungated bests in the
    // same pass rather than scanning twice. `all_names()` iterates a
    // HashMap, so ties are broken by name (via `beats`) to keep both results
    // deterministic.
    let mut best_available: Option<&'static EntityPrototype> = None;
    let mut best_overall: Option<&'static EntityPrototype> = None;
    for name in prototype::all_names() {
        let Some(proto) = prototype::lookup(name) else { continue };
        if !proto.crafting_categories.iter().any(|c| c == category) {
            continue;
        }
        let Some(speed) = proto.crafting_speed else { continue };

        if beats(&proto.name, speed, best_overall) {
            best_overall = Some(proto);
        }
        if availability.allows_machine(&proto.name) && beats(&proto.name, speed, best_available) {
            best_available = Some(proto);
        }
    }

    if let Some(proto) = best_available {
        return Ok(proto);
    }

    // Nothing available covers the category. If the ungated search would
    // have found something, the honest error names the lock
    // (`MachineNotUnlocked`) rather than `NoMachineForCategory`, whose
    // message points at the machine policy — the wrong control when the
    // real remedy is research.
    if let Some(proto) = best_overall {
        return Err(ChainError::MachineNotUnlocked {
            machine: proto.name.clone(),
            unlocked_by: availability::machine_unlockers(&proto.name),
        });
    }

    Err(ChainError::NoMachineForCategory { category: category.clone(), recipe: recipe.name.clone() })
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
