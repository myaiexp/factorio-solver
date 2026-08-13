// Rate solver: turns a ChainGoal into a topologically ordered ProductionPlan.
//
// Demand propagates backward from the goal to the bus through a work queue,
// netting each recipe's own item against itself before dividing (see
// `net_yield`) — the trick that lets a multi-output recipe
// (uranium-processing) and a self-consuming one (kovarex) resolve in one
// pass each, where a tree recursion could express neither.
use std::collections::{HashMap, HashSet, VecDeque};

use crate::chain::select::{
    available_candidates_for, candidates_for, select_machine, select_recipe, unlockers_of,
};
use crate::chain::{ChainError, ChainGoal, ItemRate, ProductionPlan, ProductionStep};
use crate::recipe::{ItemKind, Recipe};

/// Demand-propagation passes before giving up.
///
/// An item is re-popped once per consuming path rather than once per item,
/// so this counts paths through the chain, not nodes. Measured against the
/// whole 2.0.77 registry that peaks at 63 pops (stack-inserter, mech-armor)
/// even with every ambiguous item overridden, so the cap is ~150x the real
/// worst case and cannot fire on a solvable chain.
///
/// What it does catch is a *mutual* cycle across two recipes — U-235 via
/// kovarex, whose U-238 comes from reprocessing spent fuel made from U-235.
/// A self-cycle is netted exactly by `net_yield` and never reaches here, but
/// a cross-recipe loop has no such closed form and either converges
/// geometrically or diverges. Hitting the cap is reported as
/// `DidNotConverge`, never as a partial plan.
const MAX_ITERATIONS: u32 = 10_000;
/// Below this, a rate is "zero" — floating rounding from repeated
/// multiply/divide, not a real remaining demand or byproduct.
const EPSILON: f64 = 1e-9;

