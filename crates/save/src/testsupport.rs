// Builds synthetic save zips for tests — no real Factorio save is available.
use std::io::{Cursor, Write};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

/// A run of all-`1` bytes used by `with_duplicate_satisfying_alignment`,
/// generously longer than any (stride, offset) search window this crate's
/// calibration code will ever use — every offset inside it decodes as
/// "every recipe enabled" at every stride 5..=8.
const DUPLICATE_RUN_LEN: usize = 4096;

/// Extra `level.dat<N>` entries used to pad the stream to a chosen chunk
/// count. Content is irrelevant filler; only the entry count matters.
const PADDING_CHUNK_LEN: usize = 256;

/// Builds a synthetic save zip byte-for-byte, so tests never need a real
/// Factorio save (which cannot be committed — see the design doc). Every
/// filler byte this builder emits is restricted to `2..=255`: never `0` or
/// `1` (so it can never masquerade as a calibration flag) and never `0x01`
/// (so it can never masquerade as the start of the `01 06 "player"` force
/// marker). That single invariant is what keeps every fixture free of
/// accidental extra calibration candidates except the one fixture whose whole
/// point is to manufacture them.
pub struct FixtureSave {
    version: (u16, u16, u16),
    mods: Vec<String>,
    recipes: Vec<String>,
    technologies: Vec<String>,
    unlocked: Option<Vec<String>>,
    stride: usize,
    force_padding: usize,
    chunk_count: usize,
    include_level_init: bool,
    include_player_force: bool,
    corrupt_metadata_total: bool,
    duplicate_satisfying_alignment: bool,
}

impl FixtureSave {
    pub fn new() -> Self {
        Self {
            version: (2, 0, 77),
            mods: Vec::new(),
            // At least one recipe by default so a caller can exercise
            // `unlocked_recipes(&set(&["iron-plate"]))` without first
            // configuring a recipe table.
            recipes: vec!["iron-plate".to_string()],
            technologies: Vec::new(),
            unlocked: None,
            stride: 6,
            force_padding: 24,
            chunk_count: 1,
            include_level_init: true,
            include_player_force: true,
            corrupt_metadata_total: false,
            duplicate_satisfying_alignment: false,
        }
    }

    pub fn with_version(mut self, major: u16, minor: u16, patch: u16) -> Self {
        self.version = (major, minor, patch);
        self
    }

