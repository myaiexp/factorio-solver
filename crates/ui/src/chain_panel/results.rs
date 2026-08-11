// Renders the calculator's output: the plan table on success, or an error
// message with a one-click fix when the error is an ambiguous recipe.
use factorio_solver::chain::{ChainError, ItemRate, ProductionPlan};

use super::{display_name, ChainPanel};

pub(super) fn show(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    // Cloned so the error branch is free to call back into `panel`
    // (`add_override_and_resolve`) without fighting the borrow checker over
    // `panel.result`.
    let Some(result) = panel.result.clone() else {
        ui.label("Set a product and rate, then Solve.");
        show_overrides(panel, ui);
        return;
    };

    match result {
        Ok(plan) => show_plan(ui, &plan),
        Err(err) => show_error(panel, ui, &err),
    }
}

fn show_plan(ui: &mut egui::Ui, plan: &ProductionPlan) {
    ui.heading("Steps");
    egui::Grid::new("chain_plan_steps").striped(true).show(ui, |ui| {
        ui.strong("Recipe");
        ui.strong("Machine");
        ui.strong("Machines needed");
        ui.end_row();
        for step in &plan.steps {
            ui.label(display_name(&step.recipe.name, &step.recipe.display_name));
            ui.label(display_name(&step.machine.name, &step.machine.display_name));
            // The integer is what you build; the fraction next to it is the
            // true ratio, e.g. "30 (30.00)" vs. a rounder-looking "31 (30.4)".
            ui.label(format!("{} ({:.2})", step.machines_needed, step.exact_count));
            ui.end_row();
        }
    });

    rate_section(ui, "Inputs", &plan.inputs);
    rate_section(ui, "Byproducts", &plan.byproducts);

    if !plan.warnings.is_empty() {
        ui.heading("Warnings");
        for warning in &plan.warnings {
            ui.colored_label(egui::Color32::YELLOW, warning);
        }
    }
}

fn rate_section(ui: &mut egui::Ui, title: &str, rates: &[ItemRate]) {
    ui.heading(title);
    if rates.is_empty() {
        ui.label("(none)");
        return;
    }
    for rate in rates {
        ui.label(format!("{}: {:.2}/s", rate.item, rate.per_sec));
    }
}

fn show_error(panel: &mut ChainPanel, ui: &mut egui::Ui, err: &ChainError) {
    ui.colored_label(egui::Color32::RED, err.to_string());

    // The one case the panel must offer a fix for: without this button an
    // ambiguous item (copper-cable, iron-plate, copper-plate all qualify)
    // leaves the panel with no way forward.
    if let ChainError::AmbiguousRecipe { item, candidates } = err {
        ui.label(format!("Pick a recipe for '{item}':"));
        let mut chosen: Option<String> = None;
        for candidate in candidates {
            if ui.button(candidate).clicked() {
                chosen = Some(candidate.clone());
            }
        }
        if let Some(recipe) = chosen {
            panel.add_override_and_resolve(item.clone(), recipe);
        }
    }

    show_overrides(panel, ui);
}

fn show_overrides(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    if panel.overrides.is_empty() {
        return;
    }
    ui.heading("Overrides");
    let mut remove: Option<String> = None;
    for (item, recipe) in &panel.overrides {
        ui.horizontal(|ui| {
            ui.label(format!("{item} -> {recipe}"));
            if ui.small_button("x").clicked() {
                remove = Some(item.clone());
            }
        });
    }
    if let Some(item) = remove {
        panel.overrides.remove(&item);
    }
}
