// Points the chain panel at a Factorio save so `ChainGoal.availability` can
// restrict the solver to recipes the player has actually unlocked, instead
// of every recipe the registry knows about. `controls::save_picker` is the
// egui-facing half of this; everything here is plain data and file I/O so it
// can be unit-tested with no frame driven.
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use factorio_solver::availability::from_save;

/// A save file discovered on disk.
pub struct SaveEntry {
    pub path: PathBuf,
    pub label: String,
    pub modified: SystemTime,
}

/// Scan a saves directory, newest first (ties broken by name for
/// determinism). Returns empty when the directory is absent — the app must
/// build and run with no Factorio installed, so a missing
/// `~/.factorio/saves` is routine, never an error to surface.
pub fn scan_saves(dir: &Path) -> Vec<SaveEntry> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<SaveEntry> = read_dir
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "zip"))
        .filter_map(|path| {
            let modified = fs::metadata(&path).ok()?.modified().ok()?;
            let label = path.file_stem()?.to_string_lossy().into_owned();
            Some(SaveEntry { path, label, modified })
        })
        .collect();
    entries.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.label.cmp(&b.label)));
    entries
}

/// `~/.factorio/saves`, or `None` when the home directory cannot be
/// resolved.
///
/// `std::env::home_dir` carried a deprecation warning steering callers away
/// from it; that was lifted in Rust 1.86 (the doc now just names the one
/// remaining Windows edge case instead). This workspace's toolchain is past
/// that line, so calling it directly needs no `#[allow(deprecated)]` and no
/// `dirs`/`home` dependency is worth adding for a single path join.
pub fn default_saves_dir() -> Option<PathBuf> {
    std::env::home_dir().map(|home| home.join(".factorio").join("saves"))
}

/// What a file looked like at a point in time. Both halves are needed:
/// mtime granularity is a filesystem property (a whole second on some), so a
/// quick rewrite can land on the same timestamp, while a save rewritten with
/// no net size change is routine. Together they catch what either alone
/// misses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileSig {
    modified: SystemTime,
    len: u64,
}

fn file_sig(path: &Path) -> Option<FileSig> {
    let meta = fs::metadata(path).ok()?;
    Some(FileSig { modified: meta.modified().ok()?, len: meta.len() })
}

/// Panel-side state for the save picker: what was found on disk, what's
/// selected, and the outcome of decoding it.
pub struct SavePickerState {
    pub entries: Vec<SaveEntry>,
    pub selected: Option<PathBuf>,
    pub manual_path: String,
    /// The unlocked recipe count on success, the error text on failure.
    ///
    /// The decoded *set* is not kept here. It goes straight into the panel's
    /// one `available_recipes`, so an import and a hand-edit are the same
    /// state rather than two that can disagree — and so the tick list below
    /// can correct an import rather than being overruled by it. Decoding
    /// happens on selection and afterwards only when the file itself changes
    /// (see [`poll`](Self::poll)): `build_goal` runs on every Solve click and
    /// re-inflating a save there would be work for an answer that cannot have
    /// changed.
    pub status: Option<Result<usize, String>>,
    /// The signature of `selected` at the last decode *attempt*, successful or
    /// not. Recording the attempt rather than the success is what stops a
    /// corrupt or half-written save from being re-read on every frame — a
    /// failing file is read once per distinct signature, not once per frame.
    attempted: Option<FileSig>,
    /// The set the last *successful* decode produced. Kept solely to tell an
    /// untouched import from a hand-edited one, which is what decides whether
    /// a reload can be taken silently.
    imported: Option<BTreeSet<String>>,
    /// The file changed and the change was not adopted — because adopting it
    /// would discard hand edits, or because decoding it failed. The UI turns
    /// this into a Reload button; clicking it goes through `select` and so
    /// reports the real error if there is one.
    pub changed_on_disk: bool,
}

impl Default for SavePickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl SavePickerState {
    /// Scans the default saves directory once, up front, so a fresh panel's
    /// dropdown is populated before the player does anything.
    pub fn new() -> Self {
        Self {
            entries: scan_default_dir(),
            selected: None,
            manual_path: String::new(),
            status: None,
            attempted: None,
            imported: None,
            changed_on_disk: false,
        }
    }

    /// Re-read the saves directory. The initial scan happens once at panel
    /// construction, so this is what picks up a save written afterward.
    pub fn rescan(&mut self) {
        self.entries = scan_default_dir();
    }