    pub fn with_mods(mut self, names: &[&str]) -> Self {
        self.mods = names.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Ids are assigned `1..=n` in list order, matching level-init.dat's own
    /// per-category numbering.
    pub fn with_recipes(mut self, names: &[&str]) -> Self {
        self.recipes = names.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_technologies(mut self, names: &[&str]) -> Self {
        self.technologies = names.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Narrows which recipes decode as unlocked in the force array. Without
    /// this call, every recipe in `with_recipes` is unlocked.
    pub fn with_unlocked(mut self, names: &[&str]) -> Self {
        self.unlocked = Some(names.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn with_stride(mut self, stride: usize) -> Self {
        self.stride = stride;
        self
    }

    pub fn with_force_padding(mut self, bytes: usize) -> Self {
        self.force_padding = bytes;
        self
    }

    pub fn with_chunk_count(mut self, n: usize) -> Self {
        self.chunk_count = n.max(1);
        self
    }

    pub fn without_level_init(mut self) -> Self {
        self.include_level_init = false;
        self
    }

    pub fn without_player_force(mut self) -> Self {
        self.include_player_force = false;
        self
    }

    pub fn with_corrupt_metadata_total(mut self) -> Self {
        self.corrupt_metadata_total = true;
        self
    }

    pub fn with_duplicate_satisfying_alignment(mut self) -> Self {
        self.duplicate_satisfying_alignment = true;
        self
    }

    pub fn build(self) -> Vec<u8> {
        let level_dat_chunks = self.build_level_dat_chunks();
        let declared_total = self.declared_total(&level_dat_chunks);

        let mut buf = Vec::new();
        {
            let mut w = ZipWriter::new(Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

            if self.include_level_init {
                write_entry(&mut w, opts, "level-init.dat", &self.build_level_init());
            }
            write_entry(&mut w, opts, "level.datmetadata", &declared_total.to_le_bytes());

            for (i, chunk) in level_dat_chunks.iter().enumerate() {
                let compressed = zlib_compress(chunk);
                write_entry(&mut w, opts, &format!("level.dat{i}"), &compressed);
            }

            w.finish().expect("in-memory zip write cannot fail");
        }
        buf
    }

    fn declared_total(&self, chunks: &[Vec<u8>]) -> u64 {
        let actual: u64 = chunks.iter().map(|c| c.len() as u64).sum();
        if self.corrupt_metadata_total { actual + 9_999 } else { actual }
    }

    /// `level-init.dat`: version header, mod list, then the recipe and
    /// technology category tables (see `init.rs` for the exact format this
    /// mirrors).
    fn build_level_init(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.version.0.to_le_bytes());
        buf.extend_from_slice(&self.version.1.to_le_bytes());
        buf.extend_from_slice(&self.version.2.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // developer/build field, discarded on read

        buf.extend_from_slice(&(self.mods.len() as u16).to_le_bytes());
        for m in &self.mods {
            buf.push(m.len() as u8);
            buf.extend_from_slice(m.as_bytes());
        }

        write_category_table(&mut buf, "recipe", &self.recipes);
        write_category_table(&mut buf, "technology", &self.technologies);
        buf
    }

    /// The uncompressed `level.dat` stream, split into `chunk_count` pieces.
    /// The force marker and record array always live in chunk 0 — later
    /// chunks are pure filler — so a test can assert a load only inflates the
    /// leading chunks it actually needs.
    fn build_level_dat_chunks(&self) -> Vec<Vec<u8>> {
        let mut chunks = vec![self.build_level_dat_core()];
        for i in 1..self.chunk_count {
            chunks.push(filler(i * 7, PADDING_CHUNK_LEN));
        }
        chunks
    }

    fn build_level_dat_core(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Stand-in for unrelated save data preceding the player force (other
        // forces, surfaces, etc.).
        buf.extend_from_slice(&filler(0, 16));

        if self.include_player_force {
            buf.push(0x01);
            buf.push(0x06);
            buf.extend_from_slice(b"player");

            if self.duplicate_satisfying_alignment {
                buf.extend(std::iter::repeat_n(1u8, DUPLICATE_RUN_LEN));
            } else {
                buf.extend_from_slice(&filler(buf.len(), self.force_padding));
                self.write_record_array(&mut buf);
            }
        }

        buf.extend_from_slice(&filler(buf.len(), 64)); // trailing filler
        buf
    }

    /// `recipes().len() + 1` fixed-`stride` records, one per recipe id
    /// (id 0 is the unnamed `recipe-unknown` sentinel). Byte 0 of each record
    /// is the unlocked flag (`1`/`0`); the rest is filler.
    fn write_record_array(&self, buf: &mut Vec<u8>) {
        let unlocked = self.unlocked.as_ref().unwrap_or(&self.recipes);
        for id in 0..=self.recipes.len() {
            let flag = match id {
                0 => 0, // sentinel: no name, value is arbitrary but must be 0/1
                _ => u8::from(unlocked.iter().any(|n| n == &self.recipes[id - 1])),
            };
            buf.push(flag);
            buf.extend_from_slice(&filler(buf.len(), self.stride - 1));
        }
    }
}

impl Default for FixtureSave {
    fn default() -> Self {
        Self::new()
    }
}

fn write_category_table(buf: &mut Vec<u8>, category: &str, names: &[String]) {
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
/// struct-level doc comment for why that range is load-bearing.
fn filler(start: usize, n: usize) -> Vec<u8> {
    (0..n).map(|i| 2 + ((start + i) % 254) as u8).collect()
}

fn zlib_compress(bytes: &[u8]) -> Vec<u8> {
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).expect("in-memory zlib write cannot fail");
    enc.finish().expect("in-memory zlib finish cannot fail")
}

fn write_entry<W: std::io::Write + std::io::Seek>(
    w: &mut ZipWriter<W>,
    opts: SimpleFileOptions,
    name: &str,
    bytes: &[u8],
) {
    w.start_file(name, opts).expect("in-memory zip start_file cannot fail");
    w.write_all(bytes).expect("in-memory zip write cannot fail");
}
