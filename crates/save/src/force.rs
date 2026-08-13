// Player-force location, (stride, offset) calibration search, and the
// recipe-unlock decode. See the design doc
// (.claude/plans/2026-08-13-save-derived-availability-design.md) for how the
// calibration invariant below was measured against ~50 real saves.
use std::collections::HashSet;

use crate::SaveFile;
use crate::error::SaveError;
use crate::init::IdTable;

/// The literal bytes that locate the player force in the inflated level.dat
/// stream: a Factorio length-prefixed string field naming the force
/// (`0x06` = length 6, then `"player"`), preceded by a type tag byte. This
/// was found by brute-force byte search against real saves, not derived from
/// any documented format — see the design doc.
const FORCE_MARKER: &[u8] = b"\x01\x06player";

/// Record width, in bytes, to search. Measured: 7 at 2.0.8/2.0.28/2.0.32, 6
/// at 2.0.60/2.0.77. Widened by one on each side because the measurement set
/// is ~50 saves, not exhaustive.
const STRIDE_RANGE: std::ops::RangeInclusive<usize> = 5..=8;

/// How far past `force_base` the record array's start may sit. Measured
/// offsets: `+67`, `+181`, `+134` (all comfortably inside this span).
const OFFSET_MIN: usize = 16;
const OFFSET_MAX: usize = 1024;

/// An accepted (stride, offset) pair for the player force's recipe-unlock
/// record array. `offset` is the absolute byte index, in the save's inflated
/// stream, of record id 0's flag byte; `stride` is the record width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    pub stride: usize,
    pub offset: usize,
}

impl SaveFile {
    /// The player force's unlocked recipe names.
    ///
    /// `default_enabled` is the set of recipe names the game enables without
    /// research, supplied by the caller (this crate cannot depend on
    /// `solver`'s registry — see the crate-root doc comment). It is the
    /// calibration invariant, not a hint: the located array must mark every
    /// one of `default_enabled ∩ recipes().names()` as enabled, and only a
    /// layout that does so is ever accepted. See the design doc for why a
    /// weaker check (e.g. "the common recipes are present") is not safe to
    /// substitute — it is satisfied by a one-record-off alignment.
    pub fn unlocked_recipes(
        &mut self,
        default_enabled: &HashSet<String>,
    ) -> Result<HashSet<String>, SaveError> {
        if let Some(cal) = self.calibration {
            // Already calibrated: decode straight from the buffer we already
            // have, no re-search and no further inflation. Note this path does
            // not re-check `default_enabled` — the cache is keyed on the save,
            // not on the invariant set, so a caller that passes a *different*
            // set on a second call gets the first call's calibration. Every
            // caller in this workspace passes the same registry-derived set.
            return Ok(decode(&self.inflated, cal, &self.recipes));
        }

        let force_base = self.find_force_base()?;
        let (candidates, checked) = self.search_candidates(force_base, default_enabled)?;

        match candidates.as_slice() {
            [] => Err(SaveError::CalibrationFailed { version: self.version, checked }),
            [only] => {
                self.calibration = Some(*only);
                Ok(decode(&self.inflated, *only, &self.recipes))
            }
            _ => Err(SaveError::CalibrationAmbiguous { candidates: candidates.len() }),
        }
    }

    /// The calibration accepted by the last successful `unlocked_recipes`
    /// call. `None` before that first success.
    pub fn calibration(&self) -> Option<Calibration> {
        self.calibration
    }

    /// Byte index just past the first `01 06 "player"` marker in the
    /// inflated stream. Grows the buffer one chunk at a time rather than
    /// inflating everything up front — the force sits well inside the first
    /// chunk on every save measured (see the design doc) — and re-searches
    /// from `FORCE_MARKER.len() - 1` bytes before the previous end each pass,
    /// so a marker that straddles an old/new chunk boundary is never missed
    /// by only scanning the newly appended bytes.
    fn find_force_base(&mut self) -> Result<usize, SaveError> {
        let mut search_from = 0;
        loop {
            if let Some(pos) = find_subslice(&self.inflated[search_from..], FORCE_MARKER) {
                return Ok(search_from + pos + FORCE_MARKER.len());
            }
            search_from = self.inflated.len().saturating_sub(FORCE_MARKER.len() - 1);

            let before = self.inflated_count;
            self.ensure_inflated(self.inflated.len() + 1)?;
            if self.inflated_count == before {
                // No new chunk was appended: the stream is exhausted and the
                // marker never appeared in it.
                return Err(SaveError::ForceNotFound);
            }
        }
    }

    /// Search every (stride, offset) pair in the calibration window,
    /// inflating further chunks only as a candidate's reach requires it.
    /// Returns the surviving candidates and how many pairs were examined
    /// (for `CalibrationFailed`'s diagnostic).
    fn search_candidates(
        &mut self,
        force_base: usize,
        default_enabled: &HashSet<String>,
    ) -> Result<(Vec<Calibration>, usize), SaveError> {
        let max_id = self.recipes.len(); // record count is max_id + 1 (id 0 = sentinel)
        let mut candidates = Vec::new();
        let mut checked = 0;

        for stride in STRIDE_RANGE {
            for offset in force_base + OFFSET_MIN..=force_base + OFFSET_MAX {
                checked += 1;

                // Byte just past the last record's flag byte (id = max_id).
                let needed = offset + max_id * stride + 1;
                self.ensure_inflated(needed)?;
                if self.inflated.len() < needed {
                    // Chunks ran out before this candidate's array would
                    // end. Not a candidate — not an error; a smaller stride
                    // at a later offset, or a later stride, may still fit.
                    continue;
                }

                let satisfies = candidate_satisfies(
                    &self.inflated,
                    stride,
                    offset,
                    max_id,
                    &self.recipes,
                    default_enabled,
                );
                if satisfies {
                    candidates.push(Calibration { stride, offset });
                }
            }
        }

        Ok((candidates, checked))
    }
}

/// True when every record in `0..=max_id` has a valid 0/1 flag byte and every
/// name in `default_enabled` that this save's recipe table knows about
/// decodes as enabled. Returns on the first disqualifying byte rather than
/// scanning the whole array, so the common case (one candidate, most
/// rejected immediately) stays close to one byte read per rejected offset.
fn candidate_satisfies(
    inflated: &[u8],
    stride: usize,
    offset: usize,
    max_id: usize,
    recipes: &IdTable,
    default_enabled: &HashSet<String>,
) -> bool {
    for id in 0..=max_id {
        let flag = inflated[offset + id * stride];
        if flag > 1 {
            return false;
        }
        // id 0 (the recipe-unknown sentinel) has no name and can never be in
        // default_enabled, so this lookup only ever fires for a real recipe id.
        if flag == 0
            && let Some(name) = recipes.name(id as u16)
            && default_enabled.contains(name)
        {
            return false;
        }
    }
    true
}

/// Decode the full unlocked set for an accepted calibration. Id 0 (the
/// recipe-unknown sentinel) has no name in the table and is silently
/// skipped, per the design doc — never an error, since a save legitimately
/// has no name for it.
fn decode(inflated: &[u8], cal: Calibration, recipes: &IdTable) -> HashSet<String> {
    let max_id = recipes.len();
    let mut out = HashSet::new();
    for id in 0..=max_id {
        if inflated[cal.offset + id * cal.stride] == 1
            && let Some(name) = recipes.name(id as u16)
        {
            out.insert(name.to_string());
        }
    }
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests;
