// Pure decisions the clipboard watcher makes: is this text worth decoding, and
// is the decoded result worth putting on screen.

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;

/// The shortest plausible payload after the version byte. `SINGLE_BELT`, the
/// smallest fixture in the blueprint crate, is ~100 base64 characters; 32 is
/// well under any real blueprint while still rejecting the short all-base64
/// strings ordinary text produces ("0", "0K", a hex digest starting with 0).
const MIN_PAYLOAD_LEN: usize = 32;

/// A cheap syntactic screen, run before the real decode.
///
/// Not a substitute for `decode` — the watcher still decodes before offering
/// anything, because only that proves the string is a blueprint. This exists so
/// that the overwhelmingly common clipboard content (prose, code, URLs) costs a
/// few character comparisons rather than a base64 allocation. `decode`'s own
/// `COMPRESSED_LIMIT` bounds the damage of a huge paste; this avoids paying it
/// at 2 Hz for something that was never a candidate.
pub fn looks_like_blueprint(text: &str) -> bool {
    let Some(payload) = text.strip_prefix('0') else { return false };
    payload.len() >= MIN_PAYLOAD_LEN
        && payload
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
}

/// Whether a blueprint the watcher found should replace what the viewport shows.
///
/// One rule, and it covers two things that look like separate problems:
///
///   * re-copying a blueprint that is already on screen would re-fit the camera
///     and otherwise change nothing;
///   * "Copy blueprint" puts the *generated* block's own string on the
///     clipboard, and the viewport is showing exactly that block — so without
///     this the app would immediately reload its own output. A guard written
///     specifically against the copy button would have to be re-added at every
///     future call site that writes to the clipboard; this one cannot be
///     forgotten, because it is stated in terms of what is on screen.
pub fn should_load(candidate: &str, displayed: Option<&str>) -> bool {
    Some(candidate) != displayed
}
