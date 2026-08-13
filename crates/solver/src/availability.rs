// Bridges a Factorio save file to the chain solver's Availability constraint.
//
// The only place `solver` depends on `save`, which is what keeps the crate
// graph acyclic: `ui -> solver -> save`. The dependency points this way
// because the calibration invariant needs the default-enabled recipe set, and
// that set lives here, in the committed registry — so `save` takes it as a
// parameter and this module is what supplies it.
use std::collections::HashSet;
use std::path::Path;

use factorio_save::SaveFile;

use crate::chain::Availability;
use crate::recipe;

/// The save-reading error type, re-exported so callers can name the result of
/// `from_save` without depending on `factorio-save` themselves.
///
/// Deliberately *not* folded into `ChainError`: failing to read a save is not
/// a chain failure, and a UI that conflated them would offer chain remedies
/// ("add it to the bus") for a corrupt zip.
pub use factorio_save::SaveError;

/// Recipe names the game enables without research.
///
/// This is the calibration invariant `factorio-save` searches with, not a
/// hint — the layout it accepts must mark every one of these enabled. It
/// therefore has to come from the same dump the save was made against; a
/// mismatch is exactly what `SaveError::CalibrationFailed` reports.
pub fn default_enabled() -> HashSet<String> {
    recipe::registry().values().filter(|r| r.enabled).map(|r| r.name.clone()).collect()
}

/// Read a save and turn it into an availability constraint.
pub fn from_save(path: &Path) -> Result<Availability, SaveError> {
    let mut save = SaveFile::open(path)?;
    let unlocked = save.unlocked_recipes(&default_enabled())?;
    Ok(Availability::Unlocked(unlocked))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_enabled_matches_the_registry() {
        let d = default_enabled();
        assert!(d.contains("iron-plate"));
        assert!(!d.contains("beacon"), "beacon is research-locked in the dump");
        assert_eq!(d.len(), recipe::registry().values().filter(|r| r.enabled).count());
    }

    /// The set is the calibration invariant, so an empty or near-empty one
    /// would make every alignment satisfy it and turn a unique hit into
    /// `CalibrationAmbiguous`. Guards against a registry load that silently
    /// yields nothing.
    #[test]
    fn default_enabled_is_a_substantial_set() {
        assert!(default_enabled().len() > 100, "too small to discriminate an alignment");
    }

    #[test]
    fn a_missing_save_is_an_error_not_a_panic() {
        assert!(from_save(Path::new("/nonexistent/save.zip")).is_err());
    }

    /// End to end over the whole bridge, against a synthetic save whose recipe
    /// table is the *real* registry — 649 recipes, not a handful. No real save
    /// can be committed, so this is the closest available check that
    /// `default_enabled()` is specific enough to pick out one alignment at a
    /// realistic table size: too weak a set and the search would come back
    /// `CalibrationAmbiguous` rather than a unique hit.
    #[test]
    fn a_synthetic_save_over_the_real_registry_round_trips() {
        let all: Vec<String> = recipe::registry().keys().cloned().collect();
        let all_refs: Vec<&str> = all.iter().map(String::as_str).collect();

        // A save must have every default-enabled recipe on, or no alignment
        // can satisfy the invariant. Beyond those, one researched extra.
        let mut expected = default_enabled();
        expected.insert("beacon".to_string());
        let unlocked: Vec<&str> = expected.iter().map(String::as_str).collect();

        let zip = factorio_save::testsupport::FixtureSave::new()
            .with_recipes(&all_refs)
            .with_unlocked(&unlocked)
            .build();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.zip");
        std::fs::write(&path, zip).unwrap();

        match from_save(&path).expect("the real registry must calibrate uniquely") {
            Availability::Unlocked(got) => {
                assert_eq!(got, expected);
                assert!(got.contains("beacon"), "the researched extra must decode as unlocked");
                assert!(
                    !got.contains("assembling-machine-3"),
                    "a locked recipe must not leak in"
                );
            }
            other => panic!("expected Unlocked, got {other:?}"),
        }
    }
}
