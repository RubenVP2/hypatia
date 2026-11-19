use serde::{Deserialize, Serialize};

use crate::game_text::{game_name, game_names, GameTextKind};
use crate::i18n::Lang;
use crate::remote::{
    RemoteChampionOption, RemotePerkOption, RemoteRunePage, RemoteRuneTree, RemoteSkinOption,
    RemoteSpellOption,
};

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
    BuiltinChampion {
        id: 1,
        name: "Annie",
        roles: &["MIDDLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 2,
        name: "Olaf",
        roles: &["TOP", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 3,
        name: "Galio",
        roles: &["MIDDLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 4,
        name: "Twisted Fate",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 5,
        name: "Xin Zhao",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 6,
        name: "Urgot",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 7,
        name: "LeBlanc",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 8,
        name: "Vladimir",
        roles: &["MIDDLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 9,
        name: "Fiddlesticks",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 10,
        name: "Kayle",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 11,
        name: "Master Yi",
        roles: &["JUNGLE"],
        aliases: &["maitreyi"],
    },
    BuiltinChampion {
        id: 12,
        name: "Alistar",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 13,
        name: "Ryze",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 14,
        name: "Sion",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 15,
        name: "Sivir",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 16,
        name: "Soraka",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 17,
        name: "Teemo",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 18,
        name: "Tristana",
        roles: &["BOTTOM", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 19,
        name: "Warwick",
        roles: &["JUNGLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 20,
        name: "Nunu & Willump",
        roles: &["JUNGLE"],
        aliases: &["nunu", "nunuetwillump"],
    },
    BuiltinChampion {
        id: 21,
        name: "Miss Fortune",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 22,
        name: "Ashe",
        roles: &["BOTTOM", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 23,
        name: "Tryndamere",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 24,
        name: "Jax",
        roles: &["TOP", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 25,
        name: "Morgana",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 26,
        name: "Zilean",
        roles: &["UTILITY", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 27,
        name: "Singed",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 28,
        name: "Evelynn",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 29,
        name: "Twitch",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 30,
        name: "Karthus",
        roles: &["JUNGLE", "BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 31,
        name: "Cho'Gath",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 32,
        name: "Amumu",
        roles: &["JUNGLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 33,
        name: "Rammus",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 34,
        name: "Anivia",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 35,
        name: "Shaco",
        roles: &["JUNGLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 36,
        name: "Dr. Mundo",
        roles: &["TOP", "JUNGLE"],
        aliases: &["mundo"],
    },
    BuiltinChampion {
        id: 37,
        name: "Sona",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 38,
        name: "Kassadin",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 39,
        name: "Irelia",
        roles: &["TOP", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 40,
        name: "Janna",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 41,
        name: "Gangplank",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 42,
        name: "Corki",
        roles: &["BOTTOM", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 43,
        name: "Karma",
        roles: &["UTILITY", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 44,
        name: "Taric",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 45,
        name: "Veigar",
        roles: &["MIDDLE", "BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 48,
        name: "Trundle",
        roles: &["TOP", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 50,
        name: "Swain",
        roles: &["UTILITY", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 51,
        name: "Caitlyn",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 53,
        name: "Blitzcrank",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 54,
        name: "Malphite",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 55,
        name: "Katarina",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 56,
        name: "Nocturne",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 57,
        name: "Maokai",
        roles: &["UTILITY", "JUNGLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 58,
        name: "Renekton",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 59,
        name: "Jarvan IV",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 60,
        name: "Elise",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 61,
        name: "Orianna",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 62,
        name: "Wukong",
        roles: &["JUNGLE", "TOP"],
        aliases: &["monkeyking"],
    },
    BuiltinChampion {
        id: 63,
        name: "Brand",
        roles: &["UTILITY", "MIDDLE", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 64,
        name: "Lee Sin",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 67,
        name: "Vayne",
        roles: &["BOTTOM", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 68,
        name: "Rumble",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 69,
        name: "Cassiopeia",
        roles: &["MIDDLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 72,
        name: "Skarner",
        roles: &["JUNGLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 74,
        name: "Heimerdinger",
        roles: &["MIDDLE", "TOP", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 75,
        name: "Nasus",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 76,
        name: "Nidalee",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 77,
        name: "Udyr",
        roles: &["JUNGLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 78,
        name: "Poppy",
        roles: &["JUNGLE", "TOP", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 79,
        name: "Gragas",
        roles: &["TOP", "JUNGLE", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 80,
        name: "Pantheon",
        roles: &["TOP", "MIDDLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 81,
        name: "Ezreal",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 82,
        name: "Mordekaiser",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 83,
        name: "Yorick",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 84,
        name: "Akali",
        roles: &["MIDDLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 85,
        name: "Kennen",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 86,
        name: "Garen",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 89,
        name: "Leona",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 90,
        name: "Malzahar",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 91,
        name: "Talon",
        roles: &["MIDDLE", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 92,
        name: "Riven",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 96,
        name: "Kog'Maw",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 98,
        name: "Shen",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 99,
        name: "Lux",
        roles: &["UTILITY", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 101,
        name: "Xerath",
        roles: &["UTILITY", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 102,
        name: "Shyvana",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 103,
        name: "Ahri",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 104,
        name: "Graves",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 105,
        name: "Fizz",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 106,
        name: "Volibear",
        roles: &["TOP", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 107,
        name: "Rengar",
        roles: &["JUNGLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 110,
        name: "Varus",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 111,
        name: "Nautilus",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 112,
        name: "Viktor",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 113,
        name: "Sejuani",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 114,
        name: "Fiora",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 117,
        name: "Lulu",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 119,
        name: "Draven",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 120,
        name: "Hecarim",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 121,
        name: "Kha'Zix",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 122,
        name: "Darius",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 126,
        name: "Jayce",
        roles: &["TOP", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 127,
        name: "Lissandra",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 131,
        name: "Diana",
        roles: &["JUNGLE", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 133,
        name: "Quinn",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 134,
        name: "Syndra",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 136,
        name: "Aurelion Sol",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 141,
        name: "Kayn",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 142,
        name: "Zoe",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 143,
        name: "Zyra",
        roles: &["JUNGLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 145,
        name: "Kai'Sa",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 147,
        name: "Seraphine",
        roles: &["UTILITY", "BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 150,
        name: "Gnar",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 154,
        name: "Zac",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 157,
        name: "Yasuo",
        roles: &["MIDDLE", "TOP", "BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 161,
        name: "Vel'Koz",
        roles: &["UTILITY", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 163,
        name: "Taliyah",
        roles: &["JUNGLE", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 164,
        name: "Camille",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 200,
        name: "Bel'Veth",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 201,
        name: "Braum",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 202,
        name: "Jhin",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 203,
        name: "Kindred",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 221,
        name: "Zeri",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 222,
        name: "Jinx",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 223,
        name: "Tahm Kench",
        roles: &["TOP", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 233,
        name: "Briar",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 234,
        name: "Viego",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 235,
        name: "Senna",
        roles: &["UTILITY", "BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 236,
        name: "Lucian",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 238,
        name: "Zed",
        roles: &["MIDDLE", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 240,
        name: "Kled",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 245,
        name: "Ekko",
        roles: &["JUNGLE", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 246,
        name: "Qiyana",
        roles: &["MIDDLE", "JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 254,
        name: "Vi",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 266,
        name: "Aatrox",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 267,
        name: "Nami",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 268,
        name: "Azir",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 350,
        name: "Yuumi",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 360,
        name: "Samira",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 412,
        name: "Thresh",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 420,
        name: "Illaoi",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 421,
        name: "Rek'Sai",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 427,
        name: "Ivern",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 429,
        name: "Kalista",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 432,
        name: "Bard",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 497,
        name: "Rakan",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 498,
        name: "Xayah",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 516,
        name: "Ornn",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 517,
        name: "Sylas",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 518,
        name: "Neeko",
        roles: &["MIDDLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 523,
        name: "Aphelios",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 526,
        name: "Rell",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 555,
        name: "Pyke",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 711,
        name: "Vex",
        roles: &["MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 777,
        name: "Yone",
        roles: &["MIDDLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 799,
        name: "Ambessa",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 800,
        name: "Mel",
        roles: &["MIDDLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 804,
        name: "Yunara",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 805,
        name: "Zaahen",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 875,
        name: "Sett",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 876,
        name: "Lillia",
        roles: &["JUNGLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 887,
        name: "Gwen",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 888,
        name: "Renata Glasc",
        roles: &["UTILITY"],
        aliases: &["renata"],
    },
    BuiltinChampion {
        id: 893,
        name: "Aurora",
        roles: &["MIDDLE", "TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 895,
        name: "Nilah",
        roles: &["BOTTOM"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 897,
        name: "K'Sante",
        roles: &["TOP"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 901,
        name: "Smolder",
        roles: &["BOTTOM", "MIDDLE"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 902,
        name: "Milio",
        roles: &["UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 910,
        name: "Hwei",
        roles: &["MIDDLE", "UTILITY"],
        aliases: &[],
    },
    BuiltinChampion {
        id: 950,
        name: "Naafiri",
        roles: &["MIDDLE", "JUNGLE"],
        aliases: &[],
    },
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

    /// Returns the champion roster, named in `lang` and sorted alphabetically, for pick/ban
    /// selectors.
    fn champion_catalog(&self, lang: Lang) -> Vec<RemoteChampionOption> {
        BuiltinChampionCatalog.champion_catalog(lang)
    }
}

/// A champion's name in `lang` from Game Text, else the built-in English name.
pub fn builtin_champion_name(champion_id: i64, lang: Lang) -> Option<&'static str> {
    game_name(GameTextKind::Champion, champion_id, lang).or_else(|| {
        BUILTIN_CHAMPIONS
            .iter()
            .find(|c| c.id == champion_id)
            .map(|c| c.name)
    })
}

/// Resolves a normalized champion key from any Hypatia language's Game Text name, a built-in
/// name, or a built-in alias.
fn builtin_champion_id(key: &str) -> Option<i64> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static BY_KEY: OnceLock<HashMap<String, i64>> = OnceLock::new();
    BY_KEY
        .get_or_init(|| {
            let mut map = HashMap::new();
            for c in BUILTIN_CHAMPIONS {
                map.insert(normalize_champion_key(c.name), c.id);
                for alias in c.aliases {
                    map.insert((*alias).to_string(), c.id);
                }
            }
            for lang in [Lang::En, Lang::Fr] {
                for &(id, name) in game_names(GameTextKind::Champion, lang) {
                    map.entry(normalize_champion_key(name)).or_insert(id);
                }
            }
            map
        })
        .get(key)
        .copied()
}

/// Pure offline champion catalog backed by [`BUILTIN_CHAMPIONS`].
#[derive(Debug, Default, Clone, Copy)]
pub struct BuiltinChampionCatalog;

impl ChampionLookup for BuiltinChampionCatalog {
    fn resolve_champion_id(&self, name: &str) -> i64 {
        if is_champion_placeholder(name) {
            return 0;
        }
        builtin_champion_id(&normalize_champion_key(name)).unwrap_or(0)
    }

    fn resolve_champion_name_localized(&self, champion_id: i64, lang: Lang) -> String {
        if champion_id <= 0 {
            return lang.pick("Selecting...", "Sélection...").to_string();
        }
        builtin_champion_name(champion_id, lang)
            .map(str::to_string)
            .unwrap_or_else(|| format!("Champion #{champion_id}"))
    }

    fn champion_catalog(&self, lang: Lang) -> Vec<RemoteChampionOption> {
        let mut out: Vec<RemoteChampionOption> = game_names(GameTextKind::Champion, lang)
            .iter()
            .map(|&(id, name)| (id, name))
            .chain(
                BUILTIN_CHAMPIONS
                    .iter()
                    .filter(|c| game_name(GameTextKind::Champion, c.id, lang).is_none())
                    .map(|c| (c.id, c.name)),
            )
            .map(|(id, name)| RemoteChampionOption {
                id,
                name: name.to_string(),
                roles: builtin_champion_roles(id)
                    .iter()
                    .map(|r| (*r).to_string())
                    .collect(),
                pickable: true,
                bannable: true,
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

/// Localized static catalog of Summoner's Rift summoner spells.
pub fn default_summoner_spells_localized(lang: Lang) -> Vec<RemoteSpellOption> {
    const SPELLS: &[(i64, &str, &str, &str, &str, &str, &str)] = &[
        (
            4,
            "Flash",
            "Saut éclair",
            "F",
            "S",
            "300s",
            "Blink a short distance toward your cursor.",
        ),
        (
            14,
            "Ignite",
            "Embrasement",
            "I",
            "E",
            "180s",
            "Ignites target enemy champion for true damage and Grievous Wounds.",
        ),
        (
            12,
            "Teleport",
            "Téléportation",
            "T",
            "T",
            "360s",
            "Channel to teleport to an allied structure, minion, or ward.",
        ),
        (
            11,
            "Smite",
            "Châtiment",
            "S",
            "C",
            "15s",
            "Deals true damage to a monster or minion. Required for Junglers.",
        ),
        (
            7,
            "Heal",
            "Soins",
            "H",
            "So",
            "240s",
            "Restores health and grants movement speed to you and a nearby ally.",
        ),
        (
            21,
            "Barrier",
            "Barrière",
            "B",
            "B",
            "180s",
            "Shields your champion from damage for 2.5 seconds.",
        ),
        (
            3,
            "Exhaust",
            "Fatigue",
            "E",
            "F",
            "210s",
            "Slows target enemy champion and reduces their damage dealt by 35%.",
        ),
        (
            6,
            "Ghost",
            "Fantôme",
            "G",
            "Fa",
            "240s",
            "Gain movement speed and ignore unit collision for 15 seconds.",
        ),
        (
            1,
            "Cleanse",
            "Purge",
            "C",
            "P",
            "210s",
            "Removes all disables (excluding suppression and airborne) and summoner spell debuffs.",
        ),
    ];
    let fr_descs: &[&str] = &[
        "Téléporte votre champion sur une courte distance vers votre curseur.",
        "Enflamme un champion ennemi, infligeant des dégâts bruts et Hémorragie.",
        "Canalise pour vous téléporter vers une structure, un sbire ou une balise alliée.",
        "Inflige des dégâts bruts à un monstre ou un sbire. Requis pour la jungle.",
        "Rend des PV et confère un bonus de vitesse à vous et à un allié proche.",
        "Protège votre champion des dégâts pendant 2,5 secondes.",
        "Ralentit le champion ennemi ciblé et réduit ses dégâts infligés de 35 %.",
        "Augmente la vitesse de déplacement et ignore les collisions pendant 15 secondes.",
        "Dissipe toutes les entraves (hors neutralisation et projection) et sorts d'invocateur.",
    ];
    SPELLS
        .iter()
        .zip(fr_descs.iter())
        .map(
            |(&(id, en_n, fr_n, en_k, fr_k, cd, en_d), &fr_d)| RemoteSpellOption {
                id,
                name: game_name(GameTextKind::Spell, id, lang)
                    .unwrap_or(lang.pick(en_n, fr_n))
                    .to_string(),
                key: lang.pick(en_k, fr_k).to_string(),
                cooldown: cd.to_string(),
                description: lang.pick(en_d, fr_d).to_string(),
            },
        )
        .collect()
}

pub fn spell_name_from_id_localized(spell_id: i64, lang: Lang) -> &'static str {
    game_name(GameTextKind::Spell, spell_id, lang).unwrap_or(lang.pick("Spell", "Sort"))
}

fn perk_opt(
    lang: Lang,
    id: i64,
    en_n: &'static str,
    fr_n: &'static str,
    en_d: &'static str,
    fr_d: &'static str,
) -> RemotePerkOption {
    RemotePerkOption {
        id,
        name: game_name(GameTextKind::Perk, id, lang)
            .unwrap_or(lang.pick(en_n, fr_n))
            .to_string(),
        short_desc: lang.pick(en_d, fr_d).to_string(),
    }
}

/// Localized Rune Trees (Precision, Domination, Sorcery, Resolve, Inspiration) with real Riot Perk IDs.
pub fn default_rune_trees_localized(lang: Lang) -> Vec<RemoteRuneTree> {
    vec![
        RemoteRuneTree {
            id: 8000,
            name: rune_style_name_localized(8000, lang).to_string(),
            tagline: lang
                .pick(
                    "Improved attacks and sustained damage",
                    "Attaques améliorées et dégâts soutenus",
                )
                .to_string(),
            keystones: vec![
                perk_opt(
                    lang,
                    8005,
                    "Press the Attack",
                    "Attaque soutenue",
                    "3 hits expose target",
                    "3 attaques exposent la cible",
                ),
                perk_opt(
                    lang,
                    8008,
                    "Lethal Tempo",
                    "Tempo mortel",
                    "Attack speed on hit",
                    "Vitesse d'attaque à l'impact",
                ),
                perk_opt(
                    lang,
                    8021,
                    "Fleet Footwork",
                    "Jeu de jambes",
                    "Energized heal + speed",
                    "Soin énergisé + vitesse",
                ),
                perk_opt(
                    lang,
                    8010,
                    "Conqueror",
                    "Conquérant",
                    "Stacking adaptive force + heal",
                    "Force adaptative cumulable + soin",
                ),
            ],
            slot1: vec![
                perk_opt(
                    lang,
                    9101,
                    "Absorb Life",
                    "Absorption vitale",
                    "Killing targets heals",
                    "Éliminer une cible vous soigne",
                ),
                perk_opt(
                    lang,
                    9111,
                    "Triumph",
                    "Triomphe",
                    "Takedowns restore 10% missing HP",
                    "Les éliminations rendent 10 % des PV manquants",
                ),
                perk_opt(
                    lang,
                    8009,
                    "Presence of Mind",
                    "Présence d'esprit",
                    "Restore mana/energy on takedown",
                    "Rend du mana/énergie sur élimination",
                ),
            ],
            slot2: vec![
                perk_opt(
                    lang,
                    9104,
                    "Legend: Alacrity",
                    "Légende : alacrité",
                    "Takedowns grant attack speed",
                    "Vitesse d'attaque par élimination",
                ),
                perk_opt(
                    lang,
                    9105,
                    "Legend: Haste",
                    "Légende : accélération",
                    "Takedowns grant basic ability haste",
                    "Accélération de compétence par élimination",
                ),
                perk_opt(
                    lang,
                    9103,
                    "Legend: Bloodline",
                    "Légende : sangsue",
                    "Takedowns grant life steal",
                    "Vol de vie par élimination",
                ),
            ],
            slot3: vec![
                perk_opt(
                    lang,
                    8014,
                    "Coup de Grace",
                    "Coup de grâce",
                    "Bonus damage to low HP enemies",
                    "Dégâts bonus aux ennemis affaiblis",
                ),
                perk_opt(
                    lang,
                    8017,
                    "Cut Down",
                    "Abattage",
                    "Bonus damage to high HP enemies",
                    "Dégâts bonus contre les cibles résistantes",
                ),
                perk_opt(
                    lang,
                    8299,
                    "Last Stand",
                    "Baroud d'honneur",
                    "Bonus damage while low HP",
                    "Dégâts bonus quand vos PV sont bas",
                ),
            ],
        },
        RemoteRuneTree {
            id: 8100,
            name: rune_style_name_localized(8100, lang).to_string(),
            tagline: lang
                .pick(
                    "Burst damage and target access",
                    "Dégâts explosifs et accès aux cibles",
                )
                .to_string(),
            keystones: vec![
                perk_opt(
                    lang,
                    8112,
                    "Electrocute",
                    "Électrocution",
                    "3 separate hits deal burst damage",
                    "3 coups distincts infligent un burst",
                ),
                perk_opt(
                    lang,
                    8128,
                    "Dark Harvest",
                    "Moisson noire",
                    "Damage low HP champs for souls",
                    "Récolte des âmes sur cibles faibles",
                ),
                perk_opt(
                    lang,
                    9923,
                    "Hail of Blades",
                    "Déluge de lames",
                    "110% attack speed for 3 attacks",
                    "+110 % vitesse d'attaque sur 3 coups",
                ),
            ],
            slot1: vec![
                perk_opt(
                    lang,
                    8126,
                    "Cheap Shot",
                    "Coup bas",
                    "True damage to impaired targets",
                    "Dégâts bruts sur cibles entravées",
                ),
                perk_opt(
                    lang,
                    8139,
                    "Taste of Blood",
                    "Goût du sang",
                    "Heal when damaging an enemy",
                    "Soigne en blessant un ennemi",
                ),
                perk_opt(
                    lang,
                    8143,
                    "Sudden Impact",
                    "Ruée offensive",
                    "Bonus damage after dash/stealth",
                    "Dégâts bonus après ruée ou furtivité",
                ),
            ],
            slot2: vec![
                perk_opt(
                    lang,
                    8136,
                    "Zombie Ward",
                    "Balise zombie",
                    "Takedowns on wards spawn allied wards",
                    "Détruire une balise crée une balise alliée",
                ),
                perk_opt(
                    lang,
                    8120,
                    "Ghost Poro",
                    "Poro fantôme",
                    "Expired wards leave a Ghost Poro",
                    "Les balises expirées laissent un Poro fantôme",
                ),
                perk_opt(
                    lang,
                    8138,
                    "Eyeball Collection",
                    "Arracheur d'œil",
                    "Stacking adaptive force on takedowns",
                    "Force adaptative cumulée sur élimination",
                ),
            ],
            slot3: vec![
                perk_opt(
                    lang,
                    8135,
                    "Treasure Hunter",
                    "Chasseur de trésors",
                    "Bonus gold on unique takedowns",
                    "Or bonus sur éliminations uniques",
                ),
                perk_opt(
                    lang,
                    8105,
                    "Relentless Hunter",
                    "Chasseur acharné",
                    "Out-of-combat movement speed",
                    "Vitesse de déplacement hors combat",
                ),
                perk_opt(
                    lang,
                    8106,
                    "Ultimate Hunter",
                    "Chasseur ultime",
                    "Ultimate ability haste stacks",
                    "Accélération d'ultime cumulable",
                ),
            ],
        },
        RemoteRuneTree {
            id: 8200,
            name: rune_style_name_localized(8200, lang).to_string(),
            tagline: lang
                .pick(
                    "Empowered abilities and resource manipulation",
                    "Compétences renforcées et gestion des ressources",
                )
                .to_string(),
            keystones: vec![
                perk_opt(
                    lang,
                    8214,
                    "Summon Aery",
                    "Invocation d'Aery",
                    "Damage enemies or shield allies",
                    "Blesse les ennemis ou protège les alliés",
                ),
                perk_opt(
                    lang,
                    8229,
                    "Arcane Comet",
                    "Comète arcanique",
                    "Ability hits hurl a comet",
                    "Vos sorts lancent une comète",
                ),
                perk_opt(
                    lang,
                    8230,
                    "Phase Rush",
                    "Rush phasique",
                    "3 hits grant burst of speed",
                    "3 coups donnent un boost de vitesse",
                ),
            ],
            slot1: vec![
                perk_opt(
                    lang,
                    8224,
                    "Nullifying Orb",
                    "Orbe anti-magie",
                    "Magic shield at low HP",
                    "Bouclier magique à PV faibles",
                ),
                perk_opt(
                    lang,
                    8226,
                    "Manaflow Band",
                    "Ruban de mana",
                    "+250 max mana and regen",
                    "+250 mana max et régénération",
                ),
                perk_opt(
                    lang,
                    8275,
                    "Nimbus Cloak",
                    "Manteau nuageux",
                    "Speed after Summoner Spell",
                    "Vitesse après sort d'invocateur",
                ),
            ],
            slot2: vec![
                perk_opt(
                    lang,
                    8210,
                    "Transcendence",
                    "Transcendance",
                    "Ability haste + cooldown refund",
                    "Accélération + réduction des délais",
                ),
                perk_opt(
                    lang,
                    8234,
                    "Celerity",
                    "Célérité",
                    "Movement speed bonuses amplified",
                    "Bonus de vitesse de déplacement amplifiés",
                ),
                perk_opt(
                    lang,
                    8233,
                    "Absolute Focus",
                    "Concentration absolue",
                    "Adaptive force above 70% HP",
                    "Force adaptative au-dessus de 70 % PV",
                ),
            ],
            slot3: vec![
                perk_opt(
                    lang,
                    8237,
                    "Scorch",
                    "Brûlure",
                    "Abilities burn enemies in lane",
                    "Les sorts brûlent les ennemis en voie",
                ),
                perk_opt(
                    lang,
                    8232,
                    "Waterwalking",
                    "Marche sur l'eau",
                    "Speed and force in the river",
                    "Vitesse et force dans la rivière",
                ),
                perk_opt(
                    lang,
                    8236,
                    "Gathering Storm",
                    "Tempête menaçante",
                    "Scaling adaptive force every 10m",
                    "Force adaptative croissante chaque 10 min",
                ),
            ],
        },
        RemoteRuneTree {
            id: 8400,
            name: rune_style_name_localized(8400, lang).to_string(),
            tagline: lang
                .pick(
                    "Durability and crowd control",
                    "Durabilité et contrôle de foule",
                )
                .to_string(),
            keystones: vec![
                perk_opt(
                    lang,
                    8437,
                    "Grasp of the Undying",
                    "Poigne de l'immortel",
                    "Every 4s in combat heal + max HP",
                    "Soin + PV max toutes les 4s en combat",
                ),
                perk_opt(
                    lang,
                    8439,
                    "Aftershock",
                    "Après-coup",
                    "Immobilizing grants resists + burst",
                    "Immobiliser confère résistances + explosion",
                ),
                perk_opt(
                    lang,
                    8465,
                    "Guardian",
                    "Gardien",
                    "Shield nearby allies on damage",
                    "Bouclier sur allié proche subissant des dégâts",
                ),
            ],
            slot1: vec![
                perk_opt(
                    lang,
                    8446,
                    "Demolish",
                    "Démolition",
                    "Charge powerful attack vs towers",
                    "Charge une attaque puissante sur tourelles",
                ),
                perk_opt(
                    lang,
                    8463,
                    "Font of Life",
                    "Fontaine de vie",
                    "Impairing enemies heals allies",
                    "Entraver un ennemi soigne les alliés",
                ),
                perk_opt(
                    lang,
                    8401,
                    "Shield Bash",
                    "Coup de bouclier",
                    "Shields grant resists + bonus hit",
                    "Bouclier confère résistances + coup bonus",
                ),
            ],
            slot2: vec![
                perk_opt(
                    lang,
                    8429,
                    "Conditioning",
                    "Conditionnement",
                    "Bonus Armor and MR at 12m",
                    "Armure et RM bonus à 12 min",
                ),
                perk_opt(
                    lang,
                    8444,
                    "Second Wind",
                    "Second souffle",
                    "Regenerate HP after taking poke",
                    "Régénère des PV après avoir subi des dégâts",
                ),
                perk_opt(
                    lang,
                    8473,
                    "Bone Plating",
                    "Plaque d'os",
                    "Reduce damage from 3 combo hits",
                    "Réduit les dégâts de 3 coups rapides",
                ),
            ],
            slot3: vec![
                perk_opt(
                    lang,
                    8451,
                    "Overgrowth",
                    "Surcroissance",
                    "Permanent max HP from nearby minions",
                    "PV max permanents près des sbires",
                ),
                perk_opt(
                    lang,
                    8453,
                    "Revitalize",
                    "Revitalisation",
                    "5% stronger heals and shields",
                    "Soins et boucliers 5 % plus efficaces",
                ),
                perk_opt(
                    lang,
                    8242,
                    "Unflinching",
                    "Inébranlable",
                    "Bonus Armor/MR when CC'd",
                    "Armure/RM bonus sous contrôle de foule",
                ),
            ],
        },
        RemoteRuneTree {
            id: 8300,
            name: rune_style_name_localized(8300, lang).to_string(),
            tagline: lang
                .pick(
                    "Creative tools and rule bending",
                    "Outils créatifs et détournement des règles",
                )
                .to_string(),
            keystones: vec![
                perk_opt(
                    lang,
                    8351,
                    "Glacial Augment",
                    "Optimisation glaciale",
                    "Immobilizing creates slow rays",
                    "Immobiliser crée des rayons ralentissants",
                ),
                perk_opt(
                    lang,
                    8360,
                    "Unsealed Spellbook",
                    "Grimoire déchaîné",
                    "Swap Summoner Spells in game",
                    "Changez de sort d'invocateur en partie",
                ),
                perk_opt(
                    lang,
                    8369,
                    "First Strike",
                    "Premier coup",
                    "Bonus damage + gold when striking first",
                    "Dégâts bonus + or en frappant le premier",
                ),
            ],
            slot1: vec![
                perk_opt(
                    lang,
                    8306,
                    "Hextech Flashtraption",
                    "Canaliportation Hextech",
                    "Channel Flash while on cooldown",
                    "Canalise un Saut éclair en récupération",
                ),
                perk_opt(
                    lang,
                    8304,
                    "Magical Footwear",
                    "Chaussures magiques",
                    "Free boots with +10 move speed",
                    "Bottes gratuites avec +10 vitesse",
                ),
                perk_opt(
                    lang,
                    8321,
                    "Cash Back",
                    "Remboursement",
                    "6% gold back on Legendary items",
                    "6 % d'or rendu par objet légendaire",
                ),
            ],
            slot2: vec![
                perk_opt(
                    lang,
                    8313,
                    "Triple Tonic",
                    "Triple tonique",
                    "Elixirs at levels 3, 6, and 9",
                    "Élixirs aux niveaux 3, 6 et 9",
                ),
                perk_opt(
                    lang,
                    8352,
                    "Time Warp Tonic",
                    "Philtre de chronodistorsion",
                    "Immediate potion restoration",
                    "Soin instantané via les potions",
                ),
                perk_opt(
                    lang,
                    8345,
                    "Biscuit Delivery",
                    "Livraison de biscuits",
                    "Receive 3 sustain biscuits",
                    "Reçoit 3 biscuits de régénération",
                ),
            ],
            slot3: vec![
                perk_opt(
                    lang,
                    8347,
                    "Cosmic Insight",
                    "Savoir cosmique",
                    "+18 Summoner Spell & Item Haste",
                    "+18 accélération sorts & objets",
                ),
                perk_opt(
                    lang,
                    8410,
                    "Approach Velocity",
                    "Vitesse d'approche",
                    "Bonus speed toward impaired enemies",
                    "Vitesse bonus vers ennemis entravés",
                ),
                perk_opt(
                    lang,
                    8316,
                    "Jack Of All Trades",
                    "Touche-à-tout",
                    "Haste and force per unique stat",
                    "Accélération et force par stat unique",
                ),
            ],
        },
    ]
}

pub fn rune_style_name_localized(style_id: i64, lang: Lang) -> &'static str {
    game_name(GameTextKind::PerkStyle, style_id, lang).unwrap_or("Runes")
}

pub fn keystone_name_from_id_localized(perk_id: i64, lang: Lang) -> &'static str {
    game_name(GameTextKind::Perk, perk_id, lang)
        .unwrap_or(lang.pick("Custom Keystone", "Rune fondamentale"))
}

/// Localized built-in recommended rune pages tailored to champion & lane so one tap always works offline.
pub fn recommended_pages_for_localized(
    champion_id: i64,
    position: &str,
    lang: Lang,
) -> Vec<RemoteRunePage> {
    let pos = position.to_ascii_uppercase();
    let (en_name, fr_name, p_style, s_style, perks) = match (champion_id, pos.as_str()) {
        (103 | 238 | 7 | 99 | 61 | 134 | 112 | 518 | 142 | 4, _) => (
            "Burst & Roam (Electrocute)",
            "Burst & Décalage (Électrocution)",
            8100,
            8200,
            vec![8112, 8139, 8138, 8106, 8226, 8210, 5008, 5008, 5001],
        ),
        (266 | 122 | 86 | 64 | 234 | 157 | 777 | 92 | 164 | 24 | 233, _) => (
            "Sustained Skirmish (Conqueror)",
            "Escarmouche prolongée (Conquérant)",
            8000,
            8400,
            vec![8010, 9111, 9105, 8299, 8444, 8451, 5005, 5008, 5001],
        ),
        (222 | 145 | 67 | 81 | 51 | 22 | 202 | 236, _) | (_, "BOTTOM" | "BOT") => (
            "Marksman DPS (Lethal Tempo)",
            "Tireur DPS (Tempo mortel)",
            8000,
            8200,
            vec![8008, 9101, 9103, 8014, 8234, 8236, 5005, 5008, 5001],
        ),
        (412 | 89 | 111 | 53 | 516 | 54 | 98, _) | (_, "UTILITY" | "SUPPORT") => (
            "Engage & Frontline (Aftershock)",
            "Engagement & Frontline (Après-coup)",
            8400,
            8300,
            vec![8439, 8463, 8473, 8242, 8345, 8347, 5007, 5001, 5001],
        ),
        (_, "TOP") => (
            "Top Lane Duelist (Grasp)",
            "Duelliste Top (Poigne)",
            8400,
            8000,
            vec![8437, 8446, 8444, 8451, 9111, 9104, 5005, 5008, 5001],
        ),
        (_, "JUNGLE") => (
            "Jungle Tempo (Conqueror)",
            "Tempo Jungle (Conquérant)",
            8000,
            8300,
            vec![8010, 9111, 9104, 8014, 8304, 8347, 5005, 5008, 5001],
        ),
        _ => (
            "All-Round Mage (Arcane Comet)",
            "Mage polyvalent (Comète arcanique)",
            8200,
            8300,
            vec![8229, 8226, 8210, 8237, 8345, 8347, 5008, 5008, 5001],
        ),
    };

    vec![
        RemoteRunePage {
            id: -1,
            name: lang.pick(en_name, fr_name).to_string(),
            primary_style_id: p_style,
            primary_style_name: rune_style_name_localized(p_style, lang).to_string(),
            sub_style_id: s_style,
            sub_style_name: rune_style_name_localized(s_style, lang).to_string(),
            keystone_name: keystone_name_from_id_localized(perks[0], lang).to_string(),
            selected_perk_ids: perks,
            current: false,
            is_preset: true,
        },
        RemoteRunePage {
            id: -2,
            name: lang
                .pick(
                    "Poke & Scaling (Arcane Comet)",
                    "Poke & Scaling (Comète arcanique)",
                )
                .to_string(),
            primary_style_id: 8200,
            primary_style_name: rune_style_name_localized(8200, lang).to_string(),
            sub_style_id: 8300,
            sub_style_name: rune_style_name_localized(8300, lang).to_string(),
            keystone_name: keystone_name_from_id_localized(8229, lang).to_string(),
            selected_perk_ids: vec![8229, 8226, 8210, 8236, 8345, 8347, 5008, 5008, 5001],
            current: false,
            is_preset: true,
        },
        RemoteRunePage {
            id: -3,
            name: lang
                .pick(
                    "Safe Lane Sustain (Fleet Footwork)",
                    "Tenue de voie sûre (Jeu de jambes)",
                )
                .to_string(),
            primary_style_id: 8000,
            primary_style_name: rune_style_name_localized(8000, lang).to_string(),
            sub_style_id: 8400,
            sub_style_name: rune_style_name_localized(8400, lang).to_string(),
            keystone_name: keystone_name_from_id_localized(8021, lang).to_string(),
            selected_perk_ids: vec![8021, 9101, 9104, 8014, 8444, 8451, 5005, 5008, 5001],
            current: false,
            is_preset: true,
        },
    ]
}

/// Generates realistic skins for any champion so skin picking works out of the box in Demo mode
/// and falls back cleanly when LCU carousel skins are not yet loaded.
pub fn builtin_skins_for_localized(
    champion_id: i64,
    champion_name: &str,
    selected_skin_id: i64,
    lang: Lang,
) -> Vec<RemoteSkinOption> {
    if champion_id <= 0 {
        return Vec::new();
    }
    let base = champion_id * 1000;
    let current = if selected_skin_id > 0 {
        selected_skin_id
    } else {
        base
    };
    let themes: Vec<(i64, &str, &str)> = match champion_id {
        103 => vec![
            (base, "Classic Ahri", "Ahri classique"),
            (base + 1, "Dynasty Ahri", "Ahri dynastique"),
            (base + 4, "Popstar Ahri", "Popstar Ahri"),
            (
                base + 14,
                "Star Guardian Ahri",
                "Ahri gardienne des étoiles",
            ),
            (base + 27, "Spirit Blossom Ahri", "Ahri fleur spirituelle"),
            (base + 42, "Coven Ahri", "Ahri congrégation des sorcières"),
            (
                base + 85,
                "Immortalized Legend Ahri",
                "Ahri légende immortalisée",
            ),
        ],
        64 => vec![
            (base, "Classic Lee Sin", "Lee Sin classique"),
            (base + 4, "Muay Thai Lee Sin", "Lee Sin muay thaï"),
            (base + 11, "God Fist Lee Sin", "Lee Sin poing divin"),
            (
                base + 27,
                "Storm Dragon Lee Sin",
                "Lee Sin dragon des tempêtes",
            ),
            (base + 51, "Heavenscale Lee Sin", "Lee Sin écaille divine"),
        ],
        222 => vec![
            (base, "Classic Jinx", "Jinx classique"),
            (base + 4, "Star Guardian Jinx", "Jinx gardienne des étoiles"),
            (base + 13, "PROJECT: Jinx", "PROJET : Jinx"),
            (
                base + 29,
                "Arcane Fractured Jinx",
                "Jinx fracturée d'Arcane",
            ),
            (base + 37, "Battle Cat Jinx", "Jinx chat de combat"),
        ],
        266 => vec![
            (base, "Classic Aatrox", "Aatrox classique"),
            (base + 7, "Blood Moon Aatrox", "Aatrox lune de sang"),
            (base + 9, "Odyssey Aatrox", "Aatrox de l'Odyssée"),
            (base + 21, "DRX Aatrox", "DRX Aatrox"),
            (base + 30, "Primordian Aatrox", "Aatrox primordien"),
        ],
        412 => vec![
            (base, "Classic Thresh", "Thresh classique"),
            (base + 5, "Dark Star Thresh", "Thresh du pulsar sombre"),
            (base + 13, "High Noon Thresh", "Thresh de l'Ouest"),
            (
                base + 17,
                "Spirit Blossom Thresh",
                "Thresh fleur spirituelle",
            ),
        ],
        _ => vec![
            (base, "Classic", "classique"),
            (base + 1, "Blood Moon", "lune de sang"),
            (base + 2, "Star Guardian", "gardien des étoiles"),
            (base + 3, "Spirit Blossom", "fleur spirituelle"),
            (base + 4, "High Noon", "de l'Ouest"),
        ],
    };

    themes
        .into_iter()
        .map(|(id, en, fr)| {
            let label = lang.pick(en, fr);
            let name = if label.contains(champion_name) {
                label.to_string()
            } else if lang.is_fr() {
                format!("{champion_name} {label}")
            } else {
                format!("{label} {champion_name}")
            };
            RemoteSkinOption {
                id,
                name,
                selected: id == current,
                tile_path: format!(
                    "/lol-game-data/assets/v1/champion-tiles/{champion_id}/{id}.jpg"
                ),
            }
        })
        .collect()
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
            assert!(
                keys.insert(primary_key),
                "duplicate primary key for {}",
                champ.name
            );
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
        assert_eq!(
            cat.resolve_champion_name_localized(11, Lang::En),
            "Master Yi"
        );
        assert_eq!(
            cat.resolve_champion_name_localized(897, Lang::En),
            "K'Sante"
        );
        assert_eq!(
            cat.resolve_champion_name_localized(0, Lang::En),
            "Selecting..."
        );
        assert_eq!(
            cat.resolve_champion_name_localized(0, Lang::Fr),
            "Sélection..."
        );
        assert_eq!(cat.resolve_champion_id("Sélection..."), 0);
        assert_eq!(cat.resolve_champion_id("Selecting..."), 0);
        assert_eq!(
            extract_raw_champion_alias("game_character_displayname_MasterYi"),
            "MasterYi"
        );
        assert_eq!(
            cat.resolve_champion_id_with_alias(
                "UnlocalizedName",
                &extract_raw_champion_alias("game_character_displayname_MasterYi")
            ),
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

    #[test]
    fn localizes_spells_runes_and_skins_for_en_and_fr() {
        assert_eq!(spell_name_from_id_localized(4, Lang::En), "Flash");
        assert_eq!(spell_name_from_id_localized(4, Lang::Fr), "Saut éclair");
        assert_eq!(spell_name_from_id_localized(14, Lang::Fr), "Embrasement");

        assert_eq!(rune_style_name_localized(8000, Lang::En), "Precision");
        assert_eq!(rune_style_name_localized(8000, Lang::Fr), "Précision");
        assert_eq!(
            keystone_name_from_id_localized(8112, Lang::En),
            "Electrocute"
        );
        assert_eq!(
            keystone_name_from_id_localized(8112, Lang::Fr),
            "Électrocution"
        );

        let en_skins = builtin_skins_for_localized(103, "Ahri", 103000, Lang::En);
        let fr_skins = builtin_skins_for_localized(103, "Ahri", 103000, Lang::Fr);
        assert_eq!(en_skins[0].name, "Classic Ahri");
        assert_eq!(fr_skins[0].name, "Ahri classique");

        let en_fallback = builtin_skins_for_localized(99, "Lux", 99000, Lang::En);
        let fr_fallback = builtin_skins_for_localized(99, "Lux", 99000, Lang::Fr);
        assert_eq!(en_fallback[0].name, "Classic Lux");
        assert_eq!(fr_fallback[0].name, "Lux classique");
    }
}
