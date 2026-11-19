//! Opening a provider page in the player's default browser, on their click. The one way the
//! desktop app hands a URL to the system: plain `https` pages on an allowlist of hosts. Hypatia
//! itself requests nothing from these sites; the browser does, after the click.
//!
//! The allowlist and the dpm.lol paths live in `hypatia_shared::links`, shared with the
//! Mobile Remote.

pub use hypatia_shared::links::{dpm_build, dpm_matchups, is_allowed, DPM_TIER_LIST};

/// Opens `url` in the default browser if [`is_allowed`]; returns whether it was handed over.
pub fn open_external(url: &str) -> bool {
    if !is_allowed(url) {
        tracing::warn!("Refused to open a link outside the allowlist");
        return false;
    }
    #[cfg(windows)]
    let spawned = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(url).spawn();
    match spawned {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!("Could not open the browser: {err}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_outside_the_allowlist_never_open() {
        assert!(!open_external("http://dpm.lol/tierlist"));
        assert!(!open_external("file:///C:/Windows"));
    }
}
