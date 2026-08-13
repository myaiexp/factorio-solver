// Input controls for the chain panel: product picker, rate, bus contents,
// machine policy, and the save picker. Each function owns one section of
// `ChainPanel::ui`.
use std::path::PathBuf;

use factorio_grid::prototype;
use factorio_solver::recipe::{self, Recipe};

use super::logic::{belt_tier_names, crafting_machine_names, filtered_recipes, recipe_product_item};
use super::{display_name, AvailabilityMode, ChainPanel, MachineChoice, RateUnit};

/// The product picker's list, computed the same way it's rendered — pulled
/// out so `availability_tests.rs` can assert on it directly instead of
/// scraping painted text. An unbuildable product must not be offered as a
/// goal in the first place, so this filters by `panel.availability()` on top
/// of the usual hidden/recycling/search chain.
pub(super) fn picker_matches(panel: &ChainPanel) -> Vec<&'static Recipe> {
    let registry = recipe::registry();
    let availability = panel.availability();
    filtered_recipes(registry.values(), &panel.product, panel.show_hidden_recycling)
        .into_iter()
        .filter(|r| availability.allows(r))
        .collect()
}

/// Points the solver at a save's unlocked-recipe set: a dropdown of saves
/// found in `~/.factorio/saves`, newest first, present only when that scan
/// found something (a missing `~/.factorio` — routine on a machine with no
/// Factorio install — simply leaves it out and the manual field remains); a
/// Rescan control for a save written after the panel started; a manual path
/// field plus a Load button for saves kept elsewhere; and a Clear control
/// back to unrestricted. Scanning/decoding lives in
/// `save_picker::SavePickerState` — this only wires that state to widgets.
pub(super) fn save_picker(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Save (recipe availability)").strong());

    if !panel.save_picker.entries.is_empty() {
        let current_label = panel
            .save_picker
            .selected
            .as_ref()
            .map(|selected| {
                panel
                    .save_picker
                    .entries
                    .iter()
                    .find(|e| &e.path == selected)
                    .map(|e| e.label.clone())
                    // A selection outside the scanned list (loaded via the
                    // manual field) has no dropdown label of its own — show
                    // the path instead of pretending nothing is selected.
                    .unwrap_or_else(|| selected.display().to_string())
            })
            .unwrap_or_else(|| "None".to_string());

        let mut chosen: Option<PathBuf> = None;
        egui::ComboBox::from_id_salt("chain_save_picker").selected_text(current_label).show_ui(
            ui,
            |ui| {
                for entry in &panel.save_picker.entries {
                    let is_selected = panel.save_picker.selected.as_deref() == Some(entry.path.as_path());
                    if ui.selectable_label(is_selected, &entry.label).clicked() {
                        chosen = Some(entry.path.clone());
                    }
                }
            },
        );
        // Applied after the dropdown closes, not inside it: `select` needs
        // `&mut panel.save_picker` while the loop above still borrows
        // `panel.save_picker.entries`.
        if let Some(path) = chosen {
            panel.adopt_save(&path);
        }
    }

    ui.horizontal(|ui| {
        // Short labels, deliberately kept off the text field's row below: at
        // the panel's un-resized default width (200px, `SidePanel::right`'s
        // own default) a `text_edit_singleline` claims essentially the whole
        // row on its own, which pushed a same-row Clear button past the
        // panel's clip rect — painted nowhere and clickable never, the same
        // failure mode `scroll_tests.rs` guards against vertically.
        if ui.button("Rescan").clicked() {
            panel.save_picker.rescan();
        }
        if ui.button("Clear").clicked() {
            panel.save_picker.clear();
            // Back to unrestricted, but the set itself is kept: a user who
            // imported a save and then hand-corrected it has edits worth more
            // than the import, and switching the mode radio back gets them.
            panel.availability_mode = AvailabilityMode::Everything;
        }
    });

    // Full width to itself, then its Load button on the next line — see the
    // comment above for why they aren't packed into one row.
    ui.text_edit_singleline(&mut panel.save_picker.manual_path);
    if ui.button("Load").clicked() && !panel.save_picker.manual_path.trim().is_empty() {
        let path = PathBuf::from(panel.save_picker.manual_path.trim());
        panel.adopt_save(&path);
    }

    match &panel.save_picker.status {
        Some(Ok(count)) => {
            ui.label(format!("{count} recipes unlocked"));
        }
        // Shown in full — `SaveError`'s messages are written to be read
        // verbatim (e.g. `CalibrationFailed` names the remedy), so truncating
        // here would cut off the fix along with the problem.
        Some(Err(msg)) => {
            ui.colored_label(egui::Color32::RED, msg.as_str());
        }
        None => {}
    }

    // The save moved and the reload was not taken silently — the tick list
    // holds edits that adopting it would discard, or the decode failed
    // (a half-written zip being the likely one). Either way the decision is
    // the user's, and Reload goes through the ordinary import path so a real
    // failure lands in the status line above rather than staying a flag.
    if panel.save_picker.changed_on_disk {
        ui.colored_label(egui::Color32::YELLOW, "Save changed on disk");
        if ui.button("Reload").clicked()
            && let Some(path) = panel.save_picker.selected.clone()
        {
            panel.adopt_save(&path);
        }
    }
}

