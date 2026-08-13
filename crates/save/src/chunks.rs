// Zip entry discovery and lazy level.dat<N> chunk inflation.
use std::io::{Cursor, Read};

use flate2::read::ZlibDecoder;
use zip::ZipArchive;

use crate::error::SaveError;

/// The `level.dat<N>` zip entry names, ordered by numeric `N` ascending —
/// concatenating and inflating them in this order reproduces the original
/// stream. Sorting the entry names lexicographically would put `level.dat10`
/// before `level.dat2`, silently corrupting the stream, so this always parses
/// the numeric suffix and sorts on that.
///
/// `prefix` is the save's own directory (see `SaveFile::entry_prefix`), which
/// every entry in a real save carries. It is stripped before parsing the
/// number but kept in the returned names, because those are what
/// `inflate_chunk` looks up. Matching the number against the *full* path
/// instead would find nothing in any real save — and silently, since an empty
/// chunk list reads as "this save has no data" rather than as an error.
pub(crate) fn ordered_chunk_names(
    archive: &ZipArchive<Cursor<Vec<u8>>>,
    prefix: &str,
) -> Vec<String> {
    let mut chunks: Vec<(u64, String)> = archive
        .file_names()
        .filter_map(|name| {
            let bare = name.strip_prefix(prefix)?;
            chunk_number(bare).map(|n| (n, name.to_string()))
        })
        .collect();
    chunks.sort_by_key(|(n, _)| *n);
    chunks.into_iter().map(|(_, name)| name).collect()
}

/// `level.dat<N>` where `<N>` is one or more ASCII digits. Deliberately
/// excludes `level.datmetadata` (its suffix, "metadata", is not numeric) and
/// `level-init.dat` (a different prefix — dash, not dot).
fn chunk_number(name: &str) -> Option<u64> {
    let suffix = name.strip_prefix("level.dat")?;
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

/// Inflate one chunk. A `level.dat<N>` entry is stored uncompressed at the
/// zip level ("Stored") but the bytes inside it are independently
/// zlib-compressed by Factorio itself — a separate compression layer from the
/// zip container's own, which this function undoes.
pub(crate) fn inflate_chunk(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>, SaveError> {
    // Same rule as `read_entry`: only a genuine absence is `MissingEntry`.
    // Any other `ZipError` is a different fault and has to say so rather than
    // be reported as a file that is not there.
    let mut compressed = Vec::new();
    match archive.by_name(name) {
        Ok(mut entry) => entry.read_to_end(&mut compressed)?,
        Err(zip::result::ZipError::FileNotFound) => {
            return Err(SaveError::MissingEntry { name: name.to_string() });
        }
        Err(e) => return Err(SaveError::Zip(e)),
    };

    let mut out = Vec::new();
    ZlibDecoder::new(&compressed[..])
        .read_to_end(&mut out)
        .map_err(|_| SaveError::Decompress { chunk: name.to_string() })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_number_parses_the_numeric_suffix() {
        assert_eq!(chunk_number("level.dat0"), Some(0));
        assert_eq!(chunk_number("level.dat10"), Some(10));
        assert_eq!(chunk_number("level.dat2"), Some(2));
    }

    #[test]
    fn chunk_number_rejects_non_chunk_entries() {
        assert_eq!(chunk_number("level.datmetadata"), None);
        assert_eq!(chunk_number("level-init.dat"), None);
        assert_eq!(chunk_number("level.dat"), None);
        assert_eq!(chunk_number("level.dat12x"), None);
    }

    #[test]
    fn numeric_sort_beats_lexicographic_sort() {
        // The whole point: as strings "level.dat10" < "level.dat2". Numeric
        // ordering must put chunk 2 first.
        let mut names = vec!["level.dat10".to_string(), "level.dat2".to_string()];
        names.sort_by_key(|n| chunk_number(n).unwrap());
        assert_eq!(names, vec!["level.dat2".to_string(), "level.dat10".to_string()]);
    }
}
