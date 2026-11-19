//! Game Text: the names of champions, items, runes, rune trees, summoner spells and skins by
//! Riot ID, in each language Hypatia speaks.
//!
//! The League client hands out every name in its own language, which players often pick for
//! the voices or the font. Hypatia names game things by ID from these tables instead, so a
//! Korean client still shows a French or English Hypatia. The client's own name only fills an
//! ID the tables do not know yet (content added after the tables were generated).
//!
//! The tables ship in the binary (`data/game_text/<lang>.tsv`, one `kind\tid\tname` row each)
//! and are regenerated each patch with `cargo run -p hypatia-core --example update_game_text`.
//! A new language is a new table plus its [`Lang`] arm in `source`.

use std::sync::OnceLock;

use crate::Lang;

/// What a Game Text name belongs to; IDs are only unique within one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameTextKind {
    Champion,
    Item,
    /// A rune, keystone or stat shard.
    Perk,
    /// A rune tree such as Precision.
    PerkStyle,
    /// A summoner spell such as Flash.
    Spell,
    Skin,
}

impl GameTextKind {
    const ALL: [Self; 6] = [
        Self::Champion,
        Self::Item,
        Self::Perk,
        Self::PerkStyle,
        Self::Spell,
        Self::Skin,
    ];

    /// The `kind` column of the tables.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Champion => "champion",
            Self::Item => "item",
            Self::Perk => "perk",
            Self::PerkStyle => "perkstyle",
            Self::Spell => "spell",
            Self::Skin => "skin",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// One language's table: per kind, `(id, name)` sorted by id.
struct Table {
    patch: &'static str,
    by_kind: [Vec<(i64, &'static str)>; GameTextKind::ALL.len()],
}

fn source(lang: Lang) -> &'static str {
    match lang {
        Lang::En => include_str!("../data/game_text/en.tsv"),
        Lang::Fr => include_str!("../data/game_text/fr.tsv"),
    }
}

fn table(lang: Lang) -> &'static Table {
    static TABLES: [OnceLock<Table>; 2] = [const { OnceLock::new() }; 2];
    TABLES[lang as usize].get_or_init(|| parse(source(lang)))
}

fn parse(text: &'static str) -> Table {
    let mut table = Table {
        patch: "",
        by_kind: Default::default(),
    };
    for line in text.lines() {
        if let Some(comment) = line.strip_prefix('#') {
            if let Some(rest) = comment.split_once("patch ") {
                table.patch = rest.1.split(',').next().unwrap_or("").trim();
            }
            continue;
        }
        let mut cols = line.splitn(3, '\t');
        let (Some(kind), Some(id), Some(name)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let (Some(kind), Ok(id)) = (GameTextKind::from_key(kind), id.parse::<i64>()) else {
            continue;
        };
        if !name.is_empty() {
            table.by_kind[kind as usize].push((id, name));
        }
    }
    for rows in &mut table.by_kind {
        rows.sort_unstable_by_key(|(id, _)| *id);
    }
    table
}

/// The name of `id` in `lang`, when the tables know it.
pub fn game_name(kind: GameTextKind, id: i64, lang: Lang) -> Option<&'static str> {
    let rows = &table(lang).by_kind[kind as usize];
    rows.binary_search_by_key(&id, |(id, _)| *id)
        .ok()
        .map(|i| rows[i].1)
}

/// The name of `id` in `lang`, else `client_name` (the client's own, in whatever language it
/// speaks) when the tables do not know `id` yet.
pub fn game_name_or(kind: GameTextKind, id: i64, lang: Lang, client_name: &str) -> String {
    game_name(kind, id, lang)
        .map(str::to_string)
        .unwrap_or_else(|| client_name.to_string())
}

/// Every `(id, name)` of one kind in `lang`, sorted by id.
pub fn game_names(kind: GameTextKind, lang: Lang) -> &'static [(i64, &'static str)] {
    &table(lang).by_kind[kind as usize]
}

/// The patch the `lang` table was generated from, such as `16.23.8399929`.
pub fn game_text_patch(lang: Lang) -> &'static str {
    table(lang).patch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_come_by_id_in_each_language() {
        assert_eq!(
            game_name(GameTextKind::Champion, 20, Lang::Fr),
            Some("Nunu et Willump")
        );
        assert_eq!(
            game_name(GameTextKind::Champion, 20, Lang::En),
            Some("Nunu & Willump")
        );
        assert_eq!(
            game_name(GameTextKind::Spell, 4, Lang::Fr),
            Some("Saut éclair")
        );
        assert_eq!(
            game_name(GameTextKind::PerkStyle, 8000, Lang::Fr),
            Some("Précision")
        );
        assert_eq!(
            game_name(GameTextKind::Item, 3089, Lang::En),
            Some("Rabadon's Deathcap")
        );
        assert_eq!(
            game_name(GameTextKind::Skin, 103000, Lang::En),
            Some("Ahri")
        );
        assert_eq!(game_name(GameTextKind::Champion, 0, Lang::En), None);
    }

    #[test]
    fn an_id_the_tables_lack_keeps_the_client_name() {
        assert_eq!(
            game_name_or(GameTextKind::Item, 999_999_999, Lang::Fr, "새 아이템"),
            "새 아이템"
        );
        assert_eq!(
            game_name_or(GameTextKind::Spell, 4, Lang::Fr, "점멸"),
            "Saut éclair"
        );
    }

    #[test]
    fn every_language_names_the_same_things() {
        for kind in GameTextKind::ALL {
            let en: Vec<i64> = game_names(kind, Lang::En).iter().map(|r| r.0).collect();
            let fr: Vec<i64> = game_names(kind, Lang::Fr).iter().map(|r| r.0).collect();
            assert!(!en.is_empty(), "{kind:?} table is empty");
            assert_eq!(en, fr, "{kind:?} ids differ between en and fr");
        }
        assert!(!game_text_patch(Lang::En).is_empty());
    }
}
