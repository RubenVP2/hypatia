use serde::{Deserialize, Serialize};

/// Supported application UI & game-term languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    /// Returns the short canonical code (`"en"` or `"fr"`).
    pub const fn as_code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    /// Whether a Riot client locale such as `fr_FR` or `ko_KR` is this language. `None` (the
    /// client has not said) counts as yes: its names are then all there is.
    pub fn spoken_by_client(self, locale: Option<&str>) -> bool {
        locale.is_none_or(|l| {
            l.get(..2)
                .is_some_and(|code| code.eq_ignore_ascii_case(self.as_code()))
        })
    }

    /// Parses a user/locale tag such as `"fr"`, `"fr-FR"`, `"en"`, `"en_US"`.
    pub fn from_code(raw: &str) -> Option<Self> {
        let low = raw.trim().to_ascii_lowercase();
        if low == "fr"
            || low.starts_with("fr-")
            || low.starts_with("fr_")
            || low == "french"
            || low == "français"
            || low == "francais"
        {
            Some(Lang::Fr)
        } else if low == "en"
            || low.starts_with("en-")
            || low.starts_with("en_")
            || low == "english"
            || low == "anglais"
        {
            Some(Lang::En)
        } else {
            None
        }
    }

    /// Returns `true` when French is active.
    pub const fn is_fr(self) -> bool {
        matches!(self, Lang::Fr)
    }

    /// Selects between an English and a French static string.
    pub const fn pick(self, en: &'static str, fr: &'static str) -> &'static str {
        match self {
            Lang::En => en,
            Lang::Fr => fr,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_locale_matches_only_its_language() {
        assert!(Lang::Fr.spoken_by_client(Some("fr_FR")));
        assert!(!Lang::Fr.spoken_by_client(Some("ko_KR")));
        assert!(!Lang::En.spoken_by_client(Some("ko_KR")));
        assert!(Lang::En.spoken_by_client(Some("en_GB")));
        assert!(
            Lang::En.spoken_by_client(None),
            "unknown: the client names are all there is"
        );
    }

    #[test]
    fn parses_language_codes_and_roundtrips() {
        assert_eq!(Lang::from_code("fr"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("EN_US"), Some(Lang::En));
        assert_eq!(Lang::from_code("unknown"), None);
        assert_eq!(Lang::Fr.as_code(), "fr");
        assert_eq!(Lang::En.as_code(), "en");
    }
}
