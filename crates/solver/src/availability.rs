// What the player can build: a recipe-name set, however it was obtained.
use std::collections::BTreeSet;

use crate::recipe::{self, Recipe};
use crate::tech;

pub mod from_save;

/// Which recipes the player is able to build.
///
/// Deliberately source-agnostic. The set is a set of *recipe names* rather
/// than researched technologies because that is what any source can actually
/// produce: a save file exposes per-recipe unlocked flags, while researched
/// technologies are not readable. So manual editing, a technology projection
/// and a save import are three ways of filling the same field, and none of
/// them is a rewrite of the solver.
///
/// `BTreeSet` rather than `HashSet` so any error listing recipes is
/// deterministic — the same reason `chain::select::candidates_for` sorts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Availability {
    /// No source to consult: everything except what no playthrough can reach.
    #[default]
    Everything,
    /// Recipe names known to be unlocked, from any source.
    Unlocked(BTreeSet<String>),
}

impl Availability {
    /// Whether `recipe` may be chosen.
    ///
    /// Two rules, each carrying its own reasoning:
    ///
    /// **`enabled: true` is available under `Unlocked` without being in the
    /// set.** A recipe available at game start is never taken away, so no
    /// source should have to enumerate 323 of them. A save import's set is a
    /// superset of them anyway, so OR-ing costs that source nothing while
    /// making a hand-built set possible at all.
    ///
    /// **The never-unlockable exclusion applies only to `Everything`.** Eight
    /// recipes are `enabled: false` with no technology unlocking them
    /// (loader, fast/express/turbo-loader, infinity-chest, infinity-pipe,
    /// heat-interface, pistol), so no playthrough reaches them; with no
    /// source to consult they are excluded. But under `Unlocked` the source
    /// is authoritative: if a save says a recipe is unlocked, a rule here
    /// does not overrule it.
    ///
    /// That exclusion changes no chain result, and it is worth knowing why
    /// rather than rediscovering it: all eight are also `hidden: true`, so
    /// `chain::select::candidates_for` has always dropped them. Where it does
    /// bite is [`all_available_recipe_names`], the seed the UI offers as a
    /// tickable list — without it the list would carry eight recipes no
    /// playthrough can reach, pre-ticked, inviting the user to reason about
    /// them.
    ///
    /// The exclusion is derived from the technology graph rather than a
    /// hardcoded list of those eight names, so a game update moves it for
    /// free; `tests/technology_regression.rs` pins the population so such a
    /// move fails loudly rather than silently changing what the default
    /// offers.
    pub fn allows(&self, recipe: &Recipe) -> bool {
        if recipe.enabled {
            return true;
        }
        match self {
            Availability::Everything => !tech::unlockers_for(&recipe.name).is_empty(),
            Availability::Unlocked(set) => set.contains(&recipe.name),
        }
    }

    /// [`allows`](Self::allows) by recipe *name*, for callers holding a name
    /// rather than a `&Recipe`. An unknown name is not a recipe and so is not
    /// allowed — the safer direction, since the alternative would let a typo
    /// or a retired name read as buildable.
    pub fn allows_recipe(&self, recipe: &str) -> bool {
        recipe::get(recipe).is_some_and(|r| self.allows(r))
    }

    /// Whether the machine prototype named `machine` can be obtained.
    ///
    /// Derived from recipe availability rather than kept as a second list —
    /// a machine is an item, and an item you cannot craft is a machine you
    /// cannot place. A prototype no recipe produces at all stays available:
    /// missing data is not evidence of a lock, and editor-only prototypes
    /// would otherwise be wrongly excluded.
    ///
    /// The scan walks the **raw registry**, not `candidates_for`, whose
    /// hidden/recycling/main-product filters answer a different question —
    /// "which recipe should make this item inside a chain" rather than "can
    /// this machine be obtained at all". A recycling recipe is a real way to
    /// get an item even though it is never the way to plan for one.
    pub fn allows_machine(&self, machine: &str) -> bool {
        let mut produced_by_something = false;
        for r in recipe::registry().values() {
            if !r.results.iter().any(|res| res.name == machine) {
                continue;
            }
            produced_by_something = true;
            if self.allows(r) {
                return true;
            }
        }
        !produced_by_something
    }
}

/// Technology names that would unlock some recipe producing `machine`,
/// sorted and deduped. Empty when nothing produces it, or when its producers
/// are unlocked by nothing.
///
/// Beside `allows_machine` because it walks the same raw-registry scan and
/// must stay consistent with it — this is the "why" behind a `false` from
/// that function, called only once a `MachineNotUnlocked` is already known
/// to be the outcome, never in the hot search itself.
pub fn machine_unlockers(machine: &str) -> Vec<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for r in recipe::registry().values() {
        if !r.results.iter().any(|res| res.name == machine) {
            continue;
        }
        names.extend(tech::unlockers_for(&r.name).into_iter().map(str::to_string));
    }
    names.into_iter().collect()
}

