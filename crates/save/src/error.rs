// Errors from opening and decoding a Factorio save file.
use thiserror::Error;

/// Every variant names the offending file or chunk, matching the project's
/// existing rule that ambiguity is always an error and never a silent guess.
///
/// Task 2 (calibration + unlock decode) adds `ForceNotFound`,
/// `CalibrationFailed` and `CalibrationAmbiguous` on top of this set.
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
