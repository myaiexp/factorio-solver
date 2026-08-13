// Tests for the calibration search and unlock decode. Split out of
// `force.rs` to keep it under the file-length limit.
use super::*;
use crate::SaveError;
use crate::testsupport::FixtureSave;

fn set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn decodes_unlocked_recipes_at_a_shifted_offset_and_stride() {
    // Offset and stride are both non-default — a hardcoded pair must fail this.
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "copper-plate", "advanced-circuit", "beacon"])
        .with_unlocked(&["iron-plate", "copper-plate"])
        .with_stride(7)
        .with_force_padding(113)
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let defaults = set(&["iron-plate", "copper-plate"]);
    let on = s.unlocked_recipes(&defaults).unwrap();
    assert_eq!(on, set(&["iron-plate", "copper-plate"]));
    assert_eq!(s.calibration().unwrap().stride, 7);
}

#[test]
fn rejects_an_alignment_off_by_exactly_one_record() {
    // The trap from the investigation: a one-record shift still looks plausible
    // under a weak check. The invariant must reject it, and the decode must be exact.
    let zip = FixtureSave::new()
        .with_recipes(&["a-plate", "b-plate", "c-plate", "d-plate", "e-plate"])
        .with_unlocked(&["a-plate", "b-plate", "c-plate"])
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let on = s.unlocked_recipes(&set(&["a-plate", "b-plate", "c-plate"])).unwrap();
    assert_eq!(on, set(&["a-plate", "b-plate", "c-plate"]));
    assert!(!on.contains("d-plate"), "off-by-one would leak the next recipe in");
}

#[test]
fn zero_candidates_is_a_calibration_error_naming_the_version() {
    // default_enabled names a recipe the save has locked — no alignment can satisfy it.
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "beacon"])
        .with_unlocked(&["iron-plate"])
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    match s.unlocked_recipes(&set(&["iron-plate", "beacon"])) {
        Err(SaveError::CalibrationFailed { version, .. }) => {
            assert_eq!(version.major, 2);
        }
        other => panic!("expected CalibrationFailed, got {other:?}"),
    }
}

#[test]
fn multiple_candidates_are_refused_never_guessed() {
    let zip = FixtureSave::new().with_duplicate_satisfying_alignment().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    assert!(matches!(
        s.unlocked_recipes(&set(&["iron-plate"])),
        Err(SaveError::CalibrationAmbiguous { .. })
    ));
}

#[test]
fn reads_only_the_chunks_it_needs() {
    // Guards the lazy-inflation optimisation against silent regression.
    let zip = FixtureSave::new().with_chunk_count(40).build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    s.unlocked_recipes(&set(&["iron-plate"])).unwrap();
    assert!(s.inflated_chunk_count() < 40, "must not inflate the whole stream");
}

#[test]
fn force_not_found_is_an_error() {
    let zip = FixtureSave::new().without_player_force().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    assert!(matches!(
        s.unlocked_recipes(&set(&["iron-plate"])),
        Err(SaveError::ForceNotFound)
    ));
}

#[test]
fn calibration_is_none_before_the_first_call_and_some_after() {
    let zip = FixtureSave::new().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    assert_eq!(s.calibration(), None);
    s.unlocked_recipes(&set(&["iron-plate"])).unwrap();
    // Default fixture: stride 6, force_padding 24 (see testsupport.rs).
    assert_eq!(s.calibration(), Some(Calibration { stride: 6, offset: s.calibration().unwrap().offset }));
    assert!(s.calibration().unwrap().offset > 0);
}

#[test]
fn a_second_call_returns_the_same_answer_without_inflating_more() {
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "copper-plate"])
        .with_unlocked(&["iron-plate"])
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let first = s.unlocked_recipes(&set(&["iron-plate"])).unwrap();
    let chunks_after_first = s.inflated_chunk_count();
    let cal_after_first = s.calibration();

    let second = s.unlocked_recipes(&set(&["iron-plate"])).unwrap();

    assert_eq!(first, second);
    assert_eq!(s.calibration(), cal_after_first, "the accepted calibration must not change");
    assert_eq!(
        s.inflated_chunk_count(),
        chunks_after_first,
        "a cached calibration must not re-run the search or inflate further chunks"
    );
}

#[test]
fn default_enabled_naming_a_recipe_absent_from_the_table_is_intersected_not_failed() {
    // "phantom-recipe" names nothing in this save's table, so it cannot be
    // checked and must not block calibration — only the intersection with
    // recipes().names() is the invariant.
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "copper-plate"])
        .with_unlocked(&["iron-plate", "copper-plate"])
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let on = s.unlocked_recipes(&set(&["iron-plate", "copper-plate", "phantom-recipe"])).unwrap();
    assert_eq!(on, set(&["iron-plate", "copper-plate"]));
}

#[test]
fn the_decoded_set_excludes_the_id_zero_sentinel() {
    let zip = FixtureSave::new().with_recipes(&["iron-plate"]).build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let on = s.unlocked_recipes(&set(&["iron-plate"])).unwrap();
    // Only the one named recipe — the sentinel record (id 0, no name) must
    // never contribute an entry, no matter its flag value.
    assert_eq!(on, set(&["iron-plate"]));
    assert_eq!(on.len(), 1);
}

#[test]
fn a_locked_recipe_is_genuinely_absent_from_the_returned_set() {
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "beacon"])
        .with_unlocked(&["iron-plate"])
        .build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    let on = s.unlocked_recipes(&set(&["iron-plate"])).unwrap();
    assert!(on.contains("iron-plate"));
    assert!(!on.contains("beacon"), "beacon was never unlocked in this fixture");
}
