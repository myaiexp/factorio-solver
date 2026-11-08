// Orders a plan's steps so every producer comes before the steps that consume it.
use std::collections::{HashMap, HashSet};

use crate::chain::ProductionStep;

/// Kahn's algorithm over the producer -> consumer relation between steps,
/// tie-broken by recipe name so the order is stable despite HashMaps
/// throughout. Self-edges (a recipe that both makes and needs the same
/// item) are never created — the scan below only links *different* steps —
/// which is what keeps a self-consuming recipe like kovarex from
/// deadlocking the sort.
pub(super) fn topologically_order(steps: Vec<ProductionStep>) -> Vec<ProductionStep> {
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
