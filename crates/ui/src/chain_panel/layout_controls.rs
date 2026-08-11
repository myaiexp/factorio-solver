// Belt/pole/inserter/topology pickers for the block generator's
// `LayoutConfig`, sourced live from the prototype registry so a game update
// picks up a new tier for free rather than needing a hardcoded list edited
// by hand.
use factorio_grid::prototype::{self, EntityPrototype};
use factorio_grid::EntityCategory;
use factorio_solver::layout::{self, Side, LONG_INSERTER_REACH};

use super::{display_name, ChainPanel};

/// Straight-run belt tiers. `EntityCategory::Belt` alone is not enough: any
/// prototype whose name merely contains "belt" and isn't classified as
/// underground/splitter lands here too (`linked-belt`), so the throughput
/// check is what actually narrows this to entities a player thinks of as a
/// tier. Sorted slowest first, so the dropdown reads as a ladder.
pub(super) fn belt_options() -> Vec<&'static EntityPrototype> {
    let mut belts: Vec<&'static EntityPrototype> = prototype::all_names()
        .into_iter()
        .filter(|n| EntityCategory::from_prototype_name(n) == EntityCategory::Belt)
        .filter_map(prototype::lookup)
        .filter(|p| p.belt_throughput.is_some())
        .collect();
    belts.sort_by(|a, b| a.belt_throughput.partial_cmp(&b.belt_throughput).unwrap());
    belts
}

/// Electric poles, smallest supply area first — the property that decides
/// how many a block needs, so it doubles as the natural reading order.
pub(super) fn pole_options() -> Vec<&'static EntityPrototype> {
    let mut poles: Vec<&'static EntityPrototype> = prototype::all_names()
        .into_iter()
        .filter_map(prototype::lookup)
        .filter(|p| p.supply_area_distance.is_some())
        .collect();
    poles.sort_by(|a, b| a.supply_area_distance.partial_cmp(&b.supply_area_distance).unwrap());
    poles
}

/// Inserters with both a pickup and an insert position — the same two facts
/// `LayoutConfig::resolve` checks before it will place one, so nothing shown
/// here can fail generation on an unknown-inserter error.
pub(super) fn inserter_options() -> Vec<&'static EntityPrototype> {
    let mut inserters: Vec<&'static EntityPrototype> = prototype::all_names()
        .into_iter()
        .filter(|n| EntityCategory::from_prototype_name(n) == EntityCategory::Inserter)
        .filter_map(prototype::lookup)
        .filter(|p| p.pickup_position.is_some() && p.insert_position.is_some())
        .collect();
    inserters.sort_by(|a, b| a.name.cmp(&b.name));
    inserters
}

/// Inserters that can reach the outer belt of a topology's two-belt side —
/// the same reach check `LayoutConfig::resolve` runs on `long_inserter`
/// before it will place one (`layout::reach(p) >= LONG_INSERTER_REACH`), so
/// nothing offered here can fail generation with `LongInserterUnknown`.
pub(super) fn long_inserter_options() -> Vec<&'static EntityPrototype> {
    let mut inserters: Vec<&'static EntityPrototype> = prototype::all_names()
        .into_iter()
        .filter(|n| EntityCategory::from_prototype_name(n) == EntityCategory::Inserter)
        .filter_map(prototype::lookup)
        .filter(|p| p.pickup_position.is_some() && p.insert_position.is_some())
        .filter(|p| layout::reach(p) >= LONG_INSERTER_REACH)
        .collect();
    inserters.sort_by(|a, b| a.name.cmp(&b.name));
    inserters
}

/// One labelled dropdown: shows the display name, stores the internal name
/// (`LayoutConfig` takes internal names, not what's shown on screen).
fn combo(ui: &mut egui::Ui, id: &str, label: &str, current: &mut String, options: &[&'static EntityPrototype]) {
    ui.label(label);
    let current_label = prototype::lookup(current)
        .map(|p| display_name(&p.name, &p.display_name).to_string())
        .unwrap_or_else(|| current.clone());
    egui::ComboBox::from_id_salt(id).selected_text(current_label).show_ui(ui, |ui| {
        for proto in options {
            let entry_label = display_name(&proto.name, &proto.display_name).to_string();
            ui.selectable_value(current, proto.name.clone(), entry_label);
        }
    });
}

/// Two 1-or-2 buttons and a label — `CellTopology::spine_belts`/`edge_belts`
/// have no other valid values (`CellTopology::validate` rejects anything
/// else), so this is a closed choice rather than a `DragValue`.
fn belt_count(ui: &mut egui::Ui, label: &str, value: &mut u8) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.selectable_value(value, 1, "1");
        ui.selectable_value(value, 2, "2");
    });
}

