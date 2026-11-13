use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::remote::RemoteChampionOption;

/// Canonical Summoner's Rift role across lobby preferences, draft assignments,
/// Live Client scoreboard slots, and Mobile Remote lane filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Position {
    Top,
    Jungle,
    Mid,
    Bot,
    Support,
    Fill,
}

impl Position {
    /// Riot LCU role preference codes accepted by lobby position endpoints.
    pub const VALID_LCU_CODES: &'static [&'static str] =
        &["TOP", "JUNGLE", "MIDDLE", "BOTTOM", "UTILITY", "FILL"];

    /// Parses any Riot role token (`TOP`, `JUNGLE`/`JGL`, `MIDDLE`/`MID`, `BOTTOM`/`BOT`,
    /// `UTILITY`/`SUPPORT`/`SUP`, `FILL`). Returns `None` for empty/unassigned tokens (`NONE`, `""`).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "TOP" => Some(Self::Top),
            "JUNGLE" | "JGL" => Some(Self::Jungle),
            "MIDDLE" | "MID" => Some(Self::Mid),
            "BOTTOM" | "BOT" => Some(Self::Bot),
            "UTILITY" | "SUPPORT" | "SUP" => Some(Self::Support),
            "FILL" => Some(Self::Fill),
            _ => None,
        }
    }

    /// Canonical Riot LCU code (`TOP`, `JUNGLE`, `MIDDLE`, `BOTTOM`, `UTILITY`, `FILL`).
    pub const fn lcu_code(self) -> &'static str {
        match self {
            Self::Top => "TOP",
            Self::Jungle => "JUNGLE",
            Self::Mid => "MIDDLE",
            Self::Bot => "BOTTOM",
            Self::Support => "UTILITY",
            Self::Fill => "FILL",
        }
    }

    /// Fixed desktop UI lane slot (`0..=4`), or `-1` for Fill/unassigned.
    pub const fn desktop_index(self) -> i32 {
        match self {
            Self::Top => 0,
            Self::Jungle => 1,
            Self::Mid => 2,
            Self::Bot => 3,
            Self::Support => 4,
            Self::Fill => -1,
        }
    }

    /// Compact 3–4 character scoreboard tag (`TOP`, `JGL`, `MID`, `BOT`, `SUP`, `FLEX`).
    pub const fn short_tag(self) -> &'static str {
        match self {
            Self::Top => "TOP",
            Self::Jungle => "JGL",
            Self::Mid => "MID",
            Self::Bot => "BOT",
            Self::Support => "SUP",
            Self::Fill => "FLEX",
        }
    }

    /// Localized display label for UI badges and alerts.
    pub const fn display_label(self, lang: Lang) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Jungle => "Jungle",
            Self::Mid => "Mid",
            Self::Bot => "Bot",
            Self::Support => "Support",
            Self::Fill => lang.pick("Fill", "Polyvalent"),
        }
    }
}

/// Static built-in champion catalog record unifying Riot ID, display name,
/// Summoner's Rift roles, and normalized lookup aliases.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinChampion {
    pub id: i64,
    pub name: &'static str,
    pub roles: &'static [&'static str],
    pub aliases: &'static [&'static str],
}

