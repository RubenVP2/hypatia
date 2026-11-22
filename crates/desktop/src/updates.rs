//! Update Check on the desktop: what the ticker chip and the update sheet show. The network and
//! file work lives in `hypatia_core::update`; this only turns its outcome into the `UpdateView`.

use hypatia_core::update::{Release, UpdateError, Version};
use hypatia_shared::Lang;
use slint::SharedString;

use crate::UpdateView;

/// Where an install stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstallStep {
    #[default]
    Idle,
    Downloading,
    Restarting,
    Failed(UpdateError),
}

/// The newer release the last check found, and the install started from it.
#[derive(Debug, Default)]
pub struct UpdateState {
    pub release: Option<Release>,
    pub step: InstallStep,
}

impl UpdateState {
    /// Records a check's answer. A release already being installed is kept as it is.
    pub fn found(&mut self, release: Option<Release>) {
        if self.busy() {
            return;
        }
        if self.release != release {
            self.step = InstallStep::Idle;
        }
        self.release = release;
    }

    pub fn busy(&self) -> bool {
        matches!(
            self.step,
            InstallStep::Downloading | InstallStep::Restarting
        )
    }

    pub fn view(&self, current: Version, lang: Lang) -> UpdateView {
        let Some(release) = &self.release else {
            return UpdateView::default();
        };
        let status = match self.step {
            InstallStep::Idle => "",
            InstallStep::Downloading => lang.pick(
                "Downloading and checking the signature…",
                "Téléchargement et vérification de la signature…",
            ),
            InstallStep::Restarting => lang.pick("Restarting Hypatia…", "Redémarrage d'Hypatia…"),
            InstallStep::Failed(err) => err.message(lang),
        };
        UpdateView {
            available: true,
            version: SharedString::from(release.version.to_string()),
            current: SharedString::from(current.to_string()),
            notes: SharedString::from(release.notes.as_str()),
            page_url: SharedString::from(release.page_url.as_str()),
            installable: release.installable(),
            busy: self.busy(),
            status: SharedString::from(status),
            failed: matches!(self.step, InstallStep::Failed(_)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypatia_core::update::RELEASES_PAGE;

    fn release(tag: &str) -> Release {
        Release {
            version: Version::parse(tag).unwrap(),
            tag: tag.to_string(),
            page_url: format!("{RELEASES_PAGE}/tag/{tag}"),
            notes: "Added\n- Update check".to_string(),
            archive: None,
            signature: None,
        }
    }

    #[test]
    fn the_view_follows_the_check_and_the_install() {
        let current = Version::parse("0.5.0").unwrap();
        let mut state = UpdateState::default();
        assert!(!state.view(current, Lang::En).available);

        state.found(Some(release("v0.6.0")));
        let view = state.view(current, Lang::En);
        assert!(view.available && !view.busy && !view.failed);
        assert_eq!(view.version, "v0.6.0");
        assert_eq!(view.current, "v0.5.0");
        assert_eq!(view.status, "");
        assert!(!view.installable, "no signed archive in this release");

        state.step = InstallStep::Downloading;
        assert!(state.view(current, Lang::Fr).status.contains("signature"));
        // A later check does not interrupt a running install.
        state.found(None);
        assert!(state.release.is_some());

        state.step = InstallStep::Failed(UpdateError::BadSignature);
        let view = state.view(current, Lang::Fr);
        assert!(view.failed && !view.busy);
        assert!(view.status.contains("signé"));

        state.found(Some(release("v0.6.1")));
        assert_eq!(state.step, InstallStep::Idle, "a new release starts clean");
        state.found(None);
        assert!(!state.view(current, Lang::En).available);
    }
}
