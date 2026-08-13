// Unit tests for SaveFile: container reads, id tables, and lazy inflation.
use super::*;
use crate::testsupport::FixtureSave;
#[test]
fn parses_version_and_mod_names() {
    let zip = FixtureSave::new().with_version(2, 0, 77).with_mods(&["base", "space-age"]).build();
    let s = SaveFile::open_bytes(&zip).unwrap();
    assert_eq!(s.version(), Version { major: 2, minor: 0, patch: 77 });
    assert_eq!(s.mods(), &["base".to_string(), "space-age".to_string()]);
}

#[test]
fn parses_recipe_and_technology_id_tables() {
    let zip = FixtureSave::new()
        .with_recipes(&["iron-plate", "copper-plate", "electronic-circuit"])
        .with_technologies(&["automation", "electronics"])
        .build();
    let s = SaveFile::open_bytes(&zip).unwrap();
    assert_eq!(s.recipes().len(), 3);
    assert_eq!(s.recipes().name(1), Some("iron-plate"));
    assert_eq!(s.technologies().len(), 2);
}

#[test]
fn chunks_are_ordered_numerically_not_lexicographically() {
    // 12 chunks forces the level.dat10 / level.dat2 ambiguity.
    let zip = FixtureSave::new().with_chunk_count(12).build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    s.verify_total_size().expect("chunks reassemble in numeric order");
}

#[test]
fn open_inflates_no_chunks() {
    let zip = FixtureSave::new().with_chunk_count(8).build();
    let s = SaveFile::open_bytes(&zip).unwrap();
    assert_eq!(s.inflated_chunk_count(), 0);
}

#[test]
fn size_mismatch_is_an_error() {
    let zip = FixtureSave::new().with_corrupt_metadata_total().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    assert!(matches!(s.verify_total_size(), Err(SaveError::SizeMismatch { .. })));
}

#[test]
fn missing_level_init_is_an_error() {
    let zip = FixtureSave::new().without_level_init().build();
    assert!(matches!(SaveFile::open_bytes(&zip), Err(SaveError::MissingEntry { .. })));
}

#[test]
fn without_player_force_omits_the_marker() {
    let zip = FixtureSave::new().without_player_force().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    s.verify_total_size().unwrap();
    assert!(
        !s.inflated.windows(8).any(|w| w == b"\x01\x06player"),
        "the force marker must not appear anywhere in the stream"
    );
}

#[test]
fn duplicate_alignment_fixture_yields_a_long_all_enabled_run() {
    // Sanity check on the fixture itself (not a calibration search, which
    // a later task adds): right after the force marker there must be a
    // long run of `1` bytes, since that run is what will make more than
    // one (stride, offset) satisfy the "everything enabled" invariant.
    let zip = FixtureSave::new().with_duplicate_satisfying_alignment().build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    s.verify_total_size().unwrap();
    let marker = b"\x01\x06player";
    let marker_at = s.inflated.windows(marker.len()).position(|w| w == marker).unwrap();
    let after = &s.inflated[marker_at + marker.len()..];
    let run_len = after.iter().take_while(|&&b| b == 1).count();
    assert!(run_len >= 512, "expected a long run of enabled flags, got {run_len}");
}

#[test]
fn open_bytes_works_from_a_real_path_too() {
    // A fresh directory, not a name built from the pid: a predictable path in
    // a shared /tmp is one another process can pre-create as a symlink, and
    // `fs::write` would follow it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.zip");
    std::fs::write(&path, FixtureSave::new().build()).unwrap();
    let s = SaveFile::open(&path).unwrap();
    assert_eq!(s.version(), Version { major: 2, minor: 0, patch: 77 });
}

#[test]
fn ensure_inflated_stops_once_the_target_is_reached() {
    let zip = FixtureSave::new().with_chunk_count(20).build();
    let mut s = SaveFile::open_bytes(&zip).unwrap();
    s.ensure_inflated(1).unwrap();
    let after_first = s.inflated_chunk_count();
    assert!(after_first >= 1, "must inflate at least one chunk to reach a 1-byte target");
    assert!(after_first < 20, "must not inflate every chunk for a tiny target");
}

/// A minimal but valid level-init.dat: version header, no mods, and
/// empty recipe/technology tables. Used by tests below that need
/// `init::parse` to succeed so a *later* step (the metadata read, or
/// chunk inflation) is what fails.
fn minimal_level_init() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&2u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&77u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // 0 mods
    buf.push(6);
    buf.extend_from_slice(b"recipe");
    buf.extend_from_slice(&0u16.to_le_bytes()); // 0 recipes
    buf.push(10);
    buf.extend_from_slice(b"technology");
    buf.extend_from_slice(&0u16.to_le_bytes()); // 0 technologies
    buf
}

fn write_stored_entry<W: std::io::Write + std::io::Seek>(
    w: &mut zip::ZipWriter<W>,
    name: &str,
    bytes: &[u8],
) {
    use std::io::Write as _;
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    w.start_file(name, opts).unwrap();
    w.write_all(bytes).unwrap();
}

#[test]
fn a_corrupt_chunk_reports_decompress_error() {
    // A chunk that is valid as far as the zip container is concerned
    // (correct CRC for its own bytes, since zip computes that from
    // whatever we hand it) but is not a valid zlib stream at the
    // Factorio layer this crate decodes — that layer's failure must
    // surface as `Decompress`, not bubble up as a generic zip/io error.
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        write_stored_entry(&mut w, "level-init.dat", &minimal_level_init());
        write_stored_entry(&mut w, "level.datmetadata", &0u64.to_le_bytes());
        write_stored_entry(&mut w, "level.dat0", b"not a valid zlib stream");
        w.finish().unwrap();
    }
    let mut s = SaveFile::open_bytes(&buf).unwrap();
    let err = s.verify_total_size().unwrap_err();
    assert!(matches!(err, SaveError::Decompress { .. }), "{err:?}");
}

#[test]
fn missing_metadata_is_an_error() {
    // level-init.dat alone is not enough — level.datmetadata absent.
    let mut buf = Vec::new();
    {
        let mut w = zip::ZipWriter::new(Cursor::new(&mut buf));
        write_stored_entry(&mut w, "level-init.dat", &minimal_level_init());
        w.finish().unwrap();
    }
    assert!(matches!(SaveFile::open_bytes(&buf), Err(SaveError::MissingEntry { .. })));
}
