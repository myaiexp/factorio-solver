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
}
