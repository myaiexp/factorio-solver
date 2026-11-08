// Byte and zip writers the `FixtureSave` builder composes: id tables, filler, zlib, zip entries.
use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use zip::write::{SimpleFileOptions, ZipWriter};

pub(super) fn write_category_table(buf: &mut Vec<u8>, category: &str, names: &[String]) {
    buf.push(category.len() as u8);
    buf.extend_from_slice(category.as_bytes());
    buf.extend_from_slice(&(names.len() as u16).to_le_bytes());
    for (i, name) in names.iter().enumerate() {
        let id = (i + 1) as u16;
        buf.push(name.len() as u8);
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(&id.to_le_bytes());
    }
}

/// `n` bytes in `2..=255`, seeded by `start` so different call sites don't
/// all emit the same repeating byte. Never `0`/`1` (a calibration flag value)
/// and never `0x01` (the first byte of the force marker) — see the
/// `FixtureSave` doc comment for why that range is load-bearing.
pub(super) fn filler(start: usize, n: usize) -> Vec<u8> {
    (0..n).map(|i| 2 + ((start + i) % 254) as u8).collect()
}

pub(super) fn zlib_compress(bytes: &[u8]) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).expect("in-memory zlib write cannot fail");
    enc.finish().expect("in-memory zlib finish cannot fail")
}

pub(super) fn write_entry<W: std::io::Write + std::io::Seek>(
    w: &mut ZipWriter<W>,
    opts: SimpleFileOptions,
    name: &str,
    bytes: &[u8],
) {
    w.start_file(name, opts).expect("in-memory zip start_file cannot fail");
    w.write_all(bytes).expect("in-memory zip write cannot fail");
}
