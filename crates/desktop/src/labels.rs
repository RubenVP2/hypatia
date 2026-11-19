//! Localized labels and compact formatters delegating to `hypatia_shared`.

use hypatia_shared::{champ_initials_str, format_mm_ss_str};
pub use hypatia_shared::{
    format_bytes, format_ts_hms, queue_type_label, rank_tier_label, source_short_label,
    sub_phase_label,
};
use slint::SharedString;

pub fn champ_initials(name: &str) -> SharedString {
    SharedString::from(champ_initials_str(name))
}

pub fn format_mm_ss(seconds: f64) -> SharedString {
    SharedString::from(format_mm_ss_str(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hypatia_shared::{Lang, SourceId};

    #[test]
    fn desktop_localization_labels_cover_both_languages() {
        assert_eq!(
            queue_type_label("RANKED_SOLO_5X5", Lang::En),
            "Ranked Solo/Duo"
        );
        assert_eq!(
            queue_type_label("RANKED_SOLO_5X5", Lang::Fr),
            "Classée Solo/Duo"
        );
        assert_eq!(sub_phase_label("BAN_PICK", Lang::En), "Ban / Pick");
        assert_eq!(
            sub_phase_label("BAN_PICK", Lang::Fr),
            "Bannissements & Choix"
        );
        assert_eq!(rank_tier_label("EMERALD", Lang::En), "Emerald");
        assert_eq!(rank_tier_label("EMERALD", Lang::Fr), "Émeraude");
        assert_eq!(format_bytes(2048, Lang::En), "2.0 KB");
        assert_eq!(format_bytes(2048, Lang::Fr), "2.0 Ko");
        assert_eq!(format_bytes(512, Lang::En), "512 B");
        assert_eq!(format_bytes(512, Lang::Fr), "512 o");
        assert_eq!(
            source_short_label(SourceId::LiveGame, Lang::En),
            ("GAME", 2)
        );
        assert_eq!(source_short_label(SourceId::LiveGame, Lang::Fr), ("JEU", 2));
    }
}
