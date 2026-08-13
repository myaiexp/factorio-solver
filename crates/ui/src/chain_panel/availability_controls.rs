// The availability gate's own section of the panel: a mode switch, a count
// of what the current mode allows, and — only while there is something worth
// editing — a searchable tick list. This is the whole fix for "the solver
// keeps offering me recipes I haven't unlocked": search, untick, done.
use std::collections::BTreeSet;

use factorio_solver::recipe::{self, Recipe};
use serde::{Deserialize, Serialize};

use super::logic::{display_name, filtered_recipes};
use super::ChainPanel;

/// Everything vs. an edited set. A thin wrapper around `Option<BTreeSet>`
/// would collapse "never edited" and "edited to nothing" into the same
/// state, which is exactly the distinction `ChainPanel::set_availability_mode`
/// has to preserve — so this stays its own enum rather than being inferred
/// from the set.
///
/// `pub(crate)`, not `pub(super)`: `crate::persist` is a sibling of
/// `chain_panel`, not its parent, so it needs crate-wide visibility to name
/// this type in `PanelState`. Also carries Serialize/Deserialize for the
/// same reason — persisted as part of the chain panel's saved inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum AvailabilityMode {
    Everything,
    OnlyAvailable,
}

/// What the "Untick all N matching" button calls — factored out from the
/// button's `clicked()` arm so a test can exercise the exact same removal
/// without driving a click through egui.
pub(super) fn untick_all_matching(panel: &mut ChainPanel, matches: &[&Recipe]) {
    let set = panel.available_recipes.get_or_insert_with(BTreeSet::new);
    for r in matches {
        set.remove(&r.name);
    }
}

/// The tick-all counterpart, for the same reason.
pub(super) fn tick_all_matching(panel: &mut ChainPanel, matches: &[&Recipe]) {
    let set = panel.available_recipes.get_or_insert_with(BTreeSet::new);
    for r in matches {
        set.insert(r.name.clone());
    }
}

/// The whole section: mode radio, count line, and — only in `OnlyAvailable`
/// — the search box, tick/untick buttons and the checkbox list.
pub(super) fn show(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Available recipes").strong());

    // `selectable_value` writes straight into whatever `&mut` it's given, so
    // it's aimed at a local rather than the panel field directly — the
    // seeding on entry to `OnlyAvailable` has to run through
    // `set_availability_mode`, and assigning the field here would skip it.
    let mut mode = panel.availability_mode;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut mode, AvailabilityMode::Everything, "Everything");
        ui.selectable_value(&mut mode, AvailabilityMode::OnlyAvailable, "Only what I can build");
    });
    if mode != panel.availability_mode {
        panel.set_availability_mode(mode);
    }

    let registry = recipe::registry();
    let total = registry.len();
    let availability = panel.availability();
    // Counts the whole registry, not the filtered list below — this line
    // answers "how big is the set", the list answers "which recipes can I
    // toggle right now", and those are different questions once a search is
    // typed in.
    let allowed = registry.values().filter(|r| availability.allows(r)).count();
    ui.label(format!("{allowed} of {total} recipes available"));

    if panel.availability_mode != AvailabilityMode::OnlyAvailable {
        return;
    }

    ui.text_edit_singleline(&mut panel.availability_query);

    // Same default filter as the product picker (non-hidden, non-recycling)
    // — 310 of 649 recipes are recycling and an unfiltered list is unusable.
    // The consequence is deliberate, not a gap: hidden/recycling recipes get
    // no checkbox here and so can never be unticked, which means they stay
    // available under every edited set. That's correct because
    // `candidates_for` already drops them out of chain planning regardless
    // of availability, and `allows_machine` reads the raw registry rather
    // than this filtered list — a machine obtainable only through a
    // recycling recipe must not look locked just because that recipe never
    // appears in this list.
    let matches = filtered_recipes(registry.values(), &panel.availability_query, false);

    ui.horizontal(|ui| {
        if ui.button(format!("Tick all {} matching", matches.len())).clicked() {
            tick_all_matching(panel, &matches);
        }
        if ui.button(format!("Untick all {} matching", matches.len())).clicked() {
            untick_all_matching(panel, &matches);
        }
    });

    egui::ScrollArea::vertical().max_height(200.0).id_salt("chain_availability_list").show(ui, |ui| {
        for r in &matches {
            let label = display_name(&r.name, &r.display_name);
            if r.enabled {
                // Available at game start and never taken away by any
                // source, so `Availability::allows` returns true for it
                // regardless of the set — ticking or unticking this box
                // would be a control that lies about having an effect.
                let mut always_checked = true;
                ui.add_enabled(false, egui::Checkbox::new(&mut always_checked, label));
                continue;
            }
            let mut checked = availability.allows(r);
            if ui.checkbox(&mut checked, label).changed() {
                let set = panel.available_recipes.get_or_insert_with(BTreeSet::new);
                if checked {
                    set.insert(r.name.clone());
                } else {
                    set.remove(&r.name);
                }
            }
        }
    });
}