pub fn solve(goal: &ChainGoal) -> Result<ProductionPlan, ChainError> {
    let target_rate = goal.rate.per_sec()?;

    // Already on the bus: a legitimate answer, not an error.
    if goal.available.contains(&goal.product) {
        return Ok(ProductionPlan {
            steps: Vec::new(),
            inputs: vec![ItemRate::new(&goal.product, target_rate)],
            byproducts: Vec::new(),
            warnings: Vec::new(),
        });
    }

    // The *goal* is held to a stricter bar than every intermediate below it:
    // both "no recipe" and "a recipe this player cannot build" are refusals
    // here, never a bus input. Ungated first, so a locked goal keeps its remedy.
    let goal_candidates = candidates_for(&goal.product);
    if goal_candidates.is_empty() {
        return Err(ChainError::UnreachableBoundary { item: goal.product.clone() });
    }
    if locked_without_an_override(goal, &goal.product) {
        return Err(ChainError::RecipeLocked {
            item: goal.product.clone(),
            unlocked_by: unlockers_of(&goal_candidates),
            recipes: goal_candidates.iter().map(|r| r.name.clone()).collect(),
        });
    }

    // Produced-but-unconsumed rate per item, e.g. kovarex's spare U-238.
    let mut surplus: HashMap<String, f64> = HashMap::new();
    // Total crafts/s accumulated per recipe, keyed by the recipe's own
    // `'static` name string so this doesn't need `Recipe: Hash + Eq`.
    let mut crafts: HashMap<&'static str, (&'static Recipe, f64)> = HashMap::new();
    let mut inputs: HashMap<String, f64> = HashMap::new();

    let mut queue: VecDeque<(String, f64)> = VecDeque::new();
    queue.push_back((goal.product.clone(), target_rate));

    let mut iterations = 0u32;
    while let Some((item, mut demand)) = queue.pop_front() {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return Err(ChainError::DidNotConverge { product: goal.product.clone(), iterations });
        }
        if demand.abs() < EPSILON {
            continue;
        }

        if goal.available.contains(&item) {
            *inputs.entry(item).or_insert(0.0) += demand;
            continue;
        }

        // Satisfy from surplus (an earlier step's over-production) first.
        if let Some(&have) = surplus.get(&item)
            && have > 0.0
        {
            let taken = have.min(demand);
            *surplus.entry(item.clone()).or_insert(0.0) -= taken;
            demand -= taken;
            if demand.abs() < EPSILON {
                continue;
            }
        }

        // "No recipe" and "no recipe you can build" stay distinguishable: a
        // truly empty candidate set is ore, full stop — ungated, so a locked
        // intermediate can never masquerade as a bus requirement.
        let candidates = candidates_for(&item);
        if candidates.is_empty() {
            *inputs.entry(item.clone()).or_insert(0.0) += demand;
            continue;
        }
        // Craftable, but not by this player — never a bus input, or the plan
        // looks fine and is not.
        if locked_without_an_override(goal, &item) {
            return Err(ChainError::RecipeLocked {
                item: item.clone(),
                unlocked_by: unlockers_of(&candidates),
                recipes: candidates.iter().map(|r| r.name.clone()).collect(),
            });
        }

        let recipe = select_recipe(&item, &goal.recipe_overrides, &goal.availability)?;

        // A fluid the player hasn't declared available can't be routed by
        // this solver (no pipes here) — the boundary must stop before it.
        for ingredient in &recipe.ingredients {
            if ingredient.kind == ItemKind::Fluid && !goal.available.contains(&ingredient.name) {
                return Err(ChainError::FluidIngredient {
                    recipe: recipe.name.clone(),
                    fluid: ingredient.name.clone(),
                });
            }
        }

        let net = net_yield(recipe, &item);
        if net <= EPSILON {
            // Consumes at least as much of `item` as it makes: dividing by
            // `net` would give a negative/infinite craft count, so this is
            // reported immediately rather than divided toward.
            return Err(ChainError::DidNotConverge { product: goal.product.clone(), iterations });
        }
        let n = demand / net;

        crafts.entry(recipe.name.as_str()).or_insert((recipe, 0.0)).1 += n;

        // Order matters: credit every *other* result to surplus before
        // queuing ingredient demand, so a recipe that both makes and needs
        // an item (kovarex's U-238: 5 in, 2 out) nets correctly once its
        // own later demand for that item is popped.
        for result in &recipe.results {
            if result.name == item {
                continue; // already netted into `net_yield` above
            }
            *surplus.entry(result.name.clone()).or_insert(0.0) += n * result.effective_amount();
        }

        for ingredient in &recipe.ingredients {
            if ingredient.name == item {
                continue; // this recipe's own demand for `item`, already netted
            }
            queue.push_back((ingredient.name.clone(), n * ingredient.amount));
        }
    }

    let steps = build_steps(goal, &crafts)?;
    let steps = topologically_order(steps);

    let mut plan_inputs: Vec<ItemRate> = inputs
        .into_iter()
        .filter(|(_, rate)| rate.abs() > EPSILON)
        .map(|(item, per_sec)| ItemRate::new(&item, per_sec))
        .collect();
    plan_inputs.sort_by(|a, b| a.item.cmp(&b.item));

    let mut byproducts: Vec<ItemRate> = surplus
        .into_iter()
        .filter(|(_, rate)| *rate > EPSILON)
        .map(|(item, per_sec)| ItemRate::new(&item, per_sec))
        .collect();
    byproducts.sort_by(|a, b| a.item.cmp(&b.item));

    let warnings = byproducts
        .iter()
        .map(|b| format!("{:.4}/s of '{}' has no consumer in this plan", b.per_sec, b.item))
        .collect();

    Ok(ProductionPlan { steps, inputs: plan_inputs, byproducts, warnings })
}

/// Whether `item` is craftable in principle but not under `goal.availability`
/// — the case that must be refused rather than billed to the bus.
///
/// An override for `item` suppresses it outright, because `select_recipe`
/// honours an override without consulting availability at all: an override is
/// an explicit instruction, and naming a locked recipe means the user meant
/// it. Without this check the two disagreed, and `solve` won the race by
/// refusing before `select_recipe` was ever reached — so an override that
/// worked when called directly failed inside a plan.
fn locked_without_an_override(goal: &ChainGoal, item: &str) -> bool {
    !goal.recipe_overrides.contains_key(item)
        && available_candidates_for(item, &goal.availability).is_empty()
}

