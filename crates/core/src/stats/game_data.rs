//! Static game data read from the local client: items, runes, rune styles and summoner spells
//! (`/lol-game-data/assets/v1/*.json`), plus the name → id lookups the online provider needs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hypatia_shared::{
    game_name, game_names, normalize_champion_key, GameData, GameDataEntry, GameTextKind, Lang,
};
use serde_json::Value;

use super::Fail;
use crate::assets::GameAssetCache;
use crate::riot_http::ReadOnlyRiotClient;

pub const ITEMS_PATH: &str = "/lol-game-data/assets/v1/items.json";
pub const PERKS_PATH: &str = "/lol-game-data/assets/v1/perks.json";
pub const PERK_STYLES_PATH: &str = "/lol-game-data/assets/v1/perkstyles.json";
pub const SPELLS_PATH: &str = "/lol-game-data/assets/v1/summoner-spells.json";
/// The client's display locale (`{"locale": "fr_FR", ...}`); only `locale` is read.
pub const LOCALE_PATH: &str = "/riotclient/region-locale";

/// Name → id lookups over one [`GameData`]: the client's names plus every Hypatia language's
/// Game Text names, so a name in any of them resolves.
#[derive(Debug, Default)]
pub struct GameDataIndex {
    perk_style: HashMap<i64, i64>,
    boots: HashSet<i64>,
    items: HashMap<String, i64>,
    perks: HashMap<String, i64>,
    spells: HashMap<String, i64>,
}

impl GameDataIndex {
    pub fn item_id(&self, name: &str) -> Option<i64> {
        self.items.get(&normalize_champion_key(name)).copied()
    }

    pub fn perk_id(&self, name: &str) -> Option<i64> {
        self.perks.get(&normalize_champion_key(name)).copied()
    }

    pub fn spell_id(&self, name: &str) -> Option<i64> {
        self.spells.get(&normalize_champion_key(name)).copied()
    }

    /// The rune style (tree) a perk belongs to; shards belong to none.
    pub fn style_of(&self, perk_id: i64) -> Option<i64> {
        self.perk_style.get(&perk_id).copied()
    }

    pub fn is_boots(&self, item_id: i64) -> bool {
        self.boots.contains(&item_id)
    }
}

/// Game data for one client session, with its lookups and the client locale.
#[derive(Debug, Default)]
pub struct LoadedGameData {
    pub data: Arc<GameData>,
    pub index: Arc<GameDataIndex>,
    /// Client locale such as `fr_FR`, when the client told us.
    pub locale: Option<String>,
    /// `data` as each Hypatia language shows it, indexed by `Lang as usize`: Game Text names
    /// by id, whatever language the client speaks; the client's name only for an id newer than
    /// the tables.
    pub by_lang: [Arc<GameData>; 2],
}

impl LoadedGameData {
    fn new(data: GameData, index: GameDataIndex, locale: Option<String>) -> Self {
        let data = Arc::new(data);
        let by_lang = [Lang::En, Lang::Fr].map(|lang| Arc::new(with_game_text(&data, lang)));
        Self {
            data,
            index: Arc::new(index),
            locale,
            by_lang,
        }
    }

    /// The game data as `lang` shows it.
    pub fn for_lang(&self, lang: Lang) -> Arc<GameData> {
        self.by_lang[lang as usize].clone()
    }
}

/// `data` named in `lang` by Game Text wherever the tables know the id.
fn with_game_text(data: &GameData, lang: Lang) -> GameData {
    let rename = |entries: &[GameDataEntry], kind: GameTextKind| {
        entries
            .iter()
            .map(|e| GameDataEntry {
                name: game_name(kind, e.id, lang)
                    .map(str::to_string)
                    .unwrap_or_else(|| e.name.clone()),
                ..e.clone()
            })
            .collect()
    };
    GameData {
        items: rename(&data.items, GameTextKind::Item),
        perks: rename(&data.perks, GameTextKind::Perk),
        perk_styles: rename(&data.perk_styles, GameTextKind::PerkStyle),
        spells: rename(&data.spells, GameTextKind::Spell),
        boots: data.boots.clone(),
    }
}

