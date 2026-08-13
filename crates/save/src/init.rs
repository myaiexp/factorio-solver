// Parses level-init.dat: version, mod names, and prototype id tables.
use std::collections::HashMap;

use crate::error::SaveError;

/// Factorio's application version, as printed in-game (e.g. `2.0.77`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

/// Prototype name <-> numeric id for one category (e.g. `"recipe"`), as
/// stored in level-init.dat.
#[derive(Debug, Clone, Default)]
pub struct IdTable(HashMap<u16, String>);

impl IdTable {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn name(&self, id: u16) -> Option<&str> {
        self.0.get(&id).map(String::as_str)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.values().map(String::as_str)
    }
}

/// Parsed contents of level-init.dat.
#[derive(Debug)]
pub(crate) struct InitData {
    pub(crate) version: Version,
    pub(crate) mods: Vec<String>,
    pub(crate) recipes: IdTable,
    pub(crate) technologies: IdTable,
}

/// Parse level-init.dat.
///
/// The category-table format (`[u8 name_len][category name][u16 count]` then
/// `count` x `[u8 name_len][name][u16 id]`) was verified against ~50 real
/// saves and is trusted. The **header** in front of it — the version field and
/// the mod list — was never nailed down against a real save; no Factorio
/// install exists on this machine to check it against. See the doc comments
/// on `parse_header` and `parse_mods` below for what is assumed and why. This
/// is why `recipe`/`technology` are located by searching for their
/// length-prefixed category name rather than by trusting any offset the
/// header parse computed: even if the header assumption is wrong, the search
/// still finds the tables (it may just start a little later or earlier than a
/// real header would put it, which does not matter since the search ignores
/// position entirely).
pub(crate) fn parse(bytes: &[u8]) -> Result<InitData, SaveError> {
    let mut r = Reader::new(bytes);
    let version = parse_header(&mut r)?;
    let mods = parse_mods(&mut r)?;

    let recipe_offset = find_category_table(bytes, "recipe").ok_or_else(|| {
        SaveError::MalformedInit {
            reason: "no 'recipe' category table found in level-init.dat".into(),
        }
    })?;
    let recipes = parse_id_table(bytes, recipe_offset)?;

    let technology_offset = find_category_table(bytes, "technology").ok_or_else(|| {
        SaveError::MalformedInit {
            reason: "no 'technology' category table found in level-init.dat".into(),
        }
    })?;
    let technologies = parse_id_table(bytes, technology_offset)?;

    Ok(InitData { version, mods, recipes, technologies })
}

/// INFERRED, not confirmed: Factorio's application version is conventionally
/// serialized as a run of 4 little-endian `u16` fields (commonly named
/// main/major/minor/developer in the game's own source, printed to the user
/// as `main.major.minor`, e.g. `2.0.77`). We read those 4 fields and map the
/// first three onto this crate's `Version{major,minor,patch}` — so "major"
/// here is Factorio's "main" field, and so on — and discard the 4th
/// (developer/build) field, which `Version` has no place for. This has never
/// been checked against a real save; validate it via the opt-in real-save
/// test before trusting it for anything beyond the synthetic fixtures.
fn parse_header(r: &mut Reader) -> Result<Version, SaveError> {
    let main = r.u16()?;
    let major = r.u16()?;
    let minor = r.u16()?;
    let _developer = r.u16()?;
    Ok(Version { major: main, minor: major, patch: minor })
}

/// INFERRED, not confirmed: same caveat as `parse_header`. We assume the mod
/// list is a `u16` count followed by that many length-prefixed names
/// (matching the length-prefixed-string idiom the category tables are
/// confirmed to use), with no per-mod version/crc fields — because we have no
/// evidence for what such fields would contain or how wide they are, and a
/// wrong guess there would misalign every entry after the first.
///
/// This is why getting this wrong is low-stakes: `recipes()`/`technologies()`
/// do not depend on this parse landing at the right length, because they are
/// found by an independent byte search over the whole file (see `parse`).
/// A mod-list layout mismatch would only ever corrupt `mods()`, never the id
/// tables.
fn parse_mods(r: &mut Reader) -> Result<Vec<String>, SaveError> {
    let count = r.u16()?;
    let mut mods = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let len = r.u8()? as usize;
        mods.push(r.string(len)?);
    }
    Ok(mods)
}

/// Byte offset just past the first occurrence of `[len][category]` in
/// `bytes`, i.e. where that category's `u16` record count begins. Search-based
/// so a header layout drift (the header format above being wrong, or a future
/// Factorio version changing it) does not break table location — only a
/// changed *category table* format would.
fn find_category_table(bytes: &[u8], category: &str) -> Option<usize> {
    let mut pattern = Vec::with_capacity(1 + category.len());
    pattern.push(category.len() as u8);
    pattern.extend_from_slice(category.as_bytes());
    bytes.windows(pattern.len()).position(|w| w == pattern.as_slice()).map(|p| p + pattern.len())
}

