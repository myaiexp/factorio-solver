// Holds CLAUDE.md to its size limits and to linking every docs/ subdoc.
//
// CLAUDE.md is loaded into every session, so every byte is paid on every turn.
// It reached 43 KB by accretion (idea #4770) and was cut to pointers, with the
// detail moved into docs/*.md. These limits keep it from growing back.
//
// If a test here fails: move the text into the matching docs/<topic>.md and
// leave a short pointer in CLAUDE.md. Do not raise the limits — that is
// Mase's call, not a session's.
use std::fs;
use std::path::PathBuf;

use factorio_repo_docs::entries;

const MAX_TOTAL_BYTES: usize = 24_000;
const MAX_ENTRY_BYTES: usize = 500;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn claude_md() -> String {
    fs::read_to_string(repo_root().join("CLAUDE.md")).expect("read CLAUDE.md")
}

#[test]
fn claude_md_stays_under_its_total_size_limit() {
    let size = claude_md().len();
    assert!(
        size <= MAX_TOTAL_BYTES,
        "CLAUDE.md is {size} bytes, over the {MAX_TOTAL_BYTES}-byte limit. \
         Move detail into a docs/*.md subdoc and leave a pointer; do not \
         raise the limit (that is Mase's call)."
    );
}

#[test]
fn every_claude_md_entry_stays_under_the_entry_limit() {
    let over: Vec<String> = entries(&claude_md())
        .into_iter()
        .filter(|e| e.bytes() > MAX_ENTRY_BYTES)
        .map(|e| {
            let head: String = e.text.chars().take(70).collect();
            format!("  line {} ({} bytes): {head}", e.line, e.bytes())
        })
        .collect();
    assert!(
        over.is_empty(),
        "CLAUDE.md entries over {MAX_ENTRY_BYTES} bytes:\n{}\n\
         Move each one verbatim into the matching docs/*.md subdoc and replace \
         it with a pointer: what it is, the one rule a session must not break, \
         and the subdoc path. Do not raise the limit (that is Mase's call).",
        over.join("\n")
    );
}

#[test]
fn every_docs_subdoc_is_linked_from_claude_md() {
    let claude = claude_md();
    let mut unlinked = Vec::new();
    for entry in fs::read_dir(repo_root().join("docs")).expect("read docs/") {
        let name = entry.expect("docs/ entry").file_name().into_string().expect("utf-8 name");
        if name.ends_with(".md") && !claude.contains(&format!("docs/{name}")) {
            unlinked.push(name);
        }
    }
    assert!(
        unlinked.is_empty(),
        "docs/ subdocs not linked from CLAUDE.md: {unlinked:?}. An unlinked doc \
         is invisible to new sessions; add a pointer to it."
    );
}
