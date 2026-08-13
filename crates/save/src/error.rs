// Errors from opening and decoding a Factorio save file.
use thiserror::Error;

use crate::init::Version;

/// Every variant names the offending file or chunk, matching the project's
/// existing rule that ambiguity is always an error and never a silent guess.
#[derive(Debug, Error)]
pub enum SaveError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("save is missing required entry '{name}'")]
    MissingEntry { name: String },

    #[error("chunk '{chunk}' failed to decompress (corrupt or truncated zlib stream)")]
    Decompress { chunk: String },

    #[error(
        "inflated save stream is {actual} bytes but level.datmetadata declares {declared} \
         — the chunks did not reassemble to the declared total"
    )]
    SizeMismatch { declared: u64, actual: u64 },

    #[error("level-init.dat is malformed: {reason}")]
    MalformedInit { reason: String },

    #[error(
        "player-force marker (0x01 0x06 \"player\") not found anywhere in this save's \
         level.dat chunks — this may not be a Factorio 2.0+ save, or the save's \
         player-force layout differs from what this crate expects"
    )]
    ForceNotFound,

    #[error(
        "no player-force recipe-unlock layout satisfies the calibration invariant for a \
         {}.{}.{} save (checked {checked} stride/offset candidates) — the committed recipe \
         dump most likely does not match this save's game version; regenerate it by running \
         'factorio --dump-data' against a matching Factorio install and re-ingesting via \
         crates/dump-ingest",
        .version.major, .version.minor, .version.patch
    )]
    CalibrationFailed { version: Version, checked: usize },

    #[error(
        "{candidates} different (stride, offset) layouts all satisfy the calibration \
         invariant for the player force's recipe-unlock array — refusing to guess which is \
         real; widen the default-enabled recipe set passed to unlocked_recipes so the \
         layouts can be told apart"
    )]
    CalibrationAmbiguous { candidates: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_is_a_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&SaveError::MissingEntry { name: "level-init.dat".into() });
        assert_error(&SaveError::Decompress { chunk: "level.dat3".into() });
        assert_error(&SaveError::SizeMismatch { declared: 10, actual: 9 });
        assert_error(&SaveError::MalformedInit { reason: "x".into() });
        assert_error(&SaveError::ForceNotFound);
        assert_error(&SaveError::CalibrationFailed {
            version: Version { major: 2, minor: 0, patch: 77 },
            checked: 4036,
        });
        assert_error(&SaveError::CalibrationAmbiguous { candidates: 2 });
    }

    #[test]
    fn force_not_found_names_the_marker() {
        let msg = SaveError::ForceNotFound.to_string();
        assert!(msg.contains("player"), "{msg}");
    }

    #[test]
    fn calibration_failed_names_the_version_and_the_remedy() {
        let msg = SaveError::CalibrationFailed {
            version: Version { major: 2, minor: 0, patch: 77 },
            checked: 4036,
        }
        .to_string();
        assert!(msg.contains("2.0.77"), "must name the save's version: {msg}");
        assert!(msg.contains("4036"), "must name how much was checked: {msg}");
        assert!(
            msg.contains("dump-ingest") && msg.contains("--dump-data"),
            "must name the remedy (regenerate the dump via dump-ingest): {msg}"
        );
    }

    #[test]
    fn calibration_ambiguous_names_the_candidate_count() {
        let msg = SaveError::CalibrationAmbiguous { candidates: 3 }.to_string();
        assert!(msg.contains('3'), "{msg}");
        assert!(msg.contains("refusing to guess"), "must refuse rather than pick one: {msg}");
    }

    #[test]
    fn size_mismatch_names_both_numbers() {
        let msg = SaveError::SizeMismatch { declared: 100, actual: 42 }.to_string();
        assert!(msg.contains("100") && msg.contains("42"), "{msg}");
    }

    #[test]
    fn missing_entry_names_the_file() {
        let msg = SaveError::MissingEntry { name: "level.datmetadata".into() }.to_string();
        assert!(msg.contains("level.datmetadata"), "{msg}");
    }
}