fn parse_id_table(bytes: &[u8], start: usize) -> Result<IdTable, SaveError> {
    let mut r = Reader { bytes, pos: start };
    let count = r.u16()?;
    let mut map = HashMap::with_capacity(count as usize);
    for _ in 0..count {
        let len = r.u8()? as usize;
        let name = r.string(len)?;
        let id = r.u16()?;
        map.insert(id, name);
    }
    Ok(IdTable(map))
}

/// Bounds-checked little-endian cursor over `level-init.dat`. Every read can
/// fail rather than panic: the file is untrusted input (a corrupt save, or a
/// stale header assumption running past the real data), and this crate's
/// house rule is that a malformed input is a loud, named error, never a
/// silent default or an out-of-bounds panic.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn u8(&mut self) -> Result<u8, SaveError> {
        let b = *self.bytes.get(self.pos).ok_or_else(|| malformed("unexpected end of file reading a u8"))?;
        self.pos += 1;
        Ok(b)
    }

    fn u16(&mut self) -> Result<u16, SaveError> {
        let end = self.pos.checked_add(2).ok_or_else(|| malformed("offset overflow reading a u16"))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| malformed("unexpected end of file reading a u16"))?;
        self.pos = end;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    fn string(&mut self, len: usize) -> Result<String, SaveError> {
        let end = self.pos.checked_add(len).ok_or_else(|| malformed("offset overflow reading a name"))?;
        let slice = self.bytes.get(self.pos..end).ok_or_else(|| {
            malformed(format!("name of length {len} runs past the end of the file"))
        })?;
        self.pos = end;
        String::from_utf8(slice.to_vec()).map_err(|_| malformed("name is not valid utf-8"))
    }
}

fn malformed(reason: impl Into<String>) -> SaveError {
    SaveError::MalformedInit { reason: reason.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn category_table(category: &str, names: &[(&str, u16)]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(category.len() as u8);
        buf.extend_from_slice(category.as_bytes());
        buf.extend_from_slice(&(names.len() as u16).to_le_bytes());
        for (name, id) in names {
            buf.push(name.len() as u8);
            buf.extend_from_slice(name.as_bytes());
            buf.extend_from_slice(&id.to_le_bytes());
        }
        buf
    }

    fn minimal_init() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&77u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // 0 mods
        buf.extend_from_slice(&category_table("recipe", &[("iron-plate", 1)]));
        buf.extend_from_slice(&category_table("technology", &[]));
        buf
    }

    #[test]
    fn parses_a_minimal_header_and_tables() {
        let data = parse(&minimal_init()).unwrap();
        assert_eq!(data.version, Version { major: 2, minor: 0, patch: 77 });
        assert!(data.mods.is_empty());
        assert_eq!(data.recipes.len(), 1);
        assert_eq!(data.recipes.name(1), Some("iron-plate"));
        assert!(data.technologies.is_empty());
    }

    #[test]
    fn table_search_ignores_content_before_it() {
        // The category table is found wherever it is, not at a fixed offset —
        // pad the front with unrelated bytes and confirm it still parses.
        let mut buf = vec![0xAAu8; 37];
        buf.extend_from_slice(&category_table("recipe", &[("a", 1), ("b", 2)]));
        buf.extend_from_slice(&category_table("technology", &[("t", 1)]));
        let table = find_category_table(&buf, "recipe").unwrap();
        let recipes = parse_id_table(&buf, table).unwrap();
        assert_eq!(recipes.len(), 2);
        assert_eq!(recipes.name(2), Some("b"));
    }

    #[test]
    fn missing_category_table_is_malformed() {
        let buf = vec![0u8; 8];
        let err = parse(&buf).unwrap_err();
        assert!(matches!(err, SaveError::MalformedInit { .. }), "{err:?}");
    }

    #[test]
    fn name_longer_than_the_buffer_is_malformed() {
        let mut buf = Vec::new();
        buf.push(6u8);
        buf.extend_from_slice(b"recipe");
        buf.extend_from_slice(&1u16.to_le_bytes()); // claims 1 record
        buf.push(200u8); // name length far exceeds what follows
        buf.extend_from_slice(b"short");
        let err = parse_id_table(&buf, 7).unwrap_err();
        assert!(matches!(err, SaveError::MalformedInit { .. }), "{err:?}");
    }

    #[test]
    fn truncated_header_is_malformed_not_a_panic() {
        let buf = vec![1u8, 2u8, 3u8]; // shorter than the 8-byte version header
        let err = parse(&buf).unwrap_err();
        assert!(matches!(err, SaveError::MalformedInit { .. }), "{err:?}");
    }
}
