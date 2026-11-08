// Blueprint book browser: a book flattened to its blueprints, one selected.
use factorio_blueprint::{Blueprint, BlueprintBook};

/// One blueprint reachable inside a book, with the path that names it.
#[derive(Debug)]
pub struct BookEntry {
    /// Sub-book labels down to this blueprint, joined with ` / `. A
    /// blueprint or sub-book with no label is named by its 1-based slot,
    /// which is the number Factorio's book UI shows.
    pub path: String,
    pub blueprint: Blueprint,
}

/// A decoded book, flattened depth-first into its leaf blueprints, plus the
/// one currently on screen. Flattened once, on load: a book is browsed many
/// times per load and nesting is a display concern only — the viewport
/// shows one blueprint either way.
#[derive(Debug)]
pub struct BookBrowser {
    label: String,
    entries: Vec<BookEntry>,
    selected: usize,
}

impl BookBrowser {
    /// `Err` names the book when it holds no blueprint at any depth (only
    /// empty slots, planners, or empty sub-books), since there is then
    /// nothing to show.
    pub fn new(book: BlueprintBook) -> Result<Self, String> {
        let label = book.label.clone().unwrap_or_else(|| "Untitled book".to_string());
        let mut entries = Vec::new();
        let active = flatten(book, &[], &mut entries);
        if entries.is_empty() {
            return Err(format!("Blueprint book \"{label}\" contains no blueprints"));
        }
        Ok(Self { label, entries, selected: active.unwrap_or(0) })
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn entries(&self) -> &[BookEntry] {
        &self.entries
    }

    pub fn selected(&self) -> &BookEntry {
        &self.entries[self.selected]
    }

    /// Moves the selection; true only when it actually changed, so a caller
    /// reloads the viewport (and resets the camera) once per real pick.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.entries.len() || index == self.selected {
            return false;
        }
        self.selected = index;
        true
    }

    /// Entry picker for the top bar. Returns the index the user picked this
    /// frame, if it differs from the one shown; the caller applies it, so a
    /// pick and a programmatic switch go through the same path.
    pub fn picker(&self, ui: &mut egui::Ui) -> Option<usize> {
        let mut pick = self.selected;
        egui::ComboBox::from_id_salt("book_entry")
            .selected_text(&self.selected().path)
            .width(260.0)
            .show_ui(ui, |ui| {
                for (i, entry) in self.entries().iter().enumerate() {
                    ui.selectable_value(&mut pick, i, &entry.path);
                }
            });
        (pick != self.selected).then_some(pick)
    }
}

/// Appends `book`'s leaf blueprints to `out` in slot order, and returns the
/// position in `out` of the entry Factorio had active, following
/// `active_index` into a nested book. `None` when the active slot is empty
/// or missing, so the caller falls back to the first entry.
fn flatten(book: BlueprintBook, prefix: &[String], out: &mut Vec<BookEntry>) -> Option<usize> {
    let mut slots = book.blueprints;
    // The wire order is not slot order; Factorio's own UI is.
    slots.sort_by_key(|e| e.index);
    let mut active = None;
    for slot in slots {
        let is_active = slot.index == book.active_index;
        let slot_name = || format!("Slot {}", slot.index + 1);
        if let Some(bp) = slot.blueprint {
            let mut path = prefix.to_vec();
            path.push(bp.label.clone().unwrap_or_else(slot_name));
            if is_active {
                active = Some(out.len());
            }
            out.push(BookEntry { path: path.join(" / "), blueprint: bp });
        } else if let Some(sub) = slot.blueprint_book {
            let mut path = prefix.to_vec();
            path.push(sub.label.clone().unwrap_or_else(slot_name));
            let first = out.len();
            let inner = flatten(sub, &path, out);
            if is_active && out.len() > first {
                active = Some(inner.unwrap_or(first));
            }
        }
    }
    active
}

#[cfg(test)]
#[path = "book_tests.rs"]
mod tests;
