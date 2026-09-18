// Splits a markdown doc into the entries CLAUDE.md's size limits apply to.
//
// An entry is a list item (`- ` or `1. ` at column 0, plus its continuation
// lines) or a prose paragraph, blockquotes included. Headings, tables,
// horizontal rules and fenced code blocks are not entries: none of them is
// where detail accretes, and a code block's size is its content's business.

/// One list item or paragraph, with the 1-based line it starts on.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub line: usize,
    pub text: String,
}

impl Entry {
    /// Size in bytes as the file stores it, one newline per line.
    pub fn bytes(&self) -> usize {
        self.text.lines().map(|l| l.len() + 1).sum()
    }
}

fn starts_list_item(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") {
        return true;
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && line[digits..].starts_with(". ")
}

fn is_structural(trimmed: &str) -> bool {
    trimmed.starts_with('#') || trimmed.starts_with('|') || trimmed == "---"
}

pub fn entries(doc: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut current: Option<Entry> = None;
    let mut in_fence = false;
    for (i, line) in doc.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            out.extend(current.take());
            continue;
        }
        if in_fence || trimmed.is_empty() || is_structural(trimmed) {
            out.extend(current.take());
            continue;
        }
        match current.as_mut() {
            Some(entry) if !starts_list_item(line) => {
                entry.text.push('\n');
                entry.text.push_str(line);
            }
            _ => {
                out.extend(current.take());
                current = Some(Entry { line: i + 1, text: line.to_string() });
            }
        }
    }
    out.extend(current);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(doc: &str) -> Vec<usize> {
        entries(doc).iter().map(|e| e.line).collect()
    }

    #[test]
    fn each_list_item_is_its_own_entry_with_its_continuation_lines() {
        let doc = "- one\n  still one\n- two\n1. three\n2. four\n";
        let e = entries(doc);
        assert_eq!(lines_of(doc), vec![1, 3, 4, 5]);
        assert_eq!(e[0].text, "- one\n  still one");
        assert_eq!(e[0].bytes(), "- one\n  still one\n".len());
    }

    #[test]
    fn a_paragraph_runs_to_the_next_blank_line() {
        let doc = "first line\nsecond line\n\n> quoted\n> more\n";
        assert_eq!(lines_of(doc), vec![1, 4]);
    }

    #[test]
    fn headings_tables_rules_and_code_blocks_are_not_entries() {
        let doc = "# Title\n| a | b |\n---\n```\n- not an item\nprose inside code\n```\ntext\n";
        assert_eq!(lines_of(doc), vec![8]);
    }

    #[test]
    fn a_heading_ends_the_paragraph_above_it() {
        let doc = "para\n## Heading\nnext\n";
        assert_eq!(lines_of(doc), vec![1, 3]);
    }
}