/// Product text field plus a filtered, scrollable recipe list. Typing
/// filters the list; clicking an entry sets `product` to that recipe's
/// output item.
pub(super) fn product_picker(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label("Product");
    ui.text_edit_singleline(&mut panel.product);
    ui.checkbox(&mut panel.show_hidden_recycling, "Show hidden/recycling recipes");

    let matches = picker_matches(panel);

    egui::ScrollArea::vertical().max_height(150.0).id_salt("chain_product_picker").show(ui, |ui| {
        for r in matches {
            let label = display_name(&r.name, &r.display_name);
            if ui.selectable_label(false, label).clicked() {
                panel.product = recipe_product_item(r).to_string();
            }
        }
    });
}

/// Rate value plus unit selector; a belt-tier selector appears only when
/// the "belts" unit is chosen.
pub(super) fn rate_controls(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label("Rate");
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut panel.rate_value).range(0.0..=f64::MAX).speed(0.1));
        egui::ComboBox::from_id_salt("chain_rate_unit")
            .selected_text(match panel.rate_unit {
                RateUnit::PerSec => "items/sec",
                RateUnit::PerMin => "items/min",
                RateUnit::Belts => "belts",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut panel.rate_unit, RateUnit::PerSec, "items/sec");
                ui.selectable_value(&mut panel.rate_unit, RateUnit::PerMin, "items/min");
                ui.selectable_value(&mut panel.rate_unit, RateUnit::Belts, "belts");
            });
    });

    if panel.rate_unit == RateUnit::Belts {
        egui::ComboBox::from_id_salt("chain_belt_tier")
            .selected_text(panel.belt_tier.clone())
            .show_ui(ui, |ui| {
                for tier in belt_tier_names() {
                    ui.selectable_value(&mut panel.belt_tier, tier.to_string(), tier);
                }
            });
    }
}

/// The bus contents: an add box plus a removable row per item. This is the
/// field that decides how much of the chain actually gets built, so it gets
/// its own labelled section rather than blending into the rest of the form.
pub(super) fn available_list(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Available on the bus").strong());
    ui.horizontal(|ui| {
        let response = ui.text_edit_singleline(&mut panel.available_input);
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let add_clicked = ui.button("Add").clicked();
        if (add_clicked || enter_pressed) && !panel.available_input.trim().is_empty() {
            panel.available.push(panel.available_input.trim().to_string());
            panel.available_input.clear();
        }
    });

    let mut remove_index = None;
    for (i, item) in panel.available.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(item);
            if ui.small_button("x").clicked() {
                remove_index = Some(i);
            }
        });
    }
    if let Some(i) = remove_index {
        panel.available.remove(i);
    }
}

/// "Fastest available" or a named machine, restricted to prototypes that
/// can actually craft something.
pub(super) fn machine_selector(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label("Machine");
    let current_label = match &panel.machine {
        MachineChoice::Fastest => "Fastest available".to_string(),
        MachineChoice::Named(name) => prototype::lookup(name)
            .map(|p| display_name(&p.name, &p.display_name).to_string())
            .unwrap_or_else(|| name.clone()),
    };

    egui::ComboBox::from_id_salt("chain_machine_choice").selected_text(current_label).show_ui(
        ui,
        |ui| {
            ui.selectable_value(&mut panel.machine, MachineChoice::Fastest, "Fastest available");
            for name in crafting_machine_names() {
                if let Some(proto) = prototype::lookup(name) {
                    let label = display_name(&proto.name, &proto.display_name).to_string();
                    ui.selectable_value(&mut panel.machine, MachineChoice::Named(name.to_string()), label);
                }
            }
        },
    );
}