/// Unified built-in champion table combining full ID->name, role filters, and
/// localized/internal aliases (including Riot `rawChampionName` keys and FR names).
pub const BUILTIN_CHAMPIONS: &[BuiltinChampion] = &[
    BuiltinChampion { id: 1, name: "Annie", roles: &["MIDDLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 2, name: "Olaf", roles: &["TOP", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 3, name: "Galio", roles: &["MIDDLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 4, name: "Twisted Fate", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 5, name: "Xin Zhao", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 6, name: "Urgot", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 7, name: "LeBlanc", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 8, name: "Vladimir", roles: &["MIDDLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 9, name: "Fiddlesticks", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 10, name: "Kayle", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 11, name: "Master Yi", roles: &["JUNGLE"], aliases: &["maitreyi"] },
    BuiltinChampion { id: 12, name: "Alistar", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 13, name: "Ryze", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 14, name: "Sion", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 15, name: "Sivir", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 16, name: "Soraka", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 17, name: "Teemo", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 18, name: "Tristana", roles: &["BOTTOM", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 19, name: "Warwick", roles: &["JUNGLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 20, name: "Nunu & Willump", roles: &["JUNGLE"], aliases: &["nunu", "nunuetwillump"] },
    BuiltinChampion { id: 21, name: "Miss Fortune", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 22, name: "Ashe", roles: &["BOTTOM", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 23, name: "Tryndamere", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 24, name: "Jax", roles: &["TOP", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 25, name: "Morgana", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 26, name: "Zilean", roles: &["UTILITY", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 27, name: "Singed", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 28, name: "Evelynn", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 29, name: "Twitch", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 30, name: "Karthus", roles: &["JUNGLE", "BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 31, name: "Cho'Gath", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 32, name: "Amumu", roles: &["JUNGLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 33, name: "Rammus", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 34, name: "Anivia", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 35, name: "Shaco", roles: &["JUNGLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 36, name: "Dr. Mundo", roles: &["TOP", "JUNGLE"], aliases: &["mundo"] },
    BuiltinChampion { id: 37, name: "Sona", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 38, name: "Kassadin", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 39, name: "Irelia", roles: &["TOP", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 40, name: "Janna", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 41, name: "Gangplank", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 42, name: "Corki", roles: &["BOTTOM", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 43, name: "Karma", roles: &["UTILITY", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 44, name: "Taric", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 45, name: "Veigar", roles: &["MIDDLE", "BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 48, name: "Trundle", roles: &["TOP", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 50, name: "Swain", roles: &["UTILITY", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 51, name: "Caitlyn", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 53, name: "Blitzcrank", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 54, name: "Malphite", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 55, name: "Katarina", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 56, name: "Nocturne", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 57, name: "Maokai", roles: &["UTILITY", "JUNGLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 58, name: "Renekton", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 59, name: "Jarvan IV", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 60, name: "Elise", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 61, name: "Orianna", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 62, name: "Wukong", roles: &["JUNGLE", "TOP"], aliases: &["monkeyking"] },
    BuiltinChampion { id: 63, name: "Brand", roles: &["UTILITY", "MIDDLE", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 64, name: "Lee Sin", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 67, name: "Vayne", roles: &["BOTTOM", "TOP"], aliases: &[] },
    BuiltinChampion { id: 68, name: "Rumble", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 69, name: "Cassiopeia", roles: &["MIDDLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 72, name: "Skarner", roles: &["JUNGLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 74, name: "Heimerdinger", roles: &["MIDDLE", "TOP", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 75, name: "Nasus", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 76, name: "Nidalee", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 77, name: "Udyr", roles: &["JUNGLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 78, name: "Poppy", roles: &["JUNGLE", "TOP", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 79, name: "Gragas", roles: &["TOP", "JUNGLE", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 80, name: "Pantheon", roles: &["TOP", "MIDDLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 81, name: "Ezreal", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 82, name: "Mordekaiser", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 83, name: "Yorick", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 84, name: "Akali", roles: &["MIDDLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 85, name: "Kennen", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 86, name: "Garen", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 89, name: "Leona", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 90, name: "Malzahar", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 91, name: "Talon", roles: &["MIDDLE", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 92, name: "Riven", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 96, name: "Kog'Maw", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 98, name: "Shen", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 99, name: "Lux", roles: &["UTILITY", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 101, name: "Xerath", roles: &["UTILITY", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 102, name: "Shyvana", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 103, name: "Ahri", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 104, name: "Graves", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 105, name: "Fizz", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 106, name: "Volibear", roles: &["TOP", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 107, name: "Rengar", roles: &["JUNGLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 110, name: "Varus", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 111, name: "Nautilus", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 112, name: "Viktor", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 113, name: "Sejuani", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 114, name: "Fiora", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 117, name: "Lulu", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 119, name: "Draven", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 120, name: "Hecarim", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 121, name: "Kha'Zix", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 122, name: "Darius", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 126, name: "Jayce", roles: &["TOP", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 127, name: "Lissandra", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 131, name: "Diana", roles: &["JUNGLE", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 133, name: "Quinn", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 134, name: "Syndra", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 136, name: "Aurelion Sol", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 141, name: "Kayn", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 142, name: "Zoe", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 143, name: "Zyra", roles: &["JUNGLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 145, name: "Kai'Sa", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 147, name: "Seraphine", roles: &["UTILITY", "BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 150, name: "Gnar", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 154, name: "Zac", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 157, name: "Yasuo", roles: &["MIDDLE", "TOP", "BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 161, name: "Vel'Koz", roles: &["UTILITY", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 163, name: "Taliyah", roles: &["JUNGLE", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 164, name: "Camille", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 200, name: "Bel'Veth", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 201, name: "Braum", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 202, name: "Jhin", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 203, name: "Kindred", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 221, name: "Zeri", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 222, name: "Jinx", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 223, name: "Tahm Kench", roles: &["TOP", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 233, name: "Briar", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 234, name: "Viego", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 235, name: "Senna", roles: &["UTILITY", "BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 236, name: "Lucian", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 238, name: "Zed", roles: &["MIDDLE", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 240, name: "Kled", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 245, name: "Ekko", roles: &["JUNGLE", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 246, name: "Qiyana", roles: &["MIDDLE", "JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 254, name: "Vi", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 266, name: "Aatrox", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 267, name: "Nami", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 268, name: "Azir", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 350, name: "Yuumi", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 360, name: "Samira", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 412, name: "Thresh", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 420, name: "Illaoi", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 421, name: "Rek'Sai", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 427, name: "Ivern", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 429, name: "Kalista", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 432, name: "Bard", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 497, name: "Rakan", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 498, name: "Xayah", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 516, name: "Ornn", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 517, name: "Sylas", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 518, name: "Neeko", roles: &["MIDDLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 523, name: "Aphelios", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 526, name: "Rell", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 555, name: "Pyke", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 711, name: "Vex", roles: &["MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 777, name: "Yone", roles: &["MIDDLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 799, name: "Ambessa", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 800, name: "Mel", roles: &["MIDDLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 804, name: "Yunara", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 805, name: "Zaahen", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 875, name: "Sett", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 876, name: "Lillia", roles: &["JUNGLE"], aliases: &[] },
    BuiltinChampion { id: 887, name: "Gwen", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 888, name: "Renata Glasc", roles: &["UTILITY"], aliases: &["renata"] },
    BuiltinChampion { id: 893, name: "Aurora", roles: &["MIDDLE", "TOP"], aliases: &[] },
    BuiltinChampion { id: 895, name: "Nilah", roles: &["BOTTOM"], aliases: &[] },
    BuiltinChampion { id: 897, name: "K'Sante", roles: &["TOP"], aliases: &[] },
    BuiltinChampion { id: 901, name: "Smolder", roles: &["BOTTOM", "MIDDLE"], aliases: &[] },
    BuiltinChampion { id: 902, name: "Milio", roles: &["UTILITY"], aliases: &[] },
    BuiltinChampion { id: 910, name: "Hwei", roles: &["MIDDLE", "UTILITY"], aliases: &[] },
    BuiltinChampion { id: 950, name: "Naafiri", roles: &["MIDDLE", "JUNGLE"], aliases: &[] },
];

fn fold_latin_ascii(ch: char) -> char {
    match ch {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => 'y',
        other => other,
    }
}

/// Normalizes a champion name or internal alias key (`"Kai'Sa"`, `"LeeSin"`, `"Maître Yi"`, `"K'Santé"`)
/// into lowercase ASCII alphanumeric form for reliable ID lookup across locales.
pub fn normalize_champion_key(name: &str) -> String {
    name.chars()
        .map(fold_latin_ascii)
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Strips Riot Live Client `rawChampionName` localization prefixes (`game_character_displayname_MasterYi`).
pub fn extract_raw_champion_alias(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["game_character_displayname_", "character_"] {
        if lower.starts_with(prefix) {
            return trimmed[prefix.len()..].to_string();
        }
    }
    String::new()
}

/// Returns `true` when `name` is blank or represents an unselected / unknown champion placeholder.
pub fn is_champion_placeholder(name: &str) -> bool {
    matches!(
        normalize_champion_key(name).as_str(),
        "" | "selecting" | "selection" | "choosing" | "none" | "aucun" | "unknown" | "inconnu"
    )
}

/// Returns the built-in Summoner's Rift lane roles for `champion_id`, if present in the static catalog.
pub fn builtin_champion_roles(champion_id: i64) -> &'static [&'static str] {
    BUILTIN_CHAMPIONS
        .iter()
        .find(|c| c.id == champion_id)
        .map(|c| c.roles)
        .unwrap_or(&[])
}

/// Read-only champion resolution and catalog seam.
///
/// Pure `hypatia-shared` match projection runs against `&dyn ChampionLookup`, allowing
/// in-memory `BuiltinChampionCatalog` offline or `GameAssetCache` in `hypatia-core`
/// with synced LCU CommunityDragon locale metadata.
pub trait ChampionLookup {
    /// Resolves a Riot champion ID from its display name or alias (`0` when empty or placeholder).
    fn resolve_champion_id(&self, name: &str) -> i64;

    /// Resolves a Riot champion ID trying display name first, then internal alias (`rawChampionName`).
    fn resolve_champion_id_with_alias(&self, name: &str, alias: &str) -> i64 {
        let primary = self.resolve_champion_id(name);
        if primary > 0 || alias.trim().is_empty() {
            return primary;
        }
        self.resolve_champion_id(alias)
    }

    /// Resolves a localized champion display name from ID (`Selecting...`/`Sélection...` for `<= 0`).
    fn resolve_champion_name_localized(&self, champion_id: i64, lang: Lang) -> String;

    /// Returns the champion roster sorted alphabetically for pick/ban selectors.
    fn champion_catalog(&self) -> Vec<RemoteChampionOption> {
        BuiltinChampionCatalog.champion_catalog()
    }
}

/// Pure offline champion catalog backed by [`BUILTIN_CHAMPIONS`].
#[derive(Debug, Default, Clone, Copy)]
pub struct BuiltinChampionCatalog;

impl ChampionLookup for BuiltinChampionCatalog {
    fn resolve_champion_id(&self, name: &str) -> i64 {
        if is_champion_placeholder(name) {
            return 0;
        }
        let key = normalize_champion_key(name);
        BUILTIN_CHAMPIONS
            .iter()
            .find(|c| normalize_champion_key(c.name) == key || c.aliases.contains(&key.as_str()))
            .map(|c| c.id)
            .unwrap_or(0)
    }

    fn resolve_champion_name_localized(&self, champion_id: i64, lang: Lang) -> String {
        if champion_id <= 0 {
            return lang.pick("Selecting...", "Sélection...").to_string();
        }
        BUILTIN_CHAMPIONS
            .iter()
            .find(|c| c.id == champion_id)
            .map(|c| c.name.to_string())
            .unwrap_or_else(|| format!("Champion #{champion_id}"))
    }

    fn champion_catalog(&self) -> Vec<RemoteChampionOption> {
        let mut out: Vec<RemoteChampionOption> = BUILTIN_CHAMPIONS
            .iter()
            .map(|c| RemoteChampionOption {
                id: c.id,
                name: c.name.to_string(),
                roles: c.roles.iter().map(|r| (*r).to_string()).collect(),
                pickable: true,
                bannable: true,
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builtin_catalog_has_unique_ids_and_keys() {
        let mut ids = HashSet::new();
        let mut keys = HashSet::new();
        for champ in BUILTIN_CHAMPIONS {
            assert!(ids.insert(champ.id), "duplicate champion id {}", champ.id);
            let primary_key = normalize_champion_key(champ.name);
            assert!(keys.insert(primary_key), "duplicate primary key for {}", champ.name);
            for alias in champ.aliases {
                assert!(keys.insert((*alias).to_string()), "duplicate alias {alias}");
            }
        }
    }

    #[test]
    fn resolves_champions_aliases_and_placeholders_bidirectionally() {
        let cat = BuiltinChampionCatalog;
        assert_eq!(cat.resolve_champion_id("Ahri"), 103);
        assert_eq!(cat.resolve_champion_id("Kai'Sa"), 145);
        assert_eq!(cat.resolve_champion_id("KaiSa"), 145);
        assert_eq!(cat.resolve_champion_id("Lee Sin"), 64);
        assert_eq!(cat.resolve_champion_id("LeeSin"), 64);
        assert_eq!(cat.resolve_champion_id("Wukong"), 62);
        assert_eq!(cat.resolve_champion_id("MonkeyKing"), 62);
        assert_eq!(cat.resolve_champion_id("Maître Yi"), 11);
        assert_eq!(cat.resolve_champion_id("K'Santé"), 897);
        assert_eq!(cat.resolve_champion_id("Nunu et Willump"), 20);
        assert_eq!(cat.resolve_champion_id("Dr. Mundo"), 36);
        assert_eq!(cat.resolve_champion_id("Mundo"), 36);
        assert_eq!(cat.resolve_champion_name_localized(11, Lang::En), "Master Yi");
        assert_eq!(cat.resolve_champion_name_localized(897, Lang::En), "K'Sante");
        assert_eq!(cat.resolve_champion_name_localized(0, Lang::En), "Selecting...");
        assert_eq!(cat.resolve_champion_name_localized(0, Lang::Fr), "Sélection...");
        assert_eq!(cat.resolve_champion_id("Sélection..."), 0);
        assert_eq!(cat.resolve_champion_id("Selecting..."), 0);
        assert_eq!(
            extract_raw_champion_alias("game_character_displayname_MasterYi"),
            "MasterYi"
        );
        assert_eq!(
            cat.resolve_champion_id_with_alias("UnlocalizedName", &extract_raw_champion_alias("game_character_displayname_MasterYi")),
            11
        );
    }

    #[test]
    fn normalizes_positions_across_riot_tokens() {
        assert_eq!(Position::parse("TOP"), Some(Position::Top));
        assert_eq!(Position::parse("jgl"), Some(Position::Jungle));
        assert_eq!(Position::parse("MIDDLE"), Some(Position::Mid));
        assert_eq!(Position::parse("mid"), Some(Position::Mid));
        assert_eq!(Position::parse("BOTTOM"), Some(Position::Bot));
        assert_eq!(Position::parse("UTILITY"), Some(Position::Support));
        assert_eq!(Position::parse("SUPPORT"), Some(Position::Support));
        assert_eq!(Position::parse("FILL"), Some(Position::Fill));
        assert_eq!(Position::parse("NONE"), None);
        assert_eq!(Position::Support.desktop_index(), 4);
        assert_eq!(Position::Support.short_tag(), "SUP");
        assert_eq!(Position::Support.lcu_code(), "UTILITY");
        assert_eq!(Position::Fill.display_label(Lang::Fr), "Polyvalent");
    }
}