/// Parses the four client JSONs. Unknown shapes yield empty lists, never a panic.
pub fn parse_game_data(
    items: &Value,
    perks: &Value,
    styles: &Value,
    spells: &Value,
) -> (GameData, GameDataIndex) {
    let mut index = GameDataIndex::default();

    let items: Vec<GameDataEntry> = array(items)
        .iter()
        .filter_map(|item| {
            let entry = entry(item, "description")?;
            let is_boots = item
                .get("categories")
                .and_then(Value::as_array)
                .is_some_and(|c| c.iter().any(|v| v.as_str() == Some("Boots")));
            if is_boots {
                index.boots.insert(entry.id);
            }
            Some(entry)
        })
        .collect();

    let perks: Vec<GameDataEntry> = array(perks)
        .iter()
        .filter_map(|perk| entry(perk, "shortDesc").or_else(|| entry(perk, "tooltip")))
        .collect();

    let style_list = styles
        .get("styles")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_else(|| array(styles));
    let perk_styles: Vec<GameDataEntry> = style_list
        .iter()
        .filter_map(|style| {
            let entry = entry(style, "tooltip")?;
            for slot in style
                .get("slots")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if slot.get("type").and_then(Value::as_str) == Some("kStatMod") {
                    continue;
                }
                for perk in slot
                    .get("perks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(id) = perk.as_i64() {
                        index.perk_style.insert(id, entry.id);
                    }
                }
            }
            Some(entry)
        })
        .collect();

    let spells: Vec<GameDataEntry> = array(spells)
        .iter()
        .filter_map(|spell| entry(spell, "description"))
        .collect();

    index_names(&mut index.items, &items);
    index_names(&mut index.perks, &perks);
    index_names(&mut index.spells, &spells);
    for lang in [Lang::En, Lang::Fr] {
        index_game_text(&mut index.items, GameTextKind::Item, lang, &items);
        index_game_text(&mut index.perks, GameTextKind::Perk, lang, &perks);
        index_game_text(&mut index.spells, GameTextKind::Spell, lang, &spells);
    }

    let mut boots: Vec<i64> = index.boots.iter().copied().collect();
    boots.sort_unstable();
    (
        GameData {
            items,
            perks,
            perk_styles,
            spells,
            boots,
        },
        index,
    )
}

/// Reads the client's game data through the read-only client and registers item icons with the
/// asset cache. One call per client session (the service memoizes it per LCU port).
pub(crate) async fn load(
    client: &ReadOnlyRiotClient,
    port: u16,
    token: &str,
    assets: &GameAssetCache,
) -> Result<LoadedGameData, Fail> {
    let mut bodies = Vec::with_capacity(4);
    for (file_name, path) in [
        ("items.json", ITEMS_PATH),
        ("perks.json", PERKS_PATH),
        ("perkstyles.json", PERK_STYLES_PATH),
        ("summoner-spells.json", SPELLS_PATH),
    ] {
        let (status, _, bytes) = client
            .get_bytes(port, Some(token), path)
            .await
            .map_err(|_| Fail::ClientNoAnswer)?;
        if !status.is_success() {
            return Err(Fail::ClientHttp(status.as_u16()));
        }
        let json: Value = serde_json::from_slice(&bytes).map_err(|_| Fail::ClientBadFormat)?;
        if file_name == "items.json" {
            assets
                .ingest_and_persist_metadata(file_name, &json, &bytes)
                .await;
        }
        bodies.push(json);
    }
    let locale = match client.get(port, Some(token), LOCALE_PATH).await {
        Ok((status, _, body)) if status.is_success() => body
            .get("locale")
            .and_then(Value::as_str)
            .filter(|l| !l.is_empty())
            .map(str::to_string),
        _ => None,
    };
    let (data, index) = parse_game_data(&bodies[0], &bodies[1], &bodies[2], &bodies[3]);
    Ok(LoadedGameData::new(data, index, locale))
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn entry(value: &Value, description_key: &str) -> Option<GameDataEntry> {
    let id = value
        .get("id")
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return None;
    }
    let description = value
        .get(description_key)
        .and_then(Value::as_str)
        .map(strip_markup)
        .unwrap_or_default();
    let icon_path = value
        .get("iconPath")
        .and_then(Value::as_str)
        .and_then(crate::assets::sanitize_lcu_asset_path)
        .unwrap_or_default();
    Some(GameDataEntry {
        id,
        name: name.to_string(),
        description,
        icon_path,
    })
}

