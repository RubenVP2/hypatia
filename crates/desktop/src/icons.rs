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

struct CachedImage {
    image: Image,
    approx_bytes: u64,
    last_used: u64,
}

pub struct DesktopAssetLoader {
    images: HashMap<String, CachedImage>,
    max_entries: usize,
    clock: u64,
    total_bytes: u64,
}

impl Default for DesktopAssetLoader {
    fn default() -> Self {
        Self {
            images: HashMap::new(),
            max_entries: 256,
            clock: 0,
            total_bytes: 0,
        }
    }
}

impl DesktopAssetLoader {
    /// Returns the decoded icon, or an empty image while it is being fetched (or when game
    /// icons are disabled).
    pub fn resolve(&mut self, app_state: &AppState, category: &'static str, id: i64) -> Image {
        if !app_state.flags.game_icons() {
            return Image::default();
        }
        self.set_max_entries(app_state.flags.asset_cache_budget().max_entries());
        let key = format!("{category}/{id}");
        if let Some(img) = self.get_cached(&key) {
            return img;
        }
        let Some(path) = app_state.resolve_cached_asset(category, id) else {
            return Image::default();
        };
        self.insert_from_path(key, &path).unwrap_or_default()
    }

    /// Updates the maximum number of decoded Slint textures held in RAM, evicting LRU entries immediately.
    pub fn set_max_entries(&mut self, max: usize) {
        self.max_entries = max;
        if max == 0 {
            self.clear();
            return;
        }
        let evicted = self.evict_excess(max);
        if evicted && self.images.capacity() > max.saturating_mul(2).max(32) {
            self.images.shrink_to_fit();
        }
    }

    /// Drops decoded textures matching any of `categories` (e.g. `"splash"`, `"banner"`, `"art"`).
    pub fn evict_categories(&mut self, categories: &[&str]) {
        if categories.is_empty() || self.images.is_empty() {
            return;
        }
        let mut removed = 0u64;
        self.images.retain(|key, entry| {
            let matches = categories.iter().any(|cat| {
                key.starts_with(cat) && key.as_bytes().get(cat.len()) == Some(&b'/')
            });
            if matches {
                removed = removed.saturating_add(entry.approx_bytes);
            }
            !matches
        });
        self.total_bytes = self.total_bytes.saturating_sub(removed);
        if removed > 0 && self.images.capacity() > self.images.len().saturating_mul(2).max(32) {
            self.images.shrink_to_fit();
        }
    }

    /// Releases every decoded Slint texture currently kept in memory.
    pub fn clear(&mut self) {
        self.images.clear();
        self.images.shrink_to_fit();
        self.total_bytes = 0;
    }

    /// Returns `(image_count, approx_rgba_bytes)` currently decoded in memory.
    pub fn stats(&self) -> (usize, u64) {
        (self.images.len(), self.total_bytes)
    }

    fn get_cached(&mut self, key: &str) -> Option<Image> {
        let next_clock = self.clock.wrapping_add(1);
        self.clock = next_clock;
        let entry = self.images.get_mut(key)?;
        entry.last_used = next_clock;
        Some(entry.image.clone())
    }

    fn insert_from_path(&mut self, key: String, path: &Path) -> Option<Image> {
        if self.max_entries == 0 {
            return None;
        }
        let img = Image::load_from_path(path).ok()?;
        let size = img.size();
        let approx_bytes = u64::from(size.width) * u64::from(size.height) * 4;
        let next_clock = self.clock.wrapping_add(1);
        self.clock = next_clock;

        if let Some(existing) = self.images.get_mut(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(existing.approx_bytes) + approx_bytes;
            existing.image = img.clone();
            existing.approx_bytes = approx_bytes;
            existing.last_used = next_clock;
            return Some(img);
        }

        if self.images.len() >= self.max_entries {
            self.evict_excess(self.max_entries.saturating_sub(1));
        }
        self.total_bytes = self.total_bytes.saturating_add(approx_bytes);
        self.images.insert(
            key,
            CachedImage {
                image: img.clone(),
                approx_bytes,
                last_used: next_clock,
            },
        );
        Some(img)
    }

    fn evict_excess(&mut self, keep_max: usize) -> bool {
        let mut evicted = false;
        while self.images.len() > keep_max {
            let Some(oldest_key) = self
                .images
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(removed) = self.images.remove(&oldest_key) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.approx_bytes);
                evicted = true;
            }
        }
        evicted
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
        if let Some(img) = self.get_cached(&key) {
            return Some(img);
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
        assert_eq!(loader.stats(), (1, 4));
        assert!(loader.load_cached_only(&cache, "champion", 9999).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn desktop_asset_loader_enforces_lru_evicts_categories_and_clears() {
        let dir = std::env::temp_dir().join(format!("hypatia_desktop_lru_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("champion")).unwrap();
        std::fs::create_dir_all(dir.join("splash")).unwrap();
        std::fs::create_dir_all(dir.join("banner")).unwrap();

        let png_1x1: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        std::fs::write(dir.join("champion").join("103.png"), png_1x1).unwrap();
        std::fs::write(dir.join("champion").join("64.png"), png_1x1).unwrap();
        std::fs::write(dir.join("splash").join("103000.png"), png_1x1).unwrap();
        std::fs::write(dir.join("banner").join("64000.png"), png_1x1).unwrap();

        let cache = GameAssetCache::with_dir(dir.clone());
        let mut loader = DesktopAssetLoader::default();
        loader.set_max_entries(2);

        loader.load_cached_only(&cache, "champion", 103).unwrap();
        loader.load_cached_only(&cache, "splash", 103000).unwrap();
        // Touch champion/103 so splash/103000 becomes oldest.
        loader.load_cached_only(&cache, "champion", 103).unwrap();
        loader.load_cached_only(&cache, "champion", 64).unwrap();

        assert_eq!(loader.stats(), (2, 8));
        assert!(loader.get_cached("splash/103000").is_none());
        assert!(loader.get_cached("champion/103").is_some());
        assert!(loader.get_cached("champion/64").is_some());

        loader.set_max_entries(8);
        loader.load_cached_only(&cache, "splash", 103000).unwrap();
        loader.load_cached_only(&cache, "banner", 64000).unwrap();
        assert_eq!(loader.stats(), (4, 16));

        loader.evict_categories(&["splash", "banner", "art"]);
        assert_eq!(loader.stats(), (2, 8));
        assert!(loader.get_cached("splash/103000").is_none());
        assert!(loader.get_cached("banner/64000").is_none());

        loader.clear();
        assert_eq!(loader.stats(), (0, 0));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
