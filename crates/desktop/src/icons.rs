//! Decoded game icons for the desktop views, backed by the core disk cache.

use std::collections::HashMap;
use std::path::Path;

use hypatia_core::AppState;
use slint::Image;

/// Seam for resolving decoded Slint icons so view builders stay testable in-process.
pub trait IconLookup {
    fn resolve_icon(&mut self, category: &'static str, id: i64) -> Image;

    /// A skin's splash-derived art (`splash`, or the prepared `banner`), falling back to the
    /// champion's base skin while the chosen skin is not cached. Empty when the client artwork
    /// switch is off.
    fn resolve_skin_art(&mut self, category: &'static str, skin_id: i64, champ_id: i64) -> Image;
}

/// Deterministic no-op icon lookup for in-process desktop view tests.
#[cfg(test)]
#[derive(Default)]
pub struct NoopIconLookup;

#[cfg(test)]
impl IconLookup for NoopIconLookup {
    fn resolve_icon(&mut self, _category: &'static str, _id: i64) -> Image {
        Image::default()
    }

    fn resolve_skin_art(&mut self, _category: &'static str, _skin_id: i64, _champ_id: i64) -> Image {
        Image::default()
    }
}

/// Live adapter tying [`DesktopAssetLoader`] to [`AppState::resolve_cached_asset`].
pub struct AppIconLookup<'a> {
    pub app: &'a AppState,
    pub loader: &'a mut DesktopAssetLoader,
}

impl IconLookup for AppIconLookup<'_> {
    fn resolve_icon(&mut self, category: &'static str, id: i64) -> Image {
        self.loader.resolve(self.app, category, id)
    }

    fn resolve_skin_art(&mut self, category: &'static str, skin_id: i64, champ_id: i64) -> Image {
        if !self.app.flags.splash_backdrops() {
            return Image::default();
        }
        let target_id = if skin_id > 0 {
            skin_id
        } else if champ_id > 0 {
            champ_id * 1000
        } else {
            return Image::default();
        };
        let img = self.loader.resolve(self.app, category, target_id);
        if img.size().width > 0 {
            return img;
        }
        let base_id = champ_id * 1000;
        if champ_id > 0
            && base_id != target_id
            && self.app.assets.cached_image_path(category, base_id).is_some()
        {
            return self.loader.resolve(self.app, category, base_id);
        }
        Image::default()
    }
}

#[derive(Default)]
pub struct DesktopAssetLoader {
    images: HashMap<String, Image>,
}

impl DesktopAssetLoader {
    /// Returns the decoded icon, or an empty image while it is being fetched (or when game
    /// icons are disabled).
    pub fn resolve(&mut self, app_state: &AppState, category: &'static str, id: i64) -> Image {
        if !app_state.flags.game_icons() {
            return Image::default();
        }
        let key = format!("{category}/{id}");
        if let Some(img) = self.images.get(&key) {
            return img.clone();
        }
        let Some(path) = app_state.resolve_cached_asset(category, id) else {
            return Image::default();
        };
        self.insert_from_path(key, &path).unwrap_or_default()
    }

    fn insert_from_path(&mut self, key: String, path: &Path) -> Option<Image> {
        let img = Image::load_from_path(path).ok()?;
        self.images.insert(key, img.clone());
        Some(img)
    }

    #[cfg(test)]
    fn load_cached_only(
        &mut self,
        cache: &hypatia_core::assets::GameAssetCache,
        category: &str,
        id: i64,
    ) -> Option<Image> {
        if id < 0 || (category != "profile" && id == 0) {
            return None;
        }
        let key = format!("{category}/{id}");
        if let Some(img) = self.images.get(&key) {
            return Some(img.clone());
        }
        let path = cache.cached_image_path(category, id)?;
        self.insert_from_path(key, &path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypatia_core::assets::GameAssetCache;

    #[test]
    fn desktop_asset_loader_loads_valid_png_from_disk_cache() {
        let dir = std::env::temp_dir().join(format!("hypatia_desktop_img_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("champion")).unwrap();
        // Minimal valid 1x1 RGBA PNG generated cleanly via Image::from_rgba8 or known PNG bytes:
        // Standard 1x1 transparent PNG (67 bytes):
        let png_1x1: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.join("champion").join("103.png"), png_1x1).unwrap();

        let cache = GameAssetCache::with_dir(dir.clone());
        let mut loader = DesktopAssetLoader::default();
        let img = loader.load_cached_only(&cache, "champion", 103).expect("should decode cached 1x1 PNG");
        assert_eq!(img.size().width, 1);
        assert_eq!(img.size().height, 1);
        assert!(loader.load_cached_only(&cache, "champion", 9999).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
