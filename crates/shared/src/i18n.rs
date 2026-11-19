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
    fn parses_language_codes_and_roundtrips() {
        assert_eq!(Lang::from_code("fr"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("EN_US"), Some(Lang::En));
        assert_eq!(Lang::from_code("unknown"), None);
        assert_eq!(Lang::Fr.as_code(), "fr");
        assert_eq!(Lang::En.as_code(), "en");
    }
}
