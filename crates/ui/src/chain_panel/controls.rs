// Input controls for the chain panel: product picker, rate, bus contents,
// and machine policy. Each function owns one section of `ChainPanel::ui`.
use factorio_grid::prototype;
use factorio_solver::recipe;

use super::logic::{belt_tier_names, crafting_machine_names, filtered_recipes, recipe_product_item};
use super::{display_name, ChainPanel, MachineChoice, RateUnit};

/// Product text field plus a filtered, scrollable recipe list. Typing
/// filters the list; clicking an entry sets `product` to that recipe's
/// output item.
pub(super) fn product_picker(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label("Product");
    ui.text_edit_singleline(&mut panel.product);
    ui.checkbox(&mut panel.show_hidden_recycling, "Show hidden/recycling recipes");

    let registry = recipe::registry();
    let matches = filtered_recipes(registry.values(), &panel.product, panel.show_hidden_recycling);

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
