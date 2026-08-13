// Tests for the save picker: scanning `~/.factorio/saves`, decoding a
// selection, and adopting one that changed on disk.
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
    let mut expected: BTreeSet<String> = from_save::default_enabled().into_iter().collect();
    expected.insert("beacon".to_string());
    let zip = fixture_save(&["beacon"]);

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
        attempted: None,
        imported: Some(BTreeSet::new()),
        changed_on_disk: true,
    };

    state.clear();

    assert!(state.selected.is_none());
    assert!(state.status.is_none());
    assert!(state.imported.is_none(), "and the reload bookkeeping with them");
    assert!(!state.changed_on_disk);
}

// ── Reloading a save that changed on disk ──────────────────────────

/// The whole feature: research something, the game autosaves, and the
/// panel's availability follows without the player alt-tabbing to click
/// anything.
#[test]
fn a_save_that_changed_on_disk_is_adopted_when_nothing_would_be_lost() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.zip");
    write_at(&path, &fixture_save(&[]), 1_700_000_000);

    let mut state = SavePickerState::default();
    let first = state.select(&path).expect("the real registry calibrates uniquely");
    assert!(!first.contains("beacon"));

    // Nothing has changed yet, so a poll must do nothing at all.
    assert_eq!(state.poll(Some(&first)), None);

    write_at(&path, &fixture_save(&["beacon"]), 1_700_000_100);
    let second = state.poll(Some(&first)).expect("the change is adopted");
    assert!(second.contains("beacon"));
    assert_eq!(state.status, Some(Ok(second.len())));
    assert!(!state.changed_on_disk, "an adopted change is not a pending one");

    // And having adopted it, the same file must not be read again.
    assert_eq!(state.poll(Some(&second)), None);
}

/// The rule that keeps this from being a data-loss bug. The tick list
/// exists to correct an import; a background reload that overwrote a
/// correction — with no interaction and nothing on screen — would undo
/// the user's work every few minutes.
#[test]
fn a_hand_edited_set_is_never_overwritten_silently() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.zip");
    write_at(&path, &fixture_save(&[]), 1_700_000_000);

    let mut state = SavePickerState::default();
    let imported = state.select(&path).unwrap();

    // The user unticks something the save says is unlocked.
    let mut edited = imported.clone();
    let removed = edited.iter().next().cloned().unwrap();
    edited.remove(&removed);

    write_at(&path, &fixture_save(&["beacon"]), 1_700_000_100);
    assert_eq!(state.poll(Some(&edited)), None, "the edit stands");
    assert!(state.changed_on_disk, "but the user is told there is something to take");

    // Reverting the edit makes the same change adoptable — the pending
    // change is not consumed by having been declined once.
    let adopted = state.poll(Some(&imported)).expect("adoptable again");
    assert!(adopted.contains("beacon"));
    assert!(!state.changed_on_disk);
}

/// A save caught mid-write decodes to nothing usable. The previous set is
/// still the one in use, so its status line must survive — but staying
/// silent would leave availability stale with nothing on screen.
#[test]
fn a_failed_reload_keeps_the_old_status_and_raises_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.zip");
    write_at(&path, &fixture_save(&[]), 1_700_000_000);

    let mut state = SavePickerState::default();
    let imported = state.select(&path).unwrap();
    let good_status = state.status.clone();

    write_at(&path, b"half a zip", 1_700_000_100);
    assert_eq!(state.poll(Some(&imported)), None);
    assert_eq!(state.status, good_status, "the count still describes the set in use");
    assert!(state.changed_on_disk);

    // And the broken file is read once, not once per frame: a second poll
    // against the same signature must not decode again.
    state.changed_on_disk = false;
    assert_eq!(state.poll(Some(&imported)), None);
    assert!(!state.changed_on_disk, "the same signature is not re-attempted");
}

#[test]
fn polling_without_a_selection_does_nothing() {
    let mut state = SavePickerState::default();
    assert_eq!(state.poll(None), None);
    assert!(!state.changed_on_disk);
}