/// Game-mode copies of an item or rune share its name with a higher id: the lowest id wins.
fn index_names(map: &mut HashMap<String, i64>, entries: &[GameDataEntry]) {
    for e in entries {
        let key = normalize_champion_key(&e.name);
        if key.is_empty() {
            continue;
        }
        map.entry(key)
            .and_modify(|id| *id = (*id).min(e.id))
            .or_insert(e.id);
    }
}

/// Adds the Game Text names of the ids the client knows; a name the client already uses keeps
/// its id.
fn index_game_text(
    map: &mut HashMap<String, i64>,
    kind: GameTextKind,
    lang: Lang,
    entries: &[GameDataEntry],
) {
    let known: HashSet<i64> = entries.iter().map(|e| e.id).collect();
    for &(id, name) in game_names(kind, lang) {
        if !known.contains(&id) {
            continue;
        }
        let key = normalize_champion_key(name);
        if key.is_empty() {
            continue;
        }
        map.entry(key)
            .and_modify(|existing| {
                if !known.contains(existing) {
                    *existing = id;
                }
            })
            .or_insert(id);
    }
}

/// Turns client rich text (`<mainText><stats>…</stats><br>…`) into plain lines.
pub fn strip_markup(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut tag: Option<String> = None;
    for c in raw.chars() {
        match (&mut tag, c) {
            (None, '<') => tag = Some(String::new()),
            (Some(name), '>') => {
                let name = name
                    .trim()
                    .trim_end_matches('/')
                    .trim()
                    .to_ascii_lowercase();
                if name == "br" || name == "li" {
                    out.push('\n');
                }
                tag = None;
            }
            (Some(name), c) => name.push(c),
            (None, c) => out.push(c),
        }
    }
    out.replace("&nbsp;", " ")
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(name);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn parses_client_game_data_and_indexes_names() {
        let (data, index) = parse_game_data(
            &sample("lcu_items.json"),
            &sample("lcu_perks.json"),
            &sample("lcu_perkstyles.json"),
            &sample("lcu_summoner_spells.json"),
        );

        let deathcap = data.items.iter().find(|i| i.id == 3089).unwrap();
        assert_eq!(deathcap.name, "Rabadon's Deathcap");
        assert_eq!(
            deathcap.description,
            "120 Ability Power\nMagical Opus: Increases your total Ability Power by 30%."
        );
        assert!(deathcap
            .icon_path
            .starts_with("/lol-game-data/assets/ASSETS/Items/Icons2D/"));
        assert!(
            data.items.iter().all(|i| i.id != 0),
            "the empty slot item is skipped"
        );

        assert_eq!(index.item_id("rabadons deathcap"), Some(3089));
        assert_eq!(
            index.item_id("Rabadon's Deathcap"),
            Some(3089),
            "Arena copy 223089 loses"
        );
        assert!(index.is_boots(3020) && !index.is_boots(3089));

        assert_eq!(index.perk_id("Electrocute"), Some(8112));
        assert_eq!(index.style_of(8112), Some(8100));
        assert_eq!(index.style_of(8226), Some(8200));
        assert_eq!(index.style_of(5008), None, "shards belong to no style");
        assert_eq!(data.perk_styles.len(), 3);

        assert_eq!(index.spell_id("Flash"), Some(4));
        assert_eq!(
            data.spells.iter().find(|s| s.id == 14).unwrap().name,
            "Ignite"
        );
    }

    #[test]
    fn tolerates_unexpected_shapes() {
        let (data, index) = parse_game_data(
            &Value::Null,
            &serde_json::json!({}),
            &Value::Null,
            &serde_json::json!("x"),
        );
        assert_eq!(data, GameData::default());
        assert_eq!(index.item_id("anything"), None);
    }

    #[test]
    fn strips_client_markup() {
        assert_eq!(
            strip_markup("<mainText><stats>+10 <attention>AD</attention></stats><br><br>Passive&nbsp;text</mainText>"),
            "+10 AD\nPassive text"
        );
        assert_eq!(strip_markup("plain"), "plain");
    }
}