/// Every recipe name available under [`Availability::Everything`].
///
/// The seed the UI uses when switching into `Unlocked`, so the switch alone
/// changes no solve result and the first edit is a removal. Editing is
/// subtractive because the user's complaint is subtractive — "stop offering
/// me casting recipes" — and building a 300-entry set from nothing to express
/// that would be absurd.
pub fn all_available_recipe_names() -> BTreeSet<String> {
    recipe::registry()
        .values()
        .filter(|r| Availability::Everything.allows(r))
        .map(|r| r.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe_named(name: &str) -> &'static Recipe {
        recipe::get(name).unwrap_or_else(|| panic!("{name} missing from the registry"))
    }

    fn unlocked(names: &[&str]) -> Availability {
        Availability::Unlocked(names.iter().map(|n| n.to_string()).collect())
    }

    #[test]
    fn enabled_recipes_are_available_under_an_empty_unlocked_set() {
        // iron-plate is `enabled: true`. No source should have to enumerate
        // the 323 starting recipes, and a save import's set is a superset of
        // them anyway.
        assert!(unlocked(&[]).allows(recipe_named("iron-plate")));
    }

    #[test]
    fn a_locked_recipe_needs_to_be_in_the_set() {
        assert!(!unlocked(&[]).allows(recipe_named("casting-iron")));
        assert!(unlocked(&["casting-iron"]).allows(recipe_named("casting-iron")));
    }

    #[test]
    fn never_unlockable_recipes_are_excluded_under_everything() {
        // `enabled: false` with no unlocking technology, so no playthrough
        // reaches them. They are all `hidden: true` as well, so this changes
        // no chain result — what it keeps clean is the UI's seeded list,
        // which would otherwise offer eight unreachable recipes as ticked.
        for name in [
            "loader",
            "fast-loader",
            "express-loader",
            "turbo-loader",
            "infinity-chest",
            "infinity-pipe",
            "heat-interface",
            "pistol",
        ] {
            assert!(!Availability::Everything.allows(recipe_named(name)), "{name}");
        }
    }

    #[test]
    fn research_locked_recipes_stay_available_under_everything() {
        // The exclusion above must key on "nothing unlocks it", not on
        // `enabled` — 326 recipes are research-locked and all but eight are
        // legitimate goals. This is the assertion that keeps the gate opt-in
        // rather than a redefinition of the default.
        assert!(Availability::Everything.allows(recipe_named("uranium-processing")));
        assert!(Availability::Everything.allows(recipe_named("casting-iron")));
    }

    #[test]
    fn an_explicit_set_outranks_the_never_unlockable_rule() {
        // With a real source the source is authoritative; the derived rule
        // only fills in for `Everything`, where there is nothing to consult.
        assert!(unlocked(&["loader"]).allows(recipe_named("loader")));
    }

    #[test]
    fn machine_availability_follows_its_own_recipe() {
        assert!(!unlocked(&[]).allows_machine("electromagnetic-plant"));
        assert!(unlocked(&["electromagnetic-plant"]).allows_machine("electromagnetic-plant"));
        assert!(Availability::Everything.allows_machine("electromagnetic-plant"));
    }

    #[test]
    fn a_starting_machine_is_available_to_every_set() {
        // assembling-machine-1's recipe is enabled at game start, so it needs
        // no entry in any set.
        assert!(unlocked(&[]).allows_machine("assembling-machine-1"));
    }

    #[test]
    fn a_machine_no_recipe_produces_stays_available() {
        // Missing data is not evidence of a lock. 37 real prototypes — crash
        // site wreckage, editor chests, the rail curve pieces — are produced
        // by no recipe at all, and would otherwise vanish under every set.
        let produced: std::collections::HashSet<&str> = recipe::registry()
            .values()
            .flat_map(|r| r.results.iter().map(|x| x.name.as_str()))
            .collect();
        let unproduced: Vec<&str> = factorio_grid::prototype::all_names()
            .into_iter()
            .filter(|n| !produced.contains(n))
            .collect();
        assert!(!unproduced.is_empty(), "test premise: some prototypes have no recipe");
        for name in unproduced {
            assert!(unlocked(&[]).allows_machine(name), "{name}");
        }

        // …and a name that is not a prototype either, since `allows_machine`
        // is asked about whatever the policy named.
        assert!(unlocked(&[]).allows_machine("not-a-prototype-at-all"));
    }

    #[test]
    fn machine_availability_reads_the_raw_registry_not_the_chain_filters() {
        // `candidates_for` drops hidden and recycling recipes because they are
        // never how you *plan* to make an item. They are still a real way to
        // obtain one, so machine availability must not inherit that filter.
        let hidden_or_recycling: Vec<&str> = recipe::registry()
            .values()
            .filter(|r| r.results.iter().any(|x| x.name == "holmium-plate"))
            .filter(|r| r.hidden || r.category.starts_with("recycling"))
            .map(|r| r.name.as_str())
            .collect();
        assert!(!hidden_or_recycling.is_empty(), "test premise: some are filtered out of chains");
        for name in hidden_or_recycling {
            assert!(unlocked(&[name]).allows_machine("holmium-plate"), "{name}");
        }
    }

    #[test]
    fn machine_unlockers_names_the_technology_behind_the_recipe() {
        // electromagnetic-plant the machine is produced by exactly one
        // recipe of the same name, unlocked by the technology of the same
        // name — a coincidence of naming, not a rule this function relies on.
        assert_eq!(machine_unlockers("electromagnetic-plant"), vec!["electromagnetic-plant"]);
    }

    #[test]
    fn machine_unlockers_is_empty_when_nothing_produces_it() {
        assert!(machine_unlockers("not-a-prototype-at-all").is_empty());
    }

    #[test]
    fn everything_is_the_default() {
        assert_eq!(Availability::default(), Availability::Everything);
    }

    #[test]
    fn the_seed_is_exactly_what_everything_allows() {
        let seed = all_available_recipe_names();
        assert_eq!(seed.len(), recipe::registry().len() - 8, "the eight unreachable ones are out");
        assert!(seed.contains("iron-plate"));
        assert!(seed.contains("uranium-processing"), "research-locked but reachable");
        assert!(!seed.contains("loader"));

        // Seeding an `Unlocked` set from it must allow exactly what
        // `Everything` allowed — that is the whole point of the seed.
        let seeded = Availability::Unlocked(seed);
        for r in recipe::registry().values() {
            assert_eq!(seeded.allows(r), Availability::Everything.allows(r), "{}", r.name);
        }
    }
}
