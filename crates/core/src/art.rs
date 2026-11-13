//! Summoner's Rift artwork shipped by the League client, prepared once for display.
//!
//! Each piece is fetched from the local LCU like any other game asset, then cropped,
//! downscaled (and blurred, for the backdrop) a single time before it is written to the
//! disk cache. The UI only ever decodes the small prepared file: no per-frame scaling of a
//! 1280x720 texture, no blur filter at render time (Slint has none), and a few hundred KB
//! of memory instead of several MB per image.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};

/// Asset-cache category under which prepared client artwork is stored.
pub const CATEGORY: &str = "art";

const SRU_IMG: &str =
    "/lol-game-data/assets/content/src/LeagueClient/GameModeAssets/Classic_SRU/img/";

/// Square frame `(x, y, side)` of the rift inside the 1280x720 map illustrations. The map
/// widget (`LineMap` in `parts.slint`) projects its markers in this same frame.
pub const MAP_CROP: (u32, u32, u32) = (315, 20, 600);
/// Side of the prepared map: about twice the largest on-screen map, for crisp HiDPI output.
const MAP_SIDE: u32 = 480;

/// One piece of client artwork; the discriminant is its asset id in [`CATEGORY`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientArt {
    /// Rift seen with the player's base bottom-left (blue side), beam on the base.
    MapSouth = 1,
    /// Rift seen with the player's base top-right (red side).
    MapNorth = 2,
    /// Round valley medallion behind the ready check.
    ReadyCheck = 3,
    Victory = 4,
    Defeat = 5,
    /// Blurred Summoner's Rift valley used as the ambient window backdrop.
    Backdrop = 6,
}

impl ClientArt {
    pub const ALL: [ClientArt; 6] = [
        ClientArt::MapSouth,
        ClientArt::MapNorth,
        ClientArt::ReadyCheck,
        ClientArt::Victory,
        ClientArt::Defeat,
        ClientArt::Backdrop,
    ];

    pub fn id(self) -> i64 {
        self as i64
    }

    pub fn from_id(id: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }

    /// LCU path of the source artwork.
    pub fn lcu_path(self) -> String {
        let file = match self {
            ClientArt::MapSouth => "map-south.png",
            ClientArt::MapNorth => "map-north.png",
            ClientArt::ReadyCheck => "ready-check-background.png",
            ClientArt::Victory => "icon-victory.png",
            ClientArt::Defeat => "icon-defeat.png",
            ClientArt::Backdrop => "gameflow-background.jpg",
        };
        format!("{SRU_IMG}{file}")
    }

    /// Crops, downscales and encodes the raw client image for display. Returns the bytes to
    /// cache with their extension and MIME type, or `None` when the source is unusable.
    pub fn prepare(self, raw: &[u8]) -> Option<(Vec<u8>, &'static str, &'static str)> {
        let img = image::load_from_memory(raw).ok()?;
        match self {
            ClientArt::MapSouth | ClientArt::MapNorth => {
                let (x, y, side) = MAP_CROP;
                if img.width() < x + side || img.height() < y + side {
                    return None;
                }
                let map = img.crop_imm(x, y, side, side).resize_exact(
                    MAP_SIDE,
                    MAP_SIDE,
                    FilterType::CatmullRom,
                );
                encode_jpeg(&map, 86)
            }
            ClientArt::ReadyCheck => encode_png(&img.resize(256, 256, FilterType::CatmullRom)),
            ClientArt::Victory | ClientArt::Defeat => {
                encode_png(&img.resize(192, 192, FilterType::CatmullRom))
            }
            ClientArt::Backdrop => {
                // The client image carries a flat dark band on top and on the right: keep the
                // painted valley only, then blur a small copy (cheap, and it upscales cleanly).
                let w = img.width().min(1056);
                let top = 80.min(img.height() / 4);
                let valley = img.crop_imm(0, top, w, img.height() - top);
                let small = valley.resize(528, 320, FilterType::Triangle);
                encode_jpeg(&small.blur(7.0), 82)
            }
        }
    }
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Option<(Vec<u8>, &'static str, &'static str)> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode_image(&DynamicImage::ImageRgb8(img.to_rgb8()))
        .ok()?;
    Some((out, "jpg", "image/jpeg"))
}

fn encode_png(img: &DynamicImage) -> Option<(Vec<u8>, &'static str, &'static str)> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).ok()?;
    Some((out.into_inner(), "png", "image/png"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    fn png_bytes(img: DynamicImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn ids_round_trip_and_paths_stay_inside_game_data() {
        for art in ClientArt::ALL {
            assert_eq!(ClientArt::from_id(art.id()), Some(art));
            assert!(art.lcu_path().starts_with("/lol-game-data/assets/"));
        }
        assert_eq!(ClientArt::from_id(0), None);
        assert_eq!(ClientArt::from_id(99), None);
    }

    #[test]
    fn maps_are_cropped_to_the_rift_and_downscaled() {
        let raw = png_bytes(DynamicImage::ImageRgb8(RgbImage::from_pixel(
            1280,
            720,
            Rgb([10, 20, 30]),
        )));
        let (bytes, ext, mime) = ClientArt::MapSouth.prepare(&raw).unwrap();
        assert_eq!((ext, mime), ("jpg", "image/jpeg"));
        let out = image::load_from_memory(&bytes).unwrap();
        assert_eq!((out.width(), out.height()), (MAP_SIDE, MAP_SIDE));

        let too_small = png_bytes(DynamicImage::ImageRgb8(RgbImage::new(400, 300)));
        assert!(ClientArt::MapNorth.prepare(&too_small).is_none());
        assert!(ClientArt::MapNorth.prepare(b"not an image").is_none());
    }

    #[test]
    fn medallions_keep_their_transparency() {
        let raw = png_bytes(DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            410,
            410,
            Rgba([0, 0, 0, 0]),
        )));
        let (bytes, ext, _) = ClientArt::ReadyCheck.prepare(&raw).unwrap();
        assert_eq!(ext, "png");
        let out = image::load_from_memory(&bytes).unwrap();
        assert_eq!((out.width(), out.height()), (256, 256));
        assert!(out.color().has_alpha());
    }

    #[test]
    fn backdrop_is_a_small_blurred_valley() {
        let raw = png_bytes(DynamicImage::ImageRgb8(RgbImage::from_pixel(
            1280,
            720,
            Rgb([40, 60, 50]),
        )));
        let (bytes, ext, _) = ClientArt::Backdrop.prepare(&raw).unwrap();
        assert_eq!(ext, "jpg");
        let out = image::load_from_memory(&bytes).unwrap();
        assert!(out.width() <= 528 && out.height() <= 320);
    }
}
