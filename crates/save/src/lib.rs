// Crate root: opens a Factorio save and exposes level-init.dat + lazy chunks.
//
// No workspace-crate dependency, by design: `force.rs` decodes the player
// force's recipe-unlock array by searching for the alignment that satisfies
// an invariant built from `solver`'s default-enabled recipe set. Depending on
// `solver` here to get that set would invert the crate graph
// (`ui -> solver -> save`), so the set is a parameter instead — see the
// design doc at `.claude/plans/2026-08-13-save-derived-availability-design.md`.
use std::io::{Cursor, Read};
use std::path::Path;

use zip::ZipArchive;

mod chunks;
mod error;
mod force;
mod init;

#[cfg(any(test, feature = "testsupport"))]
pub mod testsupport;
#[cfg(test)]
mod tests;

pub use error::SaveError;
pub use force::Calibration;
pub use init::{IdTable, Version};

/// An open Factorio save: the zip container, the eagerly-parsed
/// level-init.dat, and a buffer of `level.dat<N>` chunks inflated so far.
///
/// Chunk inflation is lazy and append-only — `open`/`open_bytes` read
/// `level-init.dat` and `level.datmetadata` (both small and uncompressed) but
/// inflate nothing. A save's decompressed stream commonly runs to tens of
/// megabytes; the data this crate actually needs (id tables, and — in a later
/// task — the player force) sits in the first couple hundred kilobytes, so
/// inflating on demand instead of up front is the difference between a fast
/// load and a slow one.
pub struct SaveFile {
    archive: ZipArchive<Cursor<Vec<u8>>>,
    chunk_names: Vec<String>,
    inflated: Vec<u8>,
    inflated_count: usize,
    metadata_total: u64,
    version: Version,
    mods: Vec<String>,
    recipes: IdTable,
    technologies: IdTable,
    // Cached by `unlocked_recipes` (see force.rs) so a second call decodes
    // against the already-accepted (stride, offset) instead of re-running
    // the calibration search.
    calibration: Option<Calibration>,
}

impl SaveFile {
    pub fn open(path: &Path) -> Result<Self, SaveError> {
        let bytes = std::fs::read(path)?;
        Self::open_bytes(&bytes)
    }

    pub fn open_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes.to_vec()))?;

        let init_bytes = read_entry(&mut archive, "level-init.dat")?;
        let init = init::parse(&init_bytes)?;

        let metadata_bytes = read_entry(&mut archive, "level.datmetadata")?;
        let metadata_total = parse_metadata_total(&metadata_bytes)?;

        let chunk_names = chunks::ordered_chunk_names(&archive);

        Ok(Self {
            archive,
            chunk_names,
            inflated: Vec::new(),
            inflated_count: 0,
            metadata_total,
            version: init.version,
            mods: init.mods,
            recipes: init.recipes,
            technologies: init.technologies,
            calibration: None,
        })
    }

    pub fn version(&self) -> Version {
        self.version
    }

    /// Mod names only. Version bytes in level-init are not decoded — the
    /// encoding was never verified, and names alone answer "is this vanilla".
    pub fn mods(&self) -> &[String] {
        &self.mods
    }

    pub fn recipes(&self) -> &IdTable {
        &self.recipes
    }

    pub fn technologies(&self) -> &IdTable {
        &self.technologies
    }

    /// Number of `level.dat<N>` chunks inflated so far. Test-facing but
    /// public: `force.rs`'s lazy-inflation guard asserts that decoding the
    /// unlock array inflates only the chunks it needed, never the whole
    /// stream.
    pub fn inflated_chunk_count(&self) -> usize {
        self.inflated_count
    }

    /// Inflates every remaining chunk and checks the total against
    /// `level.datmetadata`. Explicit and non-default — not on the load path,
    /// because lazy inflation means the full byte count is normally never
    /// computed, and this call defeats that optimisation on purpose.
    pub fn verify_total_size(&mut self) -> Result<(), SaveError> {
        self.ensure_inflated(usize::MAX)?;
        let actual = self.inflated.len() as u64;
        if actual != self.metadata_total {
            return Err(SaveError::SizeMismatch { declared: self.metadata_total, actual });
        }
        Ok(())
    }

    /// Inflates further chunks, in numeric order, until the accumulated
    /// buffer holds at least `at_least` bytes or chunks run out. Never
    /// shrinks or re-inflates what is already in the buffer, so repeated
    /// calls with a growing target only do incremental work.
    pub(crate) fn ensure_inflated(&mut self, at_least: usize) -> Result<(), SaveError> {
        while self.inflated.len() < at_least && self.inflated_count < self.chunk_names.len() {
            let name = self.chunk_names[self.inflated_count].clone();
            let bytes = chunks::inflate_chunk(&mut self.archive, &name)?;
            self.inflated.extend_from_slice(&bytes);
            self.inflated_count += 1;
        }
        Ok(())
    }
}

fn read_entry(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, SaveError> {
    let mut file =
        archive.by_name(name).map_err(|_| SaveError::MissingEntry { name: name.to_string() })?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

fn parse_metadata_total(bytes: &[u8]) -> Result<u64, SaveError> {
    let arr: [u8; 8] = bytes.get(0..8).and_then(|s| s.try_into().ok()).ok_or_else(|| {
        SaveError::MalformedInit { reason: "level.datmetadata is shorter than 8 bytes".into() }
    })?;
    Ok(u64::from_le_bytes(arr))
}
