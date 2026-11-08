// Entity prototype schema and the dump-derived registry every crate looks prototypes up in.
use std::collections::HashMap;
use std::sync::OnceLock;

use factorio_blueprint::Direction;

// ── Fluid connection types ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FluidConnectionType {
    Input,
    Output,
    InputOutput,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FluidConnection {
    pub dx: f64,
    pub dy: f64,
    pub connection_type: FluidConnectionType,
}

// ── Energy source ────────────────────────────────────────────────────

/// What an entity runs on: the dump's `energy_source.type`, verbatim.
/// `EntityCategory` cannot carry this — a stone furnace and an electric
/// furnace are both `Furnace` — and it decides whether a placed machine
/// needs a pole or a fuel inserter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergySource {
    Electric,
    Burner,
    Heat,
    Fluid,
    Void,
}

// ── Entity prototype ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EntityPrototype {
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crafting_speed: Option<f64>,
    /// Power *consumption*, from the dump's `energy_usage`. Generators have
    /// none — their output lives in three different fields depending on
    /// prototype type and is not derived yet. Do not read this as "power
    /// produced"; the old hand-written table did, which would make a solver
    /// summing loads count a solar panel as a 60 kW draw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_kw: Option<f64>,
    #[serde(default)]
    pub module_slots: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fluid_connections: Vec<FluidConnection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub belt_throughput: Option<f64>,

    // ── Dump-derived fields ───────────────────────────────────────────
    // Every one is `default`: the registry file predates them, and a
    // hand-written fixture may omit any of them.
    /// Localised name from `entity-locale.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Mod-relative icon path, e.g. `__base__/graphics/icons/x.png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
    /// Edge length of the icon's first mip level. Absent → callers use 64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<u32>,
    /// Recipe categories this machine can craft (empty for non-crafters).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crafting_categories: Vec<String>,
    /// Underground belt / pipe span, in tiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underground_max_distance: Option<u32>,
    /// Inserter pickup offset relative to the entity centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pickup_position: Option<(f64, f64)>,
    /// Inserter drop-off offset relative to the entity centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_position: Option<(f64, f64)>,
    /// Electric pole supply-area *half-width*, in tiles: the area reaches
    /// `± this` from the pole's centre, so a medium pole's 3.5 covers 7 tiles.
    /// Not the wire reach — `maximum_wire_distance` is a different, larger
    /// number (medium pole: 3.5 supply vs 9 wire) and the two are easy to
    /// confuse into a layout that looks connected but is unpowered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply_area_distance: Option<f64>,
    /// `None` for entities the dump gives no energy source: belts, pipes,
    /// poles, chests, constant combinators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_source: Option<EnergySource>,
}

impl EntityPrototype {
    /// Connects to the electric network, so needs to sit in a pole's supply
    /// area. Includes generators and solar panels, which join the network
    /// to *supply* it.
    pub fn on_electric_network(&self) -> bool {
        self.energy_source == Some(EnergySource::Electric)
    }

    /// Runs on fuel an inserter has to bring it (stone furnace, burner
    /// inserter, biochamber's nutrients).
    pub fn burns_fuel(&self) -> bool {
        self.energy_source == Some(EnergySource::Burner)
    }

    /// The rectangle this pole energises when placed with its footprint's
    /// top-left at `top_left`, as inclusive tile bounds
    /// `(min_x, min_y, max_x, max_y)`. `None` for anything that is not a pole.
    ///
    /// Factorio powers an entity whose own footprint *overlaps* this area — it
    /// does not require containment — so callers test intersection, not
    /// enclosure.
    pub fn supply_area(&self, top_left: (i32, i32)) -> Option<(f64, f64, f64, f64)> {
        let d = self.supply_area_distance?;
        let (cx, cy) = (
            top_left.0 as f64 + self.tile_width as f64 / 2.0,
            top_left.1 as f64 + self.tile_height as f64 / 2.0,
        );
        Some((cx - d, cy - d, cx + d, cy + d))
    }
}

/// Effective (width, height) after rotation. Non-square entities swap
/// dimensions on East/West orientations.
pub fn effective_size(proto: &EntityPrototype, direction: Direction) -> (u32, u32) {
    match direction {
        Direction::East | Direction::West => (proto.tile_height, proto.tile_width),
        _ => (proto.tile_width, proto.tile_height),
    }
}

// ── OnceLock registry ─────────────────────────────────────────────────

static REGISTRY: OnceLock<HashMap<String, EntityPrototype>> = OnceLock::new();

fn load_registry() -> HashMap<String, EntityPrototype> {
    let json = include_str!("../data/prototypes.json");
    let prototypes: Vec<EntityPrototype> = serde_json::from_str(json)
        .expect("prototypes.json must be valid JSON matching EntityPrototype schema");
    prototypes.into_iter().map(|p| (p.name.clone(), p)).collect()
}

/// Lookup prototype by entity name. Returns None for unknown/modded entities.
pub fn lookup(name: &str) -> Option<&'static EntityPrototype> {
    REGISTRY.get_or_init(load_registry).get(name)
}

/// All registered prototype names.
pub fn all_names() -> Vec<&'static str> {
    REGISTRY.get_or_init(load_registry).keys().map(|s| s.as_str()).collect()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
