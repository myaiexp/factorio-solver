// Error type for dump-ingest: every variant names the offending file or entity.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("failed to read dump file {path}: {source}")]
    ReadDump { path: String, #[source] source: std::io::Error },

    #[error("failed to parse dump file {path} as JSON: {source}")]
    ParseDump { path: String, #[source] source: serde_json::Error },

    #[error("failed to read locale file {path}: {source}")]
    ReadLocale { path: String, #[source] source: std::io::Error },

    #[error("failed to parse locale file {path} as JSON: {source}")]
    ParseLocale { path: String, #[source] source: serde_json::Error },

    #[error("entity '{name}' is missing required field '{field}'")]
    MissingField { name: String, field: String },

    #[error("entity '{name}' has an unparseable energy_usage value '{value}'")]
    UnparseablePower { name: String, value: String },

    #[error("recipe '{name}' is missing required field '{field}'")]
    MissingRecipeField { name: String, field: String },

    #[error("recipe '{name}' has an ingredient/result with an unrecognised type '{value}' (expected \"item\" or \"fluid\")")]
    UnknownItemType { name: String, value: String },

    #[error("recipe '{name}' has an unrecognised '{field}' shape (expected an array, an empty object, or null)")]
    UnexpectedIngredientShape { name: String, field: String },

    #[error("recipe '{name}' has field '{field}' with an unexpected JSON type: {value}")]
    UnexpectedRecipeFieldType { name: String, field: String, value: String },

    #[error("technology '{name}' is missing required field '{field}'")]
    MissingTechnologyField { name: String, field: String },

    #[error("technology '{name}' has field '{field}' with an unexpected JSON type: {value}")]
    UnexpectedTechnologyFieldType { name: String, field: String, value: String },

    #[error("technology '{technology}' lists prerequisite '{prerequisite}', which is not a known technology")]
    UnknownPrerequisite { technology: String, prerequisite: String },

    #[error("failed to serialize prototypes to JSON: {0}")]
    Serialize(#[source] serde_json::Error),

    #[error("failed to write output file {path}: {source}")]
    WriteOutput { path: String, #[source] source: std::io::Error },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_field_error_names_entity_and_field() {
        let err = IngestError::MissingField { name: "widget".into(), field: "collision_box".into() };
        let msg = err.to_string();
        assert!(msg.contains("widget"), "message should name the entity: {msg}");
        assert!(msg.contains("collision_box"), "message should name the field: {msg}");
    }

    #[test]
    fn io_errors_wrap_the_path() {
        let source = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = IngestError::ReadDump { path: "/tmp/missing-dump.json".into(), source };
        assert!(err.to_string().contains("/tmp/missing-dump.json"));
    }

    #[test]
    fn missing_recipe_field_error_names_recipe_and_field() {
        let err = IngestError::MissingRecipeField { name: "widget-recipe".into(), field: "ingredients[].amount".into() };
        let msg = err.to_string();
        assert!(msg.contains("widget-recipe"), "message should name the recipe: {msg}");
        assert!(msg.contains("ingredients[].amount"), "message should name the field: {msg}");
    }

    #[test]
    fn unknown_item_type_error_names_recipe_and_value() {
        let err = IngestError::UnknownItemType { name: "odd-recipe".into(), value: "virtual".into() };
        let msg = err.to_string();
        assert!(msg.contains("odd-recipe") && msg.contains("virtual"), "got: {msg}");
    }

    #[test]
    fn unexpected_ingredient_shape_error_names_recipe_and_field() {
        let err = IngestError::UnexpectedIngredientShape { name: "weird-recipe".into(), field: "ingredients".into() };
        let msg = err.to_string();
        assert!(msg.contains("weird-recipe") && msg.contains("ingredients"), "got: {msg}");
    }

    #[test]
    fn unknown_prerequisite_error_names_technology_and_prerequisite() {
        let err = IngestError::UnknownPrerequisite {
            technology: "foundry".into(),
            prerequisite: "calcite-processing".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("foundry"), "message should name the technology: {msg}");
        assert!(msg.contains("calcite-processing"), "message should name the prerequisite: {msg}");
    }
}
