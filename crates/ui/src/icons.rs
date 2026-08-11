// Loads entity icons from a live Factorio install into egui textures, caching hits and misses.

mod icon_image;
mod icon_install;
mod icon_paths;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use factorio_grid::EntityPrototype;

pub use icon_image::crop_icon;
pub use icon_install::detect_install;
pub use icon_paths::resolve_icon_path;

/// Why icon loading is or isn't available, for the UI to surface to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconStatus {
    /// A Factorio install was found; icons load from here.
    Ready { install: PathBuf },
    /// Detection failed and no override was configured.
    NoInstall,
}

/// Lazily decodes, crops and uploads entity icons to egui textures, keyed by
/// prototype name. Both hits and misses are cached — `get` is called per
/// visible entity per frame, so a prototype with no icon or a missing file
/// must never be re-read from disk.
pub struct IconCache {
    install: Option<PathBuf>,
    cache: HashMap<String, Option<egui::TextureHandle>>,
}

impl IconCache {
    /// `override_path` comes from configuration and is probed ahead of
    /// everything else by [`detect_install`]. Detection failure is logged
    /// once here (not per lookup) and is also readable via [`Self::status`]
    /// so the UI can surface it.
    pub fn new(override_path: Option<&Path>) -> Self {
        let install = detect_install(override_path);
        if install.is_none() {
            eprintln!(
                "factorio-ui: no Factorio install found; entity icons disabled. \
                 Set FACTORIO_INSTALL_DIR to point at your install to enable them."
            );
        }
        Self { install, cache: HashMap::new() }
    }

    /// For the UI to surface *why* icons are unavailable.
    pub fn status(&self) -> IconStatus {
        match &self.install {
            Some(install) => IconStatus::Ready { install: install.clone() },
            None => IconStatus::NoInstall,
        }
    }

    /// Lazily decodes, crops, uploads and caches by prototype name. Returns
    /// None on any failure — the caller falls back to a coloured rect.
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        proto: &EntityPrototype,
    ) -> Option<egui::TextureHandle> {
        if let Some(cached) = self.cache.get(&proto.name) {
            return cached.clone();
        }
        let texture = self
            .install
            .as_deref()
            .and_then(|install| load_icon_rgba(install, proto))
            .map(|rgba| upload_texture(ctx, &proto.name, &rgba));
        self.cache.insert(proto.name.clone(), texture.clone());
        texture
    }
}

/// Decodes, resolves and crops a prototype's icon file into raw RGBA pixels
/// — the whole pipeline minus texture upload, so it can be exercised in a
/// headless unit test without an `egui::Context`.
fn load_icon_rgba(install: &Path, proto: &EntityPrototype) -> Option<image::RgbaImage> {
    let mod_relative = proto.icon_path.as_deref()?;
    let file_path = resolve_icon_path(install, mod_relative)?;
    let bytes = std::fs::read(&file_path).ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;
    let icon_size = proto.icon_size.unwrap_or(64);
    Some(crop_icon(decoded, icon_size).to_rgba8())
}

fn upload_texture(ctx: &egui::Context, name: &str, rgba: &image::RgbaImage) -> egui::TextureHandle {
    let (width, height) = rgba.dimensions();
    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba.as_raw());
    ctx.load_texture(name, color_image, egui::TextureOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_proto(name: &str, icon_path: Option<&str>, icon_size: Option<u32>) -> EntityPrototype {
        EntityPrototype {
            name: name.to_string(),
            tile_width: 1,
            tile_height: 1,
            crafting_speed: None,
            power_kw: None,
            module_slots: 0,
            fluid_connections: Vec::new(),
            belt_throughput: None,
            display_name: None,
            icon_path: icon_path.map(str::to_string),
            icon_size,
            crafting_categories: Vec::new(),
            underground_max_distance: None,
            pickup_position: None,
            insert_position: None,
        }
    }

    fn install_with_icon(relative: &str, image: image::DynamicImage) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/base")).unwrap();
        let file_path = dir.path().join("data/base").join(relative);
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        image.save(&file_path).unwrap();
        dir
    }

    #[test]
    fn status_reports_no_install_when_detection_fails() {
        let cache = IconCache::new(Some(Path::new("/nonexistent")));
        assert_eq!(cache.status(), IconStatus::NoInstall);
    }

    #[test]
    fn status_reports_ready_with_the_install_path_when_found() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/base")).unwrap();
        let cache = IconCache::new(Some(dir.path()));
        assert_eq!(cache.status(), IconStatus::Ready { install: dir.path().to_path_buf() });
    }

    #[test]
    fn load_icon_rgba_decodes_and_crops_a_mipmap_strip() {
        let dir =
            install_with_icon("graphics/icons/x.png", image::DynamicImage::new_rgba8(120, 64));
        let proto = fixture_proto("test-entity", Some("__base__/graphics/icons/x.png"), None);
        let rgba = load_icon_rgba(dir.path(), &proto).expect("icon should decode");
        assert_eq!(rgba.dimensions(), (64, 64));
    }

    #[test]
    fn load_icon_rgba_returns_none_without_an_icon_path() {
        let dir = tempfile::tempdir().unwrap();
        let proto = fixture_proto("no-icon", None, None);
        assert!(load_icon_rgba(dir.path(), &proto).is_none());
    }

    #[test]
    fn load_icon_rgba_returns_none_for_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let proto = fixture_proto("ghost", Some("__base__/graphics/icons/nope.png"), None);
        assert!(load_icon_rgba(dir.path(), &proto).is_none());
    }

    #[test]
    fn get_returns_none_and_does_not_panic_without_an_install() {
        let mut cache = IconCache::new(Some(Path::new("/nonexistent")));
        let ctx = egui::Context::default();
        let proto = fixture_proto("whatever", Some("__base__/graphics/icons/x.png"), None);
        assert!(cache.get(&ctx, &proto).is_none());
    }

    #[test]
    fn get_uploads_and_caches_a_texture_for_a_real_file() {
        let dir = install_with_icon("graphics/icons/x.png", image::DynamicImage::new_rgba8(120, 64));
        let mut cache = IconCache::new(Some(dir.path()));
        let ctx = egui::Context::default();
        let proto = fixture_proto("test-entity", Some("__base__/graphics/icons/x.png"), None);

        let tex = cache.get(&ctx, &proto).expect("texture should load");
        assert_eq!(tex.size(), [64, 64]);

        // Second call must hit the cache (equal handle) rather than re-decode.
        let tex2 = cache.get(&ctx, &proto).unwrap();
        assert!(tex == tex2, "second get() should return the cached handle");
    }

    #[test]
    fn get_caches_misses_so_a_missing_file_is_read_only_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data/base")).unwrap();
        let mut cache = IconCache::new(Some(dir.path()));
        let ctx = egui::Context::default();
        let proto = fixture_proto("missing-file", Some("__base__/graphics/icons/nope.png"), None);

        assert!(cache.get(&ctx, &proto).is_none());
        assert!(cache.cache.contains_key("missing-file"), "miss must be cached");
    }
}
