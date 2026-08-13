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
///
/// `candidates_for` stays purely structural (see its doc comment and
/// `solve.rs`'s raw-resource check) — availability is applied here, after
/// the structural list is in hand, not inside it. Filtering before counting
/// means a locked alternative can no longer make an item look ambiguous, so
/// availability sometimes *resolves* an `AmbiguousRecipe` that would
/// otherwise need a manual override. That is the whole point: with the
/// casting recipes out, the chemical-science chain stops asking the player to
/// arbitrate four times over recipes they cannot build.
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

    let candidates = candidates_for(item);
    if candidates.is_empty() {
        // No producer anywhere in the registry: the raw-resource case.
        // `solve` never reaches here for a raw item (its own empty-candidate
        // shortcut handles it first), but a caller that invokes
        // `select_recipe` directly still gets the same answer as before.
        return Err(ChainError::UnreachableBoundary { item: item.to_string() });
    }

    // Filtered from `candidates` rather than via `available_candidates_for`,
    // which would walk the registry a second time for a list already in hand.
    let mut available: Vec<&'static Recipe> =
        candidates.iter().copied().filter(|r| availability.allows(r)).collect();
    match available.len() {
        1 => Ok(available.remove(0)),
        n if n > 1 => Err(ChainError::AmbiguousRecipe {
            item: item.to_string(),
            candidates: available.into_iter().map(|r| r.name.clone()).collect(),
        }),
        // Craftable in principle, not by this player. Names the producers it
        // rejected *and* the research that would grant them — the first tells
        // the user what they were denied, the second what to do about it.
        _ => Err(ChainError::RecipeLocked {
            item: item.to_string(),
            unlocked_by: unlockers_of(&candidates),
            recipes: candidates.into_iter().map(|r| r.name.clone()).collect(),
        }),
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
///
/// A named or preferred machine that is locked is an error (`MachineLocked`)
/// rather than a downgrade — consistent with a named machine that cannot
/// craft the category at all already erroring instead of falling back. Under
/// the fastest-available fallback, a locked machine is skipped and the search
/// continues silently: that is the point of the fallback existing — e.g.
/// picking assembling-machine-2 over a locked assembling-machine-3.
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
            Err(ChainError::MachineLocked {
                machine: machine.clone(),
                recipe: recipe.name.clone(),
                unlocked_by: availability::machine_unlockers(machine),
            })
        };
    }

    // No preference and no named fallback: search for the fastest *available*
    // prototype covering the category, tracking the gated and ungated bests in
    // the same pass rather than scanning twice. `all_names()` iterates a
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

    // Nothing available covers the category. If the ungated search found
    // something, the honest error names that machine's lock rather than
    // `NoMachineForCategory`, whose message points at the machine policy —
    // the wrong control entirely when the real remedy is research. Tracking
    // `best_overall` alongside is what makes a name available here: every
    // candidate was ruled out for the same reason, and the fastest of them is
    // the one the player would have got.
    if let Some(proto) = best_overall {
        return Err(ChainError::MachineLocked {
            machine: proto.name.clone(),
            recipe: recipe.name.clone(),
            unlocked_by: availability::machine_unlockers(&proto.name),
        });
    }

    Err(ChainError::NoMachineForCategory { category: category.clone(), recipe: recipe.name.clone() })
}

/// Recipe selection: candidate lists, `select_recipe`, and availability.
#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
/// Machine selection: `select_machine`, its policy, and availability.
#[cfg(test)]
#[path = "select_machine_tests.rs"]
mod machine_tests;
