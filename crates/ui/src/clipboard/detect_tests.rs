use super::{looks_like_blueprint, should_load};
use factorio_blueprint::fixtures;

#[test]
fn a_real_blueprint_string_passes_the_screen() {
    assert!(looks_like_blueprint(fixtures::SINGLE_BELT));
    assert!(looks_like_blueprint(fixtures::ASSEMBLER_SETUP));
    assert!(looks_like_blueprint(fixtures::BLUEPRINT_BOOK));
}

#[test]
fn ordinary_clipboard_content_is_screened_out() {
    for text in [
        "",
        "hello world",
        "https://factorio.com/blueprint",
        "let x = 1;",
        // Base64 alphabet throughout, but no version byte.
        "eNpTVDBQ0lFQMlSKBQAFCwGL",
        // Version byte, but the payload is not base64.
        "0this is not base64 at all, not even close",
    ] {
        assert!(!looks_like_blueprint(text), "should be screened out: {text:?}");
    }
}

/// The screen must not fire on short strings that happen to start with '0' and
/// use only base64 characters — a truncated hash, a version number, an id.
#[test]
fn short_base64_looking_strings_are_screened_out() {
    assert!(!looks_like_blueprint("0"));
    assert!(!looks_like_blueprint("0a1b2c3d"));
    assert!(!looks_like_blueprint("0123456789abcdef")); // 15-char payload
}

/// The screen is deliberately permissive — it only decides what is worth
/// decoding. Garbage that survives it must still be rejected by `decode`,
/// which is what the watcher actually gates on.
#[test]
fn the_screen_passes_garbage_that_decode_then_rejects() {
    let garbage = format!("0{}", "A".repeat(64));
    assert!(looks_like_blueprint(&garbage), "the cheap screen has no way to know");
    assert!(factorio_blueprint::decode(&garbage).is_err(), "decode is the real gate");
}

#[test]
fn a_blueprint_that_is_not_on_screen_loads() {
    assert!(should_load(fixtures::SINGLE_BELT, None));
    assert!(should_load(fixtures::SINGLE_BELT, Some(fixtures::ASSEMBLER_SETUP)));
}

#[test]
fn a_blueprint_already_on_screen_is_not_reloaded() {
    assert!(!should_load(fixtures::SINGLE_BELT, Some(fixtures::SINGLE_BELT)));
}

/// The self-copy loop: the viewport shows a generated block, "Copy blueprint"
/// puts that block's own string on the clipboard, and the watcher offers it
/// straight back. It must not be loaded — otherwise every copy silently
/// re-imports the app's own output over the top of itself.
#[test]
fn the_apps_own_copy_button_cannot_trigger_a_reload() {
    let generated = fixtures::ASSEMBLER_SETUP; // stands in for a generated block
    assert!(!should_load(generated, Some(generated)));
}
