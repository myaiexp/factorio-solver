// Probes known Factorio install locations, validating each by its data/base/ subdirectory.

use std::path::{Path, PathBuf};

/// Probes known install locations; `override_path` wins when set (and must
/// still be validated — a configured path that doesn't exist yields None).
/// Falls through, in order: `override_path`, `$FACTORIO_INSTALL_DIR`, a
/// handful of well-known Steam/native locations, then the first candidate
/// with a `data/base/` subdirectory wins.
pub fn detect_install(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = override_path {
        return validate_install(path);
    }
    if let Ok(env_path) = std::env::var("FACTORIO_INSTALL_DIR")
        && let Some(install) = validate_install(Path::new(&env_path))
    {
        return Some(install);
    }
    candidates().into_iter().find_map(|path| validate_install(&path))
}

/// Ordered list of well-known install locations. Home-relative ones are
/// only included when `$HOME` (or the platform equivalent) resolves.
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::home_dir() {
        paths.push(home.join(".local/share/Steam/steamapps/common/Factorio"));
        paths.push(home.join(".steam/steam/steamapps/common/Factorio"));
        // Usually the *user data* dir rather than the install proper; the
        // data/base/ check below is what keeps this candidate honest.
        paths.push(home.join(".factorio"));
    }
    paths.push(PathBuf::from("/opt/factorio"));
    paths.push(PathBuf::from("/usr/share/factorio"));
    paths
}

/// A candidate is a real install only if it has a `data/base/` subdirectory
/// — a bare directory that happens to exist is not an install.
fn validate_install(path: &Path) -> Option<PathBuf> {
    if path.join("data").join("base").is_dir() { Some(path.to_path_buf()) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_install_yields_none_and_does_not_panic() {
        assert!(detect_install(Some(Path::new("/nonexistent"))).is_none());
    }

    #[test]
    fn a_directory_without_data_base_is_not_an_install() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect_install(Some(dir.path())).is_none());
    }

    #[test]
    fn override_path_wins_over_probing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/base")).unwrap();
        let found = detect_install(Some(dir.path())).unwrap();
        assert_eq!(found, dir.path());
    }

    #[test]
    fn a_directory_with_data_base_as_a_file_is_not_an_install() {
        // Guards against `is_dir()` false positives: data/base existing as a
        // regular file (e.g. a stray download) must not pass validation.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/base"), b"not a directory").unwrap();
        assert!(detect_install(Some(dir.path())).is_none());
    }
}