    /// Decode `path`, record the outcome in `status`, and hand back the
    /// unlocked set for the caller to adopt.
    ///
    /// Returns `None` on failure and applies nothing, so a bad read never
    /// half-lands: leaving the previous set in place beats replacing it with
    /// an empty or stale one, which would make every recipe look locked —
    /// a worse outcome than simply not changing the constraint.
    pub fn select(&mut self, path: &Path) -> Option<BTreeSet<String>> {
        self.selected = Some(path.to_path_buf());
        // Read before decoding, so a save rewritten *during* the decode is
        // seen as changed on the next poll rather than being recorded as
        // already-read. The other order loses that write silently.
        self.attempted = file_sig(path);
        self.changed_on_disk = false;
        match from_save::unlocked_from_save(path) {
            Ok(set) => {
                self.status = Some(Ok(set.len()));
                self.imported = Some(set.clone());
                Some(set)
            }
            Err(e) => {
                self.status = Some(Err(e.to_string()));
                None
            }
        }
    }

    /// Adopt the selected save again when it has changed on disk — Factorio
    /// autosaves every few minutes, and the availability decoded at import
    /// otherwise goes quietly stale for the rest of a play session.
    ///
    /// Returns the new set only when taking it silently costs nothing: the
    /// caller's `current` set must still be exactly what the last import
    /// produced. A hand edit outranks an automatic reload — the tick list
    /// exists to *correct* an import, so overwriting a correction with no
    /// interaction and nothing on screen would be the one way this feature
    /// could destroy user input. In that case, and when the decode fails
    /// (a half-written zip, most likely), `changed_on_disk` is raised instead
    /// and the user gets a Reload button.
    ///
    /// Cheap enough to call every frame: an unchanged file costs one `stat`,
    /// and the expensive path is gated behind a signature that has actually
    /// moved.
    pub fn poll(&mut self, current: Option<&BTreeSet<String>>) -> Option<BTreeSet<String>> {
        let path = self.selected.clone()?;
        // Unreadable *right now* — mid-rename, or on a mount that went away.
        // Nothing to decide yet; the next poll will see it.
        let sig = file_sig(&path)?;
        if Some(sig) == self.attempted {
            return None;
        }

        if current != self.imported.as_ref() {
            // Deliberately does not record `sig`: the moment the user reverts
            // the edit, this same change becomes adoptable again.
            self.changed_on_disk = true;
            return None;
        }

        self.attempted = Some(sig);
        match from_save::unlocked_from_save(&path) {
            Ok(set) => {
                self.status = Some(Ok(set.len()));
                self.imported = Some(set.clone());
                self.changed_on_disk = false;
                Some(set)
            }
            Err(_) => {
                // `status` is left alone: the set the panel is still using
                // came from the previous successful read and its count is
                // still true of it. Overwriting it with this error would
                // describe a set that is not in use. The flag is the trace —
                // silence here would leave availability stale with nothing on
                // screen to say so.
                self.changed_on_disk = true;
                None
            }
        }
    }

    /// Forget the selection and its status line together, so the two cannot
    /// drift apart. The caller decides what happens to the availability set
    /// itself — clearing the picker does not throw away recipes the user may
    /// since have ticked by hand.
    pub fn clear(&mut self) {
        self.selected = None;
        self.status = None;
        self.attempted = None;
        self.imported = None;
        self.changed_on_disk = false;
    }
}

fn scan_default_dir() -> Vec<SaveEntry> {
    default_saves_dir().map(|dir| scan_saves(&dir)).unwrap_or_default()
}

/// A synthetic save whose unlocked set is every default-enabled recipe plus
/// `extra`. The defaults are not optional — they *are* the calibration
/// invariant `factorio-save` searches with — so `extra` is the only free
/// variable a test has. Lives here rather than in `mod tests` below because
/// `render_tests` drives the same fixture through real frames.
#[cfg(test)]
pub(super) fn fixture_save(extra: &[&str]) -> Vec<u8> {
    let all: Vec<String> = factorio_solver::recipe::registry().keys().cloned().collect();
    let all_refs: Vec<&str> = all.iter().map(String::as_str).collect();

    let mut unlocked: BTreeSet<String> = from_save::default_enabled().into_iter().collect();
    unlocked.extend(extra.iter().map(|s| s.to_string()));
    let unlocked_refs: Vec<&str> = unlocked.iter().map(String::as_str).collect();

    factorio_save::testsupport::FixtureSave::new()
        .with_recipes(&all_refs)
        .with_unlocked(&unlocked_refs)
        .build()
}

/// Write `bytes` and stamp an explicit mtime. Two writes a few milliseconds
/// apart can share a timestamp — mtime granularity is a filesystem property —
/// so change detection tested against write order alone is a coin flip.
#[cfg(test)]
pub(super) fn write_at(path: &Path, bytes: &[u8], secs_since_epoch: u64) {
    fs::write(path, bytes).unwrap();
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs_since_epoch))
        .unwrap();
}

#[cfg(test)]
#[path = "save_picker_tests.rs"]
mod tests;
