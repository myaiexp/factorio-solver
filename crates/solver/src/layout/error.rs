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

    /// The generator builds belt-fed *item* chains. A fluid cannot ride a
    /// belt, and pipes are a later phase — so a step that moves one has no
    /// layout, whichever side of the machine it is on.
    #[error(
        "recipe `{recipe}` moves `{item}` as a fluid, and this generator only builds \
         belt-fed item chains — declare a product that comes after the fluid instead"
    )]
    FluidOnBelt { recipe: String, item: String },

    /// One belt carries two lanes, so a machine row fed by a single belt can
    /// take at most two distinct ingredients. Three-plus needs a second input
    /// belt reached by long-handed inserters, which is not built yet.
    #[error(
        "recipe `{recipe}` needs {} ingredients ({}) but one input belt carries only two \
         lanes — declare one of them available so it comes off the bus as its own block",
        .items.len(), .items.join(", ")
    )]
    TooManyIngredients { recipe: String, items: Vec<String> },

    /// The mirror of the above on the output edge: one output belt, two lanes.
    #[error(
        "recipe `{recipe}` yields {} products ({}) but one output belt carries only two lanes",
        .items.len(), .items.join(", ")
    )]
    TooManyOutputs { recipe: String, items: Vec<String> },

    /// Every ingredient gets its own inserter along the machine's input edge,
    /// so the machine has to be at least that wide.
    #[error(
        "`{machine}` is {width} tile(s) wide but recipe `{recipe}` needs {needed} \
         inserters along that edge"
    )]
    NoRoomForInserters { recipe: String, machine: String, needed: usize, width: u32 },

    #[error(
        "no free cell within reach of every machine in step `{step}` to put a `{pole}` — \
         try a pole with a larger supply area"
    )]
    NoRoomForPole { step: String, pole: String },

    #[error("the plan has no steps to build — everything asked for is already on the bus")]
    EmptyPlan,

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
    fn too_many_ingredients_lists_them() {
        let e = LayoutError::TooManyIngredients {
            recipe: "advanced-circuit".into(),
            items: vec!["copper-cable".into(), "electronic-circuit".into(), "plastic-bar".into()],
        };
        let s = e.to_string();
        assert!(s.contains("advanced-circuit") && s.contains("plastic-bar"), "{s}");
        assert!(s.contains('3'), "the count should be in the message: {s}");
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
