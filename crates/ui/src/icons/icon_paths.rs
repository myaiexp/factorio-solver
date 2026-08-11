// Resolves a mod-relative Factorio icon path ("__mod__/...") to an absolute file path.

use std::path::{Path, PathBuf};

/// "__base__/graphics/icons/x.png" -> "<install>/data/base/graphics/icons/x.png"
///
/// The path after the "__mod__/" prefix is preserved verbatim — icons are
/// not always under `graphics/icons/`, e.g. `__base__/graphics/entity/...`.
/// Returns None for anything that isn't a genuine mod-relative path: no
/// prefix, an unterminated prefix, an empty mod name, or a tail that
/// escapes the mod directory (absolute or containing `..`).
pub fn resolve_icon_path(install: &Path, mod_relative: &str) -> Option<PathBuf> {
    let (mod_name, tail) = split_mod_prefix(mod_relative)?;
    let rel_path = Path::new(tail);
    let escapes = rel_path.is_absolute()
        || rel_path.components().any(|c| matches!(c, std::path::Component::ParentDir));
    if escapes {
        return None;
    }
    Some(install.join("data").join(mod_name).join(rel_path))
}

/// Splits `"__mod-name__/rest/of/path.png"` into `("mod-name", "rest/of/path.png")`.
fn split_mod_prefix(mod_relative: &str) -> Option<(&str, &str)> {
    let rest = mod_relative.strip_prefix("__")?;
    let close = rest.find("__")?;
    let (mod_name, after_close) = rest.split_at(close);
    if mod_name.is_empty() {
        return None;
    }
    let tail = after_close.strip_prefix("__")?.strip_prefix('/')?;
    if tail.is_empty() { None } else { Some((mod_name, tail)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_mod_prefix_to_install_subdir() {
        let p = resolve_icon_path(Path::new("/i"), "__base__/graphics/icons/x.png").unwrap();
        assert_eq!(p, Path::new("/i/data/base/graphics/icons/x.png"));
        let p =
            resolve_icon_path(Path::new("/i"), "__space-age__/graphics/icons/y.png").unwrap();
        assert_eq!(p, Path::new("/i/data/space-age/graphics/icons/y.png"));
    }

    #[test]
    fn resolves_every_mod_prefix_present_in_the_data() {
        let cases = [
            ("__base__/graphics/icons/a.png", "/i/data/base/graphics/icons/a.png"),
            ("__space-age__/graphics/icons/b.png", "/i/data/space-age/graphics/icons/b.png"),
            (
                "__elevated-rails__/graphics/icons/c.png",
                "/i/data/elevated-rails/graphics/icons/c.png",
            ),
            ("__quality__/graphics/icons/d.png", "/i/data/quality/graphics/icons/d.png"),
        ];
        for (input, expected) in cases {
            let p = resolve_icon_path(Path::new("/i"), input).unwrap();
            assert_eq!(p, Path::new(expected), "input {input:?}");
        }
    }

    #[test]
    fn preserves_non_icons_directory_paths() {
        let p = resolve_icon_path(
            Path::new("/i"),
            "__base__/graphics/entity/one-way-valve/one-way-valve-east.png",
        )
        .unwrap();
        assert!(p.ends_with("data/base/graphics/entity/one-way-valve/one-way-valve-east.png"));
    }

    #[test]
    fn rejects_malformed_mod_prefixes() {
        let bad = [
            "graphics/icons/x.png",        // no prefix at all
            "__base/graphics/icons/x.png", // unterminated prefix
            "____/graphics/icons/x.png",   // empty mod name
            "__base__//etc/passwd",        // absolute-looking tail
            "__base__/../../etc/passwd",   // parent-dir traversal
            "__base__/",                   // prefix with no tail
            "",                            // empty string
        ];
        for input in bad {
            assert!(
                resolve_icon_path(Path::new("/i"), input).is_none(),
                "expected None for {input:?}"
            );
        }
    }
}
