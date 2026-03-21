pub use factorio_templates;

pub mod recipe;
pub mod calculator;

pub use recipe::{
    all_recipes, lookup, search, BeltTier, Ingredient, MachineClass, MachineTier, ModuleConfig,
    ModuleType, Recipe, RECIPES,
};
pub use calculator::{
    build_production_chain, machine_count_for_rate, ProductionChain, ProductionChainEntry,
};
