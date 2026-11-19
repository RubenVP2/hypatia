//! Headless screenshots of the Stats page: renders the real `MainWindow` with Slint's software
//! renderer in demo mode, one image per tab, theme and language. Ignored by default; run with
//!
//! ```text
//! HYPATIA_SHOTS=<dir> cargo test -p hypatia shots -- --ignored --nocapture
//! ```
//!
//! and convert the `.bmp` files with any image tool. Nothing appears on screen.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use hypatia_core::settings::Settings;
use hypatia_core::AppState;
use hypatia_shared::Lang;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Global, PhysicalSize, PlatformError, Rgb8Pixel};

use crate::stats_view::StatsView;
use crate::{MainWindow, Theme};

struct ShotPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for ShotPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

/// 24-bit BMP, bottom-up rows padded to four bytes.
fn write_bmp(path: &Path, width: usize, height: usize, pixels: &[Rgb8Pixel]) {
    let row = (width * 3).div_ceil(4) * 4;
    let size = 54 + row * height;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in (0..height).rev() {
        for x in 0..width {
            let p = pixels[y * width + x];
            let (r, g, b) = (p.r, p.g, p.b);
            out.extend_from_slice(&[b, g, r]);
        }
        out.resize(out.len() + row - width * 3, 0);
    }
    std::fs::write(path, out).expect("write screenshot");
}

#[test]
#[ignore = "writes screenshots; run on demand with HYPATIA_SHOTS"]
fn shots() {
    let dir = PathBuf::from(std::env::var("HYPATIA_SHOTS").expect("HYPATIA_SHOTS=<dir>"));
    std::fs::create_dir_all(&dir).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(ShotPlatform {
        window: window.clone(),
    }))
    .unwrap();
    let (width, height) = (1320u32, 2300u32);
    window.set_size(PhysicalSize::new(width, height));

    let settings = Settings::default();
    let app = AppState::start_with(true, 2.0, Lang::En, &settings);
    let main = MainWindow::new().unwrap();
    main.set_demo(true);
    main.set_stats_open(true);
    main.show().unwrap();

    let only = std::env::var("HYPATIA_SHOTS_ONLY").unwrap_or_default();
    let capture = |shot: &str| {
        slint::platform::update_timers_and_animations();
        let mut pixels = vec![Rgb8Pixel::default(); (width * height) as usize];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, width as usize);
        });
        let path = dir.join(format!("{shot}.bmp"));
        write_bmp(&path, width as usize, height as usize, &pixels);
        println!("{}", path.display());
    };
    for (lang, code) in [(Lang::En, "en"), (Lang::Fr, "fr")] {
        app.set_lang(lang);
        slint::select_bundled_translation(code).unwrap();
        main.set_language(code.into());
        for dark in [true, false] {
            Theme::get(&main).set_dark(dark);
            for (tab, name) in [(0, "build"), (1, "tiers"), (2, "matchups"), (3, "mine")] {
                let shot = format!("{name}-{}-{code}", if dark { "dark" } else { "light" });
                if !only.is_empty() && !shot.contains(&only) {
                    continue;
                }
                let mut view = StatsView::default();
                view.select_tab(tab);
                view.refresh(&main, &app);
                capture(&shot);
            }
        }
    }

    // Without demo, League or Online stats: the offline and Online-off states, then the
    // Performance sheet with its Online stats switch.
    app.set_lang(Lang::En);
    slint::select_bundled_translation("en").unwrap();
    Theme::get(&main).set_dark(true);
    let live = AppState::start_with(false, 2.0, Lang::En, &settings);
    main.set_demo(false);
    for (tab, name) in [(0, "build"), (1, "tiers"), (3, "mine")] {
        let shot = format!("offline-{name}");
        if !only.is_empty() && !shot.contains(&only) {
            continue;
        }
        let mut view = StatsView::default();
        view.select_tab(tab);
        view.open_build(&live, 103, 2);
        view.select_tab(tab);
        view.refresh(&main, &live);
        capture(&shot);
    }
    if only.is_empty() || "settings".contains(&only) {
        main.set_settings_open(true);
        capture("settings");
        main.set_settings_open(false);
    }
    if only.is_empty() || "remote".contains(&only) {
        main.set_remote_url(
            "http://192.168.1.10:7879/?token=0123456789abcdef0123456789abcdef".into(),
        );
        main.set_remote_panel_open(true);
        for (lang, code) in [(Lang::En, "en"), (Lang::Fr, "fr")] {
            app.set_lang(lang);
            slint::select_bundled_translation(code).unwrap();
            main.set_language(code.into());
            capture(&format!("remote-{code}"));
        }
        main.set_remote_panel_open(false);
    }
}
