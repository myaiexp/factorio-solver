// Tests for the book browser: flattening, labels, starting entry, refusals.
use super::*;
use factorio_blueprint::{decode, fixtures, Blueprint, BlueprintBook, BlueprintBookEntry};

fn bp(label: Option<&str>) -> Blueprint {
    Blueprint { item: "blueprint".into(), label: label.map(str::to_string), ..Default::default() }
}

fn leaf(index: u32, label: Option<&str>) -> BlueprintBookEntry {
    BlueprintBookEntry { index, blueprint: Some(bp(label)), blueprint_book: None }
}

fn sub_book(index: u32, label: &str, active_index: u32, entries: Vec<BlueprintBookEntry>) -> BlueprintBookEntry {
    BlueprintBookEntry { index, blueprint: None, blueprint_book: Some(book(label, active_index, entries)) }
}

fn book(label: &str, active_index: u32, blueprints: Vec<BlueprintBookEntry>) -> BlueprintBook {
    BlueprintBook {
        item: "blueprint-book".into(),
        label: Some(label.into()),
        blueprints,
        active_index,
        ..Default::default()
    }
}

fn paths(b: &BookBrowser) -> Vec<&str> {
    b.entries().iter().map(|e| e.path.as_str()).collect()
}

#[test]
fn the_real_fixture_book_lists_its_three_blueprints() {
    let data = decode(fixtures::BLUEPRINT_BOOK).unwrap();
    let browser = BookBrowser::new(data.blueprint_book.unwrap()).unwrap();
    assert_eq!(paths(&browser), ["Belt 1", "Belt 2", "Underground"]);
    assert_eq!(browser.label(), "Test Book");
    assert_eq!(browser.selected().path, "Belt 1");
}

/// Nested books are walked depth-first in slot order, each leaf labelled by
/// its path; slots arrive in any order on the wire and are sorted by `index`.
/// An empty slot (neither a blueprint nor a book — also what an upgrade or
/// deconstruction planner decodes to) is skipped.
#[test]
fn nested_books_flatten_in_slot_order_with_path_labels() {
    let b = book(
        "Main",
        0,
        vec![
            leaf(3, Some("Last")),
            sub_book(1, "Smelting", 0, vec![leaf(1, Some("Copper")), leaf(0, Some("Iron"))]),
            BlueprintBookEntry { index: 2, blueprint: None, blueprint_book: None },
            leaf(0, None),
        ],
    );
    let browser = BookBrowser::new(b).unwrap();
    assert_eq!(paths(&browser), ["Slot 1", "Smelting / Iron", "Smelting / Copper", "Last"]);
}

/// The book opens on the blueprint Factorio had active, following
/// `active_index` down into a nested book.
#[test]
fn opens_on_the_active_entry_through_nesting() {
    let b = book(
        "Main",
        1,
        vec![
            leaf(0, Some("First")),
            sub_book(1, "Smelting", 1, vec![leaf(0, Some("Iron")), leaf(1, Some("Copper"))]),
        ],
    );
    let browser = BookBrowser::new(b).unwrap();
    assert_eq!(browser.selected().path, "Smelting / Copper");
}

/// An `active_index` naming an empty slot, or no slot at all, falls back to
/// the first blueprint rather than refusing a book that is otherwise fine.
#[test]
fn an_active_index_on_an_empty_slot_falls_back_to_the_first() {
    let b = book(
        "Main",
        5,
        vec![leaf(0, Some("Only")), BlueprintBookEntry { index: 5, blueprint: None, blueprint_book: None }],
    );
    assert_eq!(BookBrowser::new(b).unwrap().selected().path, "Only");
}

#[test]
fn a_book_with_no_blueprints_anywhere_is_refused() {
    let b = book("Empty", 0, vec![sub_book(0, "Also empty", 0, vec![])]);
    let err = BookBrowser::new(b).unwrap_err();
    assert!(err.contains("Empty"), "names the book: {err}");
}

#[test]
fn select_moves_the_selection_and_ignores_out_of_range() {
    let data = decode(fixtures::BLUEPRINT_BOOK).unwrap();
    let mut browser = BookBrowser::new(data.blueprint_book.unwrap()).unwrap();
    assert!(browser.select(2));
    assert_eq!(browser.selected().path, "Underground");
    assert!(!browser.select(2), "re-selecting the same entry is not a change");
    assert!(!browser.select(9), "out of range is ignored");
    assert_eq!(browser.selected().path, "Underground");
}
