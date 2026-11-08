// Template extraction from a grid region, the template model, and JSON persistence.
pub use factorio_grid;
pub use factorio_grid::factorio_blueprint;

mod extract;
mod json;
mod model;
#[cfg(test)]
mod testsupport;

pub use extract::extract_template;
pub use json::{load_from_json, save_to_json};
pub use model::{IoPoint, IoRole, Template, TemplateEntity};
