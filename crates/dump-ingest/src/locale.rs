// Loads Factorio locale JSON files (`{"names": {...}, "descriptions": {...}}`).
use std::collections::HashMap;
use std::path::Path;

use crate::error::IngestError;

/// Localised display names keyed by prototype name, e.g. `"transport-belt"` ->
/// `"Transport belt"`.
#[derive(Debug)]
pub struct Locale {
    names: HashMap<String, String>,
}

impl Locale {
    /// Test seam: the real tool always loads from disk via `load_locale`.
    #[cfg(test)]
    pub fn empty() -> Self {
        Locale { names: HashMap::new() }
    }

    /// Test seam: builds a locale in memory so mapping tests don't have to
    /// stage a file on disk just to look up one display name.
    #[cfg(test)]
    pub fn from_names<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Locale { names: pairs.into_iter().map(|(k, v)| (k.into(), v.into())).collect() }
    }

    pub fn name(&self, key: &str) -> Option<&str> {
        self.names.get(key).map(String::as_str)
    }
}

#[derive(serde::Deserialize)]
struct LocaleFile {
    #[serde(default)]
    names: HashMap<String, String>,
}

/// Reads `<dir>/<kind>-locale.json`, e.g. `kind = "entity"` -> `entity-locale.json`.
/// `kind` is a parameter (rather than hard-coding "entity") so the recipe-ingest
/// follow-up can call `load_locale(dir, "recipe")` without reshaping this module.
pub fn load_locale(dir: &Path, kind: &str) -> Result<Locale, IngestError> {
    let path = dir.join(format!("{kind}-locale.json"));
    let text = std::fs::read_to_string(&path)
        .map_err(|source| IngestError::ReadLocale { path: path.display().to_string(), source })?;
    let file: LocaleFile = serde_json::from_str(&text)
        .map_err(|source| IngestError::ParseLocale { path: path.display().to_string(), source })?;
    Ok(Locale { names: file.names })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh unpredictable directory per test — a fixed name under /tmp would
    /// collide between concurrent runs and is pre-createable by another user.
    fn staged(kind: &str, contents: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(format!("{kind}-locale.json")), contents).unwrap();
        dir
    }

    #[test]
    fn empty_locale_has_no_names() {
        assert_eq!(Locale::empty().name("transport-belt"), None);
    }

    #[test]
    fn from_names_builds_a_lookup() {
        let locale = Locale::from_names([("transport-belt", "Transport belt")]);
        assert_eq!(locale.name("transport-belt"), Some("Transport belt"));
        assert_eq!(locale.name("missing-entity"), None);
    }

    #[test]
    fn loads_names_from_kind_locale_json() {
        let dir = staged(
            "entity",
            r#"{"names": {"transport-belt": "Transport belt"}, "descriptions": {}}"#,
        );
        let locale = load_locale(dir.path(), "entity").unwrap();
        assert_eq!(locale.name("transport-belt"), Some("Transport belt"));
        assert_eq!(locale.name("missing-entity"), None);
    }

    #[test]
    fn kind_selects_the_locale_file() {
        let dir = staged("recipe", r#"{"names": {"iron-plate": "Iron plate"}}"#);
        assert_eq!(load_locale(dir.path(), "recipe").unwrap().name("iron-plate"), Some("Iron plate"));
        assert!(matches!(
            load_locale(dir.path(), "entity").unwrap_err(),
            IngestError::ReadLocale { .. }
        ));
    }

    #[test]
    fn missing_locale_file_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_locale(dir.path(), "entity").unwrap_err();
        assert!(matches!(err, IngestError::ReadLocale { .. }));
    }

    #[test]
    fn malformed_locale_json_is_a_parse_error() {
        let dir = staged("entity", "{ not json");
        let err = load_locale(dir.path(), "entity").unwrap_err();
        assert!(matches!(err, IngestError::ParseLocale { .. }));
    }
}
