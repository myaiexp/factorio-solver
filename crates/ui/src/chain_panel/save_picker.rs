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
    /// happens once, on selection: `build_goal` runs on every Solve click and
    /// re-inflating a save there would be work for an answer that cannot have
    /// changed.
    pub status: Option<Result<usize, String>>,
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
        match from_save::unlocked_from_save(path) {
            Ok(set) => {
                self.status = Some(Ok(set.len()));
                Some(set)
            }
            Err(e) => {
                self.status = Some(Err(e.to_string()));
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
    }
}

fn scan_default_dir() -> Vec<SaveEntry> {
    default_saves_dir().map(|dir| scan_saves(&dir)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn write_zip(dir: &Path, stem: &str) {
        // Contents are irrelevant to scan_saves, which only reads directory
        // metadata — a real zip is only needed once decoding is exercised.
        fs::write(dir.join(format!("{stem}.zip")), b"placeholder").unwrap();
    }

    /// Stamps an explicit mtime rather than relying on write order plus a
    /// sleep: mtime granularity is a filesystem property (a second on some,
    /// nanoseconds on ext4), so writes ordered by a short sleep can land on
    /// the same timestamp and make the sort order a coin flip.
    fn write_zip_at(dir: &Path, stem: &str, secs_since_epoch: u64) {
        write_zip(dir, stem);
        let path = dir.join(format!("{stem}.zip"));
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs_since_epoch))
            .unwrap();
    }

    #[test]
    fn scan_returns_saves_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        // Written in an order that does *not* match the expected result, so
        // the assertion cannot pass on directory order alone.
        write_zip_at(dir.path(), "middle", 1_700_000_100);
        write_zip_at(dir.path(), "newest", 1_700_000_200);
        write_zip_at(dir.path(), "oldest", 1_700_000_000);

        let entries = scan_saves(dir.path());
        assert_eq!(
            entries.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(),
            vec!["newest", "middle", "oldest"]
        );
    }

    #[test]
    fn scan_of_a_missing_directory_is_empty_not_an_error() {
        assert!(scan_saves(Path::new("/nonexistent")).is_empty());
    }

    #[test]
    fn scan_ignores_non_zip_files() {
        let dir = tempfile::tempdir().unwrap();
        write_zip(dir.path(), "a");
        fs::write(dir.path().join("notes.txt"), b"hi").unwrap();
        fs::create_dir(dir.path().join("b")).unwrap();

        let entries = scan_saves(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "a");
    }

    #[test]
    fn default_saves_dir_does_not_panic_and_points_under_dot_factorio() {
        // $HOME is set in this environment (there is no ~/.factorio, but
        // there is a home directory), so this exercises the real path rather
        // than only the None branch.
        if let Some(dir) = default_saves_dir() {
            assert!(dir.ends_with("saves"), "{dir:?}");
            assert!(dir.to_string_lossy().contains(".factorio"), "{dir:?}");
        }
    }

    #[test]
    fn a_failed_load_surfaces_the_error_and_adopts_nothing() {
        let mut state = SavePickerState::default();
        assert!(
            state.select(Path::new("/nonexistent/save.zip")).is_none(),
            "a failed read must hand back no set, so the caller leaves the existing one alone"
        );
        assert!(matches!(state.status, Some(Err(_))));
    }

    /// End to end over a synthetic save whose recipe table is the *real*
    /// registry, mirroring `factorio_solver::availability::from_save`'s own
    /// round-trip test — the fixture must carry every `default_enabled()`
    /// name as unlocked or calibration fails by design.
    #[test]
    fn a_successful_load_returns_the_unlocked_set_and_the_count() {
        let all: Vec<String> = factorio_solver::recipe::registry().keys().cloned().collect();
        let all_refs: Vec<&str> = all.iter().map(String::as_str).collect();

        let mut expected: BTreeSet<String> =
            from_save::default_enabled().into_iter().collect();
        expected.insert("beacon".to_string());
        let unlocked: Vec<&str> = expected.iter().map(String::as_str).collect();

        let zip = factorio_save::testsupport::FixtureSave::new()
            .with_recipes(&all_refs)
            .with_unlocked(&unlocked)
            .build();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("save.zip");
        fs::write(&path, zip).unwrap();

        let mut state = SavePickerState::default();
        let got = state.select(&path).expect("the real registry must calibrate uniquely");

        assert_eq!(state.selected.as_deref(), Some(path.as_path()));
        assert_eq!(state.status, Some(Ok(expected.len())));
        assert_eq!(got, expected);
    }

    #[test]
    fn clearing_forgets_the_selection_and_the_status_together() {
        let mut state = SavePickerState {
            entries: Vec::new(),
            selected: Some(PathBuf::from("/some/save.zip")),
            manual_path: String::new(),
            status: Some(Ok(1)),
        };

        state.clear();

        assert!(state.selected.is_none());
        assert!(state.status.is_none());
    }
}
