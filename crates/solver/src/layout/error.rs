// Named, actionable failures from the block generator.
//
// Same rule as `chain::error`: a silently wrong blueprint looks right and
// costs real in-game time to discover, so every refusal names the offending
// recipe/entity and, where there is one, the remedy.
use factorio_grid::GridError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayoutError {
    /// A step whose recipe both consumes and produces the same item. In-game
    /// that needs a self-loop return belt, which is a different topology from
    /// every other step and is deliberately not invented here — the chain
    /// calculator still answers the ratio question correctly.
    #[error(
        "recipe `{recipe}` both consumes and produces `{item}`, and a self-looping \
         step has no belt-fed layout yet — the chain calculator still gives you \
         its ratios"
    )]
    CyclicStep { recipe: String, item: String },

    #[error("`{0}` is not a transport belt (no belt throughput in the prototype registry)")]
    BeltTierUnknown(String),

    #[error("`{0}` is not an electric pole (no supply area in the prototype registry)")]
    PoleUnknown(String),

    #[error("`{0}` is not an inserter (no pickup/insert position in the prototype registry)")]
    InserterUnknown(String),

    /// A topology with two belts on one side reaches the outer one with this
    /// inserter. A one-tile inserter named here would place fine and then
    /// never touch that belt, so the reach is checked, not just the category.
    #[error(
        "`{0}` cannot reach two tiles, so it cannot serve the outer belt of a pair — \
         `long-handed-inserter` can"
    )]
    LongInserterUnknown(String),

    /// The generator builds belt-fed *item* chains. A fluid cannot ride a
    /// belt, and pipes are a later phase — so a step that moves one has no
    /// layout, whichever side of the machine it is on.
    #[error(
        "recipe `{recipe}` moves `{item}` as a fluid, and this generator only builds \
         belt-fed item chains — declare a product that comes after the fluid instead"
    )]
    FluidOnBelt { recipe: String, item: String },

    /// A cell's ingredient side has `lanes` lanes (two per belt on that
    /// side) to divide among ingredients, one strictly-positive share each.
    /// More ingredients than that has no allocation at all — unlike the old
    /// per-belt cap this replaces, the fix is a wider topology
    /// (`spine_belts`/`edge_belts`), not just moving an item off the bus.
    #[error(
        "recipe `{recipe}` needs {} ingredients ({}) but the ingredient side of a cell has \
         only {lanes} lanes to divide among them — widen the topology (`spine_belts`/\
         `edge_belts`) or declare one of them available so it comes off the bus as its own block",
        .ingredients.len(), .ingredients.join(", ")
    )]
    TooManyIngredientsForLanes { recipe: String, ingredients: Vec<String>, lanes: u32 },

    /// A cell column owns its product lanes outright — there is no shared
    /// product belt the way ingredients share a spine — so a recipe with
    /// more than one item result has no column to put the second one in.
    #[error(
        "recipe `{recipe}` yields {} products ({}) but a cell column owns its product lanes \
         outright, with nowhere to put a second one — declare one of them available so it \
         comes off the bus as its own block",
        .products.len(), .products.join(", ")
    )]
    MultipleProducts { recipe: String, products: Vec<String> },

    /// `rate` is the per-machine rate the chain calculator already computed;
    /// `lane` is what a single lane of the configured belt tier carries. When
    /// `rate` exceeds even every lane a stream could ever be allocated, no
    /// column length feeds it — the fix is a faster belt tier, not a wider
    /// cell.
    #[error(
        "recipe `{recipe}`'s `{item}` needs {rate:.3}/s per machine but one lane of this belt \
         tier only carries {lane:.3}/s — not even every lane this stream could ever be given \
         is enough to feed a single machine, so only a faster belt tier fixes this"
    )]
    StreamExceedsOneLane { recipe: String, item: String, rate: f64, lane: f64 },

    /// `CellTopology.spine_belts`/`edge_belts` outside 1..=2 has no belt-fed
    /// meaning: a cell needs at least one belt per side to exist, and every
    /// formula downstream only ever reasons about one or two.
    #[error("cell topology field `{field}` is {value}, but only 1 or 2 belts are supported")]
    InvalidTopology { field: String, value: u8 },

    /// Every ingredient/product an inserter reaches needs its own gutter tile
    /// along the machine's column edge — its height, not its width, now that
    /// a columnar cell runs belts vertically.
    #[error(
        "`{machine}` has {edge_tiles} tile(s) along the edge recipe `{recipe}` needs {needed} \
         inserters on — a taller machine or a narrower topology fixes it"
    )]
    NoRoomForInserters { recipe: String, machine: String, needed: usize, edge_tiles: u32 },

    /// A long inserter's insert offset lands two tiles past its own cell, so
    /// against a machine narrower than that it would insert into open ground.
    /// A guard rather than a live path — every vanilla crafting machine is at
    /// least 2 wide — but reported as its own condition, because folding it
    /// into `NoRoomForInserters` would print a *width* into a field that now
    /// means the height of the machine's column edge.
    #[error(
        "`{machine}` is {width} tile(s) wide, and a long inserter reaching recipe `{recipe}`'s \
         outer belt would insert two tiles in, past the machine entirely — use a topology with \
         one belt per side"
    )]
    MachineTooNarrowForLongInserter { recipe: String, machine: String, width: u32 },

    #[error(
        "no free cell within reach of every machine in step `{step}` to put a `{pole}` — \
         try a pole with a larger supply area"
    )]
    NoRoomForPole { step: String, pole: String },

    #[error("the plan has no steps to build — everything asked for is already on the bus")]
    EmptyPlan,

    // ── Pre-emit validation ───────────────────────────────────────────
    // These describe a generator bug rather than user input: the row and
    // pole passes are supposed to make all three impossible. They are hard
    // errors so a broken block is never handed to the player as a blueprint
    // string that merely looks plausible.
    #[error("the `{recipe}` machine at ({x}, {y}) has no {missing} inserter")]
    MachineNotConnected { recipe: String, x: i32, y: i32, missing: String },

    #[error("two entities claim cell ({x}, {y})")]
    Overlap { x: i32, y: i32 },

    #[error("the `{recipe}` machine at ({x}, {y}) is outside every pole's supply area")]
    Unpowered { recipe: String, x: i32, y: i32 },

    #[error("placement failed: {0}")]
    Placement(#[from] GridError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyclic_step_names_recipe_and_item() {
        let e = LayoutError::CyclicStep {
            recipe: "kovarex-enrichment-process".into(),
            item: "uranium-235".into(),
        };
        let s = e.to_string();
        assert!(s.contains("kovarex-enrichment-process") && s.contains("uranium-235"), "{s}");
    }

    #[test]
    fn too_many_ingredients_for_lanes_names_the_lane_count() {
        let e = LayoutError::TooManyIngredientsForLanes {
            recipe: "advanced-circuit".into(),
            ingredients: vec![
                "copper-cable".into(),
                "electronic-circuit".into(),
                "plastic-bar".into(),
            ],
            lanes: 4,
        };
        let s = e.to_string();
        assert!(s.contains("advanced-circuit") && s.contains("plastic-bar"), "{s}");
        assert!(s.contains('4'), "the lane count should be in the message: {s}");
    }

    #[test]
    fn multiple_products_names_them() {
        let e = LayoutError::MultipleProducts {
            recipe: "uranium-processing".into(),
            products: vec!["uranium-235".into(), "uranium-238".into()],
        };
        let s = e.to_string();
        assert!(s.contains("uranium-processing") && s.contains("uranium-238"), "{s}");
    }

    #[test]
    fn stream_exceeds_one_lane_names_recipe_item_and_rates() {
        let e = LayoutError::StreamExceedsOneLane {
            recipe: "rocket-part".into(),
            item: "rocket-fuel".into(),
            rate: 500.0,
            lane: 22.5,
        };
        let s = e.to_string();
        assert!(s.contains("rocket-part") && s.contains("rocket-fuel"), "{s}");
        assert!(s.contains("500") && s.contains("22.5"), "{s}");
    }

    #[test]
    fn invalid_topology_names_the_field() {
        let e = LayoutError::InvalidTopology { field: "spine_belts".into(), value: 3 };
        let s = e.to_string();
        assert!(s.contains("spine_belts") && s.contains('3'), "{s}");
    }

    #[test]
    fn unknown_tiers_name_what_was_asked_for() {
        assert!(LayoutError::BeltTierUnknown("stone-furnace".into())
            .to_string()
            .contains("stone-furnace"));
        assert!(LayoutError::PoleUnknown("beacon".into()).to_string().contains("beacon"));
        assert!(LayoutError::InserterUnknown("chest".into()).to_string().contains("chest"));
    }
}