/// Expected units of `item` produced per craft, minus expected units
/// consumed. Results use `effective_amount()` (amount × probability), never
/// bare `amount` — probability is where a multi-output split like
/// uranium-processing's 0.007/0.993 lives. This subtraction is what makes a
/// self-consuming recipe (kovarex nets 41 − 40 = 1 U-235/craft) terminate
/// in one division instead of an iterative search.
fn net_yield(recipe: &Recipe, item: &str) -> f64 {
    let produced: f64 =
        recipe.results.iter().filter(|r| r.name == item).map(|r| r.effective_amount()).sum();
    let consumed: f64 =
        recipe.ingredients.iter().filter(|i| i.name == item).map(|i| i.amount).sum();
    produced - consumed
}

/// One `ProductionStep` per recipe actually used, in no particular order —
/// `topologically_order` fixes that up afterward.
fn build_steps(
    goal: &ChainGoal,
    crafts: &HashMap<&'static str, (&'static Recipe, f64)>,
) -> Result<Vec<ProductionStep>, ChainError> {
    let mut steps = Vec::with_capacity(crafts.len());
    for (recipe, crafts_per_sec) in crafts.values().copied() {
        let machine = select_machine(recipe, &goal.machines, &goal.availability)?;
        let per_machine_rate = machine.crafting_speed.unwrap_or(1.0) / recipe.energy_required;
        let exact_count = crafts_per_sec / per_machine_rate;
        let inputs = recipe
            .ingredients
            .iter()
            .map(|i| ItemRate::new(&i.name, crafts_per_sec * i.amount))
            .collect();
        let outputs = recipe
            .results
            .iter()
            .map(|r| ItemRate::new(&r.name, crafts_per_sec * r.effective_amount()))
            .collect();

        steps.push(ProductionStep {
            recipe,
            machine,
            exact_count,
            machines_needed: exact_count.ceil() as u32,
            crafts_per_sec,
            inputs,
            outputs,
        });
    }
    Ok(steps)
}

/// Kahn's algorithm over the producer -> consumer relation between steps,
/// tie-broken by recipe name so the order is stable despite HashMaps
/// throughout. Self-edges (a recipe that both makes and needs the same
/// item) are never created — the scan below only links *different* steps —
/// which is what keeps a self-consuming recipe like kovarex from
/// deadlocking the sort.
fn topologically_order(steps: Vec<ProductionStep>) -> Vec<ProductionStep> {
    let n = steps.len();
    let mut producers: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, step) in steps.iter().enumerate() {
        for result in &step.recipe.results {
            producers.entry(result.name.as_str()).or_default().push(i);
        }
    }
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut in_degree = vec![0usize; n];
    let mut seen_edges: HashSet<(usize, usize)> = HashSet::new();
    for (consumer, step) in steps.iter().enumerate() {
        for ingredient in &step.recipe.ingredients {
            let Some(producer_indices) = producers.get(ingredient.name.as_str()) else { continue };
            for &producer in producer_indices {
                if producer == consumer {
                    continue; // self-edge: this recipe already nets its own item
                }
                if seen_edges.insert((producer, consumer)) {
                    adjacency[producer].push(consumer);
                    in_degree[consumer] += 1;
                }
            }
        }
    }

    let mut ready: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while !ready.is_empty() {
        let pick = ready
            .iter()
            .enumerate()
            .min_by(|&(_, a), &(_, b)| steps[*a].recipe.name.cmp(&steps[*b].recipe.name))
            .map(|(pos, _)| pos)
            .unwrap_or(0);
        let i = ready.swap_remove(pick);
        order.push(i);
        for &next in &adjacency[i] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                ready.push(next);
            }
        }
    }

    // A demand walk toward the bus shouldn't produce a true cross-step
    // cycle, but appending any leftover deterministically is safer than
    // panicking or silently dropping steps if that assumption is ever wrong.
    if order.len() < n {
        let in_order: HashSet<usize> = order.iter().copied().collect();
        let mut leftover: Vec<usize> = (0..n).filter(|i| !in_order.contains(i)).collect();
        leftover.sort_by(|&a, &b| steps[a].recipe.name.cmp(&steps[b].recipe.name));
        order.extend(leftover);
    }

    let mut steps: Vec<Option<ProductionStep>> = steps.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| steps[i].take()).collect()
}

#[cfg(test)]
#[path = "solve_tests.rs"]
mod tests;
