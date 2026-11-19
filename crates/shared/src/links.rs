//! Provider pages a view may link to: the host allowlist and the dpm.lol paths for a champion.
//! Pure string building, no I/O: the desktop hands an allowed URL to the browser on a click, and
//! the Mobile Remote shows it as a link the phone's own browser opens.

use crate::champions::{BuiltinChampionCatalog, ChampionLookup};
use crate::i18n::Lang;

/// Hosts a link may point to (or any of their subdomains).
pub const ALLOWED_HOSTS: &[&str] = &["dpm.lol", "metabot.gg"];

/// dpm.lol's tier list page.
pub const DPM_TIER_LIST: &str = "https://dpm.lol/tierlist";

/// Whether `url` is a plain `https` page on an allowed host, safe to hand to a browser.
pub fn is_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    if url.len() > 2048
        || url.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(
                    c,
                    '"' | '\'' | '<' | '>' | '`' | '^' | '|' | '\\' | '&' | '%'
                )
        })
    {
        return false;
    }
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if host.contains('@') || host.contains(':') {
        return false;
    }
    ALLOWED_HOSTS
        .iter()
        .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
}

/// The champion's key in Riot's English data (`Kai'Sa` → `KaiSa`, `Wukong` → `MonkeyKing`),
/// as dpm.lol uses it in its paths. `None` for an unknown champion.
pub fn champion_slug(champion_id: i64) -> Option<String> {
    if champion_id <= 0 {
        return None;
    }
    let name = BuiltinChampionCatalog.resolve_champion_name_localized(champion_id, Lang::En);
    if name.starts_with("Champion #") {
        return None;
    }
    let special = match champion_id {
        62 => Some("MonkeyKing"),
        20 => Some("Nunu"),
        888 => Some("Renata"),
        7 => Some("Leblanc"),
        200 => Some("Belveth"),
        31 => Some("Chogath"),
        121 => Some("Khazix"),
        161 => Some("Velkoz"),
        897 => Some("KSante"),
        _ => None,
    };
    if let Some(slug) = special {
        return Some(slug.to_string());
    }
    let slug: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    (!slug.is_empty()).then_some(slug)
}

/// dpm.lol's build page for a champion.
pub fn dpm_build(champion_id: i64) -> Option<String> {
    champion_slug(champion_id).map(|slug| format!("https://dpm.lol/champions/{slug}/build"))
}

/// dpm.lol's matchups page for a champion.
pub fn dpm_matchups(champion_id: i64) -> Option<String> {
    champion_slug(champion_id).map(|slug| format!("https://dpm.lol/champions/{slug}/matchups"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_https_pages_on_the_allowlist_pass() {
        assert!(is_allowed("https://dpm.lol/champions/Ahri/build"));
        assert!(is_allowed("https://www.metabot.gg/en/lol/champions/ahri"));
        assert!(is_allowed(DPM_TIER_LIST));
        assert!(!is_allowed("http://dpm.lol/tierlist"));
        assert!(!is_allowed("https://dpm.lol.evil.example/x"));
        assert!(!is_allowed("https://evildpm.lol/x"));
        assert!(!is_allowed("https://user@dpm.lol/x"));
        assert!(!is_allowed("https://dpm.lol/a b"));
        assert!(!is_allowed("https://dpm.lol/\"&calc"));
        assert!(!is_allowed("file:///C:/Windows"));
    }

    #[test]
    fn champion_keys_follow_riot_data() {
        assert_eq!(
            dpm_build(103).as_deref(),
            Some("https://dpm.lol/champions/Ahri/build")
        );
        assert_eq!(
            dpm_matchups(145).as_deref(),
            Some("https://dpm.lol/champions/KaiSa/matchups")
        );
        assert_eq!(champion_slug(62).as_deref(), Some("MonkeyKing"));
        assert_eq!(champion_slug(36).as_deref(), Some("DrMundo"));
        assert_eq!(champion_slug(4).as_deref(), Some("TwistedFate"));
        assert_eq!(champion_slug(0), None);
        assert_eq!(champion_slug(999_999), None);
        for id in [62, 20, 888, 7, 200, 31, 121, 161, 897] {
            assert!(dpm_build(id).is_some_and(|url| is_allowed(&url)), "{id}");
        }
    }
}