/// Which side of a cell carries ingredients; the other always carries
/// products (`CellTopology::ingredients_on`).
fn ingredients_on(ui: &mut egui::Ui, value: &mut Side) {
    ui.horizontal(|ui| {
        ui.label("Ingredients on");
        ui.selectable_value(value, Side::Spine, "Spine");
        ui.selectable_value(value, Side::Edge, "Edge");
    });
}

/// A checkbox gating a `DragValue` for `CellTopology::target_width`.
/// Unchecked is `None`; the last number entered survives being unchecked
/// (kept in egui's own per-widget memory, not a `ChainPanel` field — the
/// panel's own state is only ever `Option<u32>`, matching `CellTopology`
/// exactly) so toggling the box off and back on does not reset it.
fn target_width(ui: &mut egui::Ui, value: &mut Option<u32>) {
    let remember_id = egui::Id::new("layout_target_width_last");
    ui.horizontal(|ui| {
        let mut enabled = value.is_some();
        if ui.checkbox(&mut enabled, "Target width").changed() {
            *value = if enabled {
                Some(ui.data(|d| d.get_temp::<u32>(remember_id)).unwrap_or(60))
            } else {
                None
            };
        }
        if let Some(width) = value {
            ui.add(egui::DragValue::new(width).range(20..=400));
            ui.data_mut(|d| d.insert_temp(remember_id, *width));
        }
    });
}

/// The block generator's own controls: belt tier, pole, inserter, long
/// inserter, and the columnar cell topology (belt counts per side, which
/// side carries ingredients, and the optional band-width cap).
pub(super) fn show(panel: &mut ChainPanel, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Block generator").strong());
    combo(ui, "layout_belt_tier", "Belt tier", &mut panel.layout_belt, &belt_options());
    combo(ui, "layout_pole", "Pole", &mut panel.layout_pole, &pole_options());
    combo(ui, "layout_inserter", "Inserter", &mut panel.layout_inserter, &inserter_options());
    combo(
        ui,
        "layout_long_inserter",
        "Long inserter",
        &mut panel.layout_long_inserter,
        &long_inserter_options(),
    );

    ui.label(egui::RichText::new("Cell topology").strong());
    belt_count(ui, "Spine belts", &mut panel.layout_topology.spine_belts);
    belt_count(ui, "Edge belts", &mut panel.layout_topology.edge_belts);
    ingredients_on(ui, &mut panel.layout_topology.ingredients_on);
    target_width(ui, &mut panel.layout_topology.target_width);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn belt_options_are_sorted_slowest_first_and_exclude_non_belts() {
        let names: Vec<&str> = belt_options().iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"transport-belt"));
        assert!(names.contains(&"turbo-transport-belt"));
        assert!(!names.contains(&"underground-belt"), "underground belts are a different tier");
        assert!(!names.contains(&"fast-splitter"), "splitters are not a belt tier");

        let throughputs: Vec<f64> =
            belt_options().iter().map(|p| p.belt_throughput.unwrap()).collect();
        let mut sorted = throughputs.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(throughputs, sorted, "must be slowest-first");
    }

    #[test]
    fn pole_options_are_sorted_by_supply_area_ascending() {
        let names: Vec<&str> = pole_options().iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"medium-electric-pole"));
        assert!(names.contains(&"substation"));

        let areas: Vec<f64> =
            pole_options().iter().map(|p| p.supply_area_distance.unwrap()).collect();
        let mut sorted = areas.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(areas, sorted);
    }

    #[test]
    fn inserter_options_exclude_belts_and_are_sorted_by_name() {
        let names: Vec<&str> = inserter_options().iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"inserter"));
        assert!(names.contains(&"fast-inserter"));
        assert!(!names.contains(&"transport-belt"));

        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    #[test]
    fn long_inserter_options_are_reach_filtered_not_name_matched() {
        let names: Vec<&str> = long_inserter_options().iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"long-handed-inserter"));
        assert!(!names.contains(&"inserter"), "one-tile reach must not qualify");
        assert!(!names.contains(&"fast-inserter"), "one-tile reach must not qualify");

        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }
}
