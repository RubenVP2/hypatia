//! Localized labels and compact formatters shared by the companion and telemetry views.

use hypatia_shared::{is_champion_placeholder, Lang, SourceId};
use slint::SharedString;

pub fn queue_type_label(queue_type: &str, lang: Lang) -> String {
    let trimmed = queue_type.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "RANKED_SOLO_5X5" | "RANKED SOLO 5X5" => {
            lang.pick("Ranked Solo/Duo", "Classée Solo/Duo").to_string()
        }
        "RANKED_FLEX_SR" | "RANKED FLEX SR" => {
            lang.pick("Ranked Flex", "Classée Flexible").to_string()
        }
        "NORMAL" | "NORMAL_DRAFT" | "DRAFT_PICK" => {
            lang.pick("Normal Draft", "Faille · Draft").to_string()
        }
        "QUICKPLAY" | "SWIFTPLAY" => lang.pick("Quickplay", "Partie rapide").to_string(),
        "ARAM" => "ARAM".to_string(),
        "URF" | "ARURF" => "URF".to_string(),
        "ARENA" | "CHERRY" => "Arena".to_string(),
        "PRACTICETOOL" | "PRACTICE_TOOL" => {
            lang.pick("Practice Tool", "Outil d'entraînement").to_string()
        }
        "CUSTOM" => lang.pick("Custom", "Partie personnalisée").to_string(),
        _ => trimmed.replace('_', " "),
    }
}

pub fn sub_phase_label(sub_phase: &str, lang: Lang) -> String {
    let trimmed = sub_phase.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.to_ascii_uppercase().as_str() {
        "PLANNING" => lang.pick("Planning", "Planification").to_string(),
        "BAN_PICK" => lang.pick("Ban / Pick", "Bannissements & Choix").to_string(),
        "FINALIZATION" => lang.pick("Finalization", "Finalisation").to_string(),
        "GAME_STARTING" => lang.pick("Game Starting", "Lancement de la partie").to_string(),
        _ => trimmed.replace('_', " "),
    }
}

pub fn rank_tier_label(tier: &str, lang: Lang) -> &str {
    match tier.trim().to_ascii_uppercase().as_str() {
        "IRON" => lang.pick("Iron", "Fer"),
        "BRONZE" => "Bronze",
        "SILVER" => lang.pick("Silver", "Argent"),
        "GOLD" => lang.pick("Gold", "Or"),
        "PLATINUM" => lang.pick("Platinum", "Platine"),
        "EMERALD" => lang.pick("Emerald", "Émeraude"),
        "DIAMOND" => lang.pick("Diamond", "Diamant"),
        "MASTER" => lang.pick("Master", "Maître"),
        "GRANDMASTER" => lang.pick("Grandmaster", "Grand Maître"),
        "CHALLENGER" => "Challenger",
        _ => tier,
    }
}

pub fn champ_initials(name: &str) -> SharedString {
    let trimmed = name.trim();
    if is_champion_placeholder(trimmed) {
        return SharedString::default();
    }
    let words: Vec<&str> = trimmed
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let s: String = if words.len() >= 2 {
        words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect()
    } else {
        trimmed.chars().take(2).collect()
    };
    SharedString::from(s.to_uppercase())
}

pub fn format_mm_ss(seconds: f64) -> SharedString {
    let total = seconds.max(0.0) as u64;
    SharedString::from(format!("{}:{:02}", total / 60, total % 60))
}

pub fn format_bytes(bytes: u64, lang: Lang) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} {}", bytes as f64 / 1_048_576.0, lang.pick("MB", "Mo"))
    } else if bytes >= 1024 {
        format!("{:.1} {}", bytes as f64 / 1024.0, lang.pick("KB", "Ko"))
    } else {
        format!("{} {}", bytes, lang.pick("B", "o"))
    }
}

pub fn format_ts_hms(ts_ms: i64) -> String {
    let total_secs = (ts_ms.max(0) / 1000) % 86_400;
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

pub fn source_short_label(src: SourceId, lang: Lang) -> (&'static str, i32) {
    match src {
        SourceId::RiotClient => ("RC", 0),
        SourceId::LeagueClient => ("LCU", 1),
        SourceId::LiveGame => (lang.pick("GAME", "JEU"), 2),
        SourceId::System => ("SYS", 3),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_localization_labels_cover_both_languages() {
        assert_eq!(queue_type_label("RANKED_SOLO_5X5", Lang::En), "Ranked Solo/Duo");
        assert_eq!(queue_type_label("RANKED_SOLO_5X5", Lang::Fr), "Classée Solo/Duo");
        assert_eq!(sub_phase_label("BAN_PICK", Lang::En), "Ban / Pick");
        assert_eq!(sub_phase_label("BAN_PICK", Lang::Fr), "Bannissements & Choix");
        assert_eq!(rank_tier_label("EMERALD", Lang::En), "Emerald");
        assert_eq!(rank_tier_label("EMERALD", Lang::Fr), "Émeraude");
        assert_eq!(format_bytes(2048, Lang::En), "2.0 KB");
        assert_eq!(format_bytes(2048, Lang::Fr), "2.0 Ko");
        assert_eq!(format_bytes(512, Lang::En), "512 B");
        assert_eq!(format_bytes(512, Lang::Fr), "512 o");
        assert_eq!(source_short_label(SourceId::LiveGame, Lang::En), ("GAME", 2));
        assert_eq!(source_short_label(SourceId::LiveGame, Lang::Fr), ("JEU", 2));
    }
}
