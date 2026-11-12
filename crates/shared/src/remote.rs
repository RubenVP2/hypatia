use serde::{Deserialize, Serialize};

use crate::summary::ChampSelectEnemy;

/// Strict, closed allowlist of user-initiated commands accepted by the Mobile Remote.
///
/// INVARIANT: No arbitrary URI, HTTP method, or raw payload can be passed from the mobile
/// client. Every variant maps to one specific LCU endpoint with validated parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum RemoteCommand {
    /// Choose primary and secondary role in Lobby (`TOP`, `JUNGLE`, `MIDDLE`, `BOTTOM`, `UTILITY`, `FILL`).
    SetLobbyPositions {
        first: String,
        second: String,
    },
    /// Start matchmaking queue from Lobby.
    StartMatchmaking,
    /// Cancel matchmaking queue.
    StopMatchmaking,
    /// Accept the Ready Check ("Match Found").
    AcceptReadyCheck,
    /// Decline the Ready Check.
    DeclineReadyCheck,
    /// Hover a champion during the player's ban or pick action.
    HoverChampion {
        action_id: i64,
        champion_id: i64,
    },
    /// Lock in the currently hovered ban or pick action.
    LockAction {
        action_id: i64,
    },
    /// Select both summoner spells (`spell1_id` = D, `spell2_id` = F).
    SetSummonerSpells {
        spell1_id: i64,
        spell2_id: i64,
    },
    /// Select a skin for the locked-in champion.
    SetSkin {
        skin_id: i64,
    },
    /// Activate an existing rune page by ID.
    SelectRunePage {
        page_id: i64,
    },
    /// Save and activate a rune page (recommended preset or custom 1-hand builder).
    SaveRunePage {
        name: String,
        primary_style_id: i64,
        sub_style_id: i64,
        selected_perk_ids: Vec<i64>,
    },
    /// Request a role / lane swap with an ally (`position-swaps/{swap_id}/request`).
    RequestPositionSwap {
        swap_id: i64,
    },
    /// Accept or decline an incoming role / lane swap (`position-swaps/{swap_id}/accept|decline`).
    RespondPositionSwap {
        swap_id: i64,
        accept: bool,
    },
    /// Cancel an outgoing role / lane swap request.
    CancelPositionSwap {
        swap_id: i64,
    },
    /// Request a pick-order swap with an ally (`swaps/{swap_id}/request`).
    RequestPickOrderSwap {
        swap_id: i64,
    },
    /// Accept or decline an incoming pick-order swap.
    RespondPickOrderSwap {
        swap_id: i64,
        accept: bool,
    },
    /// Cancel an outgoing pick-order swap.
    CancelPickOrderSwap {
        swap_id: i64,
    },
    /// Request a champion trade with an ally (`trades/{trade_id}/request`).
    RequestChampionTrade {
        trade_id: i64,
    },
    /// Accept or decline an incoming champion trade.
    RespondChampionTrade {
        trade_id: i64,
        accept: bool,
    },
    /// Cancel an outgoing champion trade.
    CancelChampionTrade {
        trade_id: i64,
    },
    /// In Demo Mode only: jump directly to a phase (`Lobby`, `Matchmaking`, `ReadyCheck`, `ChampSelect`, `InProgress`).
    DemoSetPhase {
        phase: String,
    },
    /// In Demo Mode only: simulate an incoming role/lane swap request from an ally.
    DemoSimulateIncomingSwap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RemoteStateSnapshot {
    pub phase: String,
    pub is_demo: bool,
    pub alert_seq: u64,
    pub alert_kind: Option<String>,
    pub alert_title: Option<String>,
    pub alert_body: Option<String>,
    pub summoner_name: String,
    pub lobby: Option<RemoteLobbyState>,
    pub matchmaking: Option<RemoteMatchmakingState>,
    pub champ_select: Option<RemoteChampSelectState>,
    #[serde(default)]
    pub live_game: Option<RemoteLiveState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteLiveFeedItem {
    pub time: String,
    pub text: String,
    pub side: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLiveObjective {
    pub label: String,
    pub mine: bool,
    pub weight: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLivePlayer {
    pub lane: String,
    pub champion: String,
    pub level: u32,
    pub kda: String,
    pub cs: u32,
    pub is_self: bool,
    pub dead: bool,
    pub respawn_sec: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteLiveState {
    pub game_mode: String,
    pub clock: String,
    pub my_kills: u32,
    pub enemy_kills: u32,
    pub my_towers: u32,
    pub enemy_towers: u32,
    pub active_champion: String,
    pub level: u32,
    pub current_gold: i64,
    pub kda: String,
    pub cs: u32,
    pub allies: Vec<RemoteLivePlayer>,
    pub enemies: Vec<RemoteLivePlayer>,
    pub objectives: Vec<RemoteLiveObjective>,
    pub feed: Vec<RemoteLiveFeedItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLobbyState {
    pub queue_id: i64,
    pub queue_name: String,
    pub first_position: String,
    pub second_position: String,
    pub is_leader: bool,
    pub member_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteMatchmakingState {
    pub queue_name: String,
    pub in_queue: bool,
    pub time_in_queue_sec: u32,
    pub estimated_queue_time_sec: u32,
    pub ready_check_active: bool,
    pub ready_check_timer_sec: i32,
    pub player_response: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteActiveAction {
    pub action_id: i64,
    /// `"ban"` or `"pick"`
    pub action_type: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub completed: bool,
    pub is_my_turn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTeamMember {
    pub cell_id: i64,
    pub assigned_position: String,
    pub champion_id: i64,
    pub champion_name: String,
    pub summoner_name: String,
    pub spell1_id: i64,
    pub spell2_id: i64,
    pub is_self: bool,
    pub is_acting: bool,
    pub acting_type: Option<String>,
    pub position_swap_id: Option<i64>,
    pub position_swap_state: Option<String>,
    pub pick_swap_id: Option<i64>,
    pub pick_swap_state: Option<String>,
    pub trade_id: Option<i64>,
    pub trade_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteIncomingSwap {
    pub id: i64,
    /// `"position"`, `"pick_order"`, or `"champion"`
    pub kind: String,
    pub caller_name: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteChampionOption {
    pub id: i64,
    pub name: String,
    pub roles: Vec<String>,
    pub pickable: bool,
    pub bannable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSkinOption {
    pub id: i64,
    pub name: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSpellOption {
    pub id: i64,
    pub name: String,
    pub key: String,
    pub cooldown: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRunePage {
    pub id: i64,
    pub name: String,
    pub primary_style_id: i64,
    pub primary_style_name: String,
    pub sub_style_id: i64,
    pub sub_style_name: String,
    pub keystone_name: String,
    pub selected_perk_ids: Vec<i64>,
    pub current: bool,
    pub is_preset: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePerkOption {
    pub id: i64,
    pub name: String,
    pub short_desc: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRuneTree {
    pub id: i64,
    pub name: String,
    pub tagline: String,
    pub keystones: Vec<RemotePerkOption>,
    pub slot1: Vec<RemotePerkOption>,
    pub slot2: Vec<RemotePerkOption>,
    pub slot3: Vec<RemotePerkOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteChampSelectState {
    pub sub_phase: String,
    pub time_left_sec: i64,
    pub queue_type: String,
    pub local_cell_id: i64,
    pub assigned_position: String,
    pub active_action: Option<RemoteActiveAction>,
    pub spell1_id: i64,
    pub spell2_id: i64,
    pub selected_skin_id: i64,
    pub my_team: Vec<RemoteTeamMember>,
    pub their_team: Vec<ChampSelectEnemy>,
    pub my_team_bans: Vec<String>,
    pub their_team_bans: Vec<String>,
    pub incoming_swaps: Vec<RemoteIncomingSwap>,
    pub champions: Vec<RemoteChampionOption>,
    pub available_skins: Vec<RemoteSkinOption>,
    pub available_spells: Vec<RemoteSpellOption>,
    pub rune_pages: Vec<RemoteRunePage>,
    pub recommended_rune_pages: Vec<RemoteRunePage>,
    pub rune_trees: Vec<RemoteRuneTree>,
}

/// Static catalog of Summoner's Rift summoner spells (IDs match Riot LCU).
pub fn default_summoner_spells() -> Vec<RemoteSpellOption> {
    vec![
        RemoteSpellOption {
            id: 4,
            name: "Flash".to_string(),
            key: "F".to_string(),
            cooldown: "300s".to_string(),
            description: "Blink a short distance toward your cursor.".to_string(),
        },
        RemoteSpellOption {
            id: 14,
            name: "Ignite".to_string(),
            key: "I".to_string(),
            cooldown: "180s".to_string(),
            description: "Ignites target enemy champion for true damage and Grievous Wounds.".to_string(),
        },
        RemoteSpellOption {
            id: 12,
            name: "Teleport".to_string(),
            key: "T".to_string(),
            cooldown: "360s".to_string(),
            description: "Channel to teleport to an allied structure, minion, or ward.".to_string(),
        },
        RemoteSpellOption {
            id: 11,
            name: "Smite".to_string(),
            key: "S".to_string(),
            cooldown: "15s".to_string(),
            description: "Deals true damage to a monster or minion. Required for Junglers.".to_string(),
        },
        RemoteSpellOption {
            id: 7,
            name: "Heal".to_string(),
            key: "H".to_string(),
            cooldown: "240s".to_string(),
            description: "Restores health and grants movement speed to you and a nearby ally.".to_string(),
        },
        RemoteSpellOption {
            id: 21,
            name: "Barrier".to_string(),
            key: "B".to_string(),
            cooldown: "180s".to_string(),
            description: "Shields your champion from damage for 2.5 seconds.".to_string(),
        },
        RemoteSpellOption {
            id: 3,
            name: "Exhaust".to_string(),
            key: "E".to_string(),
            cooldown: "210s".to_string(),
            description: "Slows target enemy champion and reduces their damage dealt by 35%.".to_string(),
        },
        RemoteSpellOption {
            id: 6,
            name: "Ghost".to_string(),
            key: "G".to_string(),
            cooldown: "240s".to_string(),
            description: "Gain movement speed and ignore unit collision for 15 seconds.".to_string(),
        },
        RemoteSpellOption {
            id: 1,
            name: "Cleanse".to_string(),
            key: "C".to_string(),
            cooldown: "210s".to_string(),
            description: "Removes all disables (excluding suppression and airborne) and summoner spell debuffs.".to_string(),
        },
    ]
}

pub fn spell_name_from_id(spell_id: i64) -> &'static str {
    match spell_id {
        1 => "Cleanse",
        3 => "Exhaust",
        4 => "Flash",
        6 => "Ghost",
        7 => "Heal",
        11 => "Smite",
        12 => "Teleport",
        13 => "Clarity",
        14 => "Ignite",
        21 => "Barrier",
        32 => "Mark",
        _ => "Spell",
    }
}

/// Complete Rune Trees (Precision, Domination, Sorcery, Resolve, Inspiration) with real Riot Perk IDs.
pub fn default_rune_trees() -> Vec<RemoteRuneTree> {
    vec![
        RemoteRuneTree {
            id: 8000,
            name: "Precision".to_string(),
            tagline: "Improved attacks and sustained damage".to_string(),
            keystones: vec![
                RemotePerkOption { id: 8005, name: "Press the Attack".into(), short_desc: "3 hits expose target".into() },
                RemotePerkOption { id: 8008, name: "Lethal Tempo".into(), short_desc: "Attack speed on hit".into() },
                RemotePerkOption { id: 8021, name: "Fleet Footwork".into(), short_desc: "Energized heal + speed".into() },
                RemotePerkOption { id: 8010, name: "Conqueror".into(), short_desc: "Stacking adaptive force + heal".into() },
            ],
            slot1: vec![
                RemotePerkOption { id: 9101, name: "Absorb Life".into(), short_desc: "Killing targets heals".into() },
                RemotePerkOption { id: 9111, name: "Triumph".into(), short_desc: "Takedowns restore 10% missing HP".into() },
                RemotePerkOption { id: 8009, name: "Presence of Mind".into(), short_desc: "Restore mana/energy on takedown".into() },
            ],
            slot2: vec![
                RemotePerkOption { id: 9104, name: "Legend: Alacrity".into(), short_desc: "Takedowns grant attack speed".into() },
                RemotePerkOption { id: 9105, name: "Legend: Haste".into(), short_desc: "Takedowns grant basic ability haste".into() },
                RemotePerkOption { id: 9103, name: "Legend: Bloodline".into(), short_desc: "Takedowns grant life steal".into() },
            ],
            slot3: vec![
                RemotePerkOption { id: 8014, name: "Coup de Grace".into(), short_desc: "Bonus damage to low HP enemies".into() },
                RemotePerkOption { id: 8017, name: "Cut Down".into(), short_desc: "Bonus damage to high HP enemies".into() },
                RemotePerkOption { id: 8299, name: "Last Stand".into(), short_desc: "Bonus damage while low HP".into() },
            ],
        },
        RemoteRuneTree {
            id: 8100,
            name: "Domination".to_string(),
            tagline: "Burst damage and target access".to_string(),
            keystones: vec![
                RemotePerkOption { id: 8112, name: "Electrocute".into(), short_desc: "3 separate hits deal burst damage".into() },
                RemotePerkOption { id: 8128, name: "Dark Harvest".into(), short_desc: "Damage low HP champs for souls".into() },
                RemotePerkOption { id: 9923, name: "Hail of Blades".into(), short_desc: "110% attack speed for 3 attacks".into() },
            ],
            slot1: vec![
                RemotePerkOption { id: 8126, name: "Cheap Shot".into(), short_desc: "True damage to impaired targets".into() },
                RemotePerkOption { id: 8139, name: "Taste of Blood".into(), short_desc: "Heal when damaging an enemy".into() },
                RemotePerkOption { id: 8143, name: "Sudden Impact".into(), short_desc: "Bonus damage after dash/stealth".into() },
            ],
            slot2: vec![
                RemotePerkOption { id: 8136, name: "Zombie Ward".into(), short_desc: "Takedowns on wards spawn allied wards".into() },
                RemotePerkOption { id: 8120, name: "Ghost Poro".into(), short_desc: "Expired wards leave a Ghost Poro".into() },
                RemotePerkOption { id: 8138, name: "Eyeball Collection".into(), short_desc: "Stacking adaptive force on takedowns".into() },
            ],
            slot3: vec![
                RemotePerkOption { id: 8135, name: "Treasure Hunter".into(), short_desc: "Bonus gold on unique takedowns".into() },
                RemotePerkOption { id: 8105, name: "Relentless Hunter".into(), short_desc: "Out-of-combat movement speed".into() },
                RemotePerkOption { id: 8106, name: "Ultimate Hunter".into(), short_desc: "Ultimate ability haste stacks".into() },
            ],
        },
        RemoteRuneTree {
            id: 8200,
            name: "Sorcery".to_string(),
            tagline: "Empowered abilities and resource manipulation".to_string(),
            keystones: vec![
                RemotePerkOption { id: 8214, name: "Summon Aery".into(), short_desc: "Damage enemies or shield allies".into() },
                RemotePerkOption { id: 8229, name: "Arcane Comet".into(), short_desc: "Ability hits hurl a comet".into() },
                RemotePerkOption { id: 8230, name: "Phase Rush".into(), short_desc: "3 hits grant burst of speed".into() },
            ],
            slot1: vec![
                RemotePerkOption { id: 8224, name: "Nullifying Orb".into(), short_desc: "Magic shield at low HP".into() },
                RemotePerkOption { id: 8226, name: "Manaflow Band".into(), short_desc: "+250 max mana and regen".into() },
                RemotePerkOption { id: 8275, name: "Nimbus Cloak".into(), short_desc: "Speed after Summoner Spell".into() },
            ],
            slot2: vec![
                RemotePerkOption { id: 8210, name: "Transcendence".into(), short_desc: "Ability haste + cooldown refund".into() },
                RemotePerkOption { id: 8234, name: "Celerity".into(), short_desc: "Movement speed bonuses amplified".into() },
                RemotePerkOption { id: 8233, name: "Absolute Focus".into(), short_desc: "Adaptive force above 70% HP".into() },
            ],
            slot3: vec![
                RemotePerkOption { id: 8237, name: "Scorch".into(), short_desc: "Abilities burn enemies in lane".into() },
                RemotePerkOption { id: 8232, name: "Waterwalking".into(), short_desc: "Speed and force in the river".into() },
                RemotePerkOption { id: 8236, name: "Gathering Storm".into(), short_desc: "Scaling adaptive force every 10m".into() },
            ],
        },
        RemoteRuneTree {
            id: 8400,
            name: "Resolve".to_string(),
            tagline: "Durability and crowd control".to_string(),
            keystones: vec![
                RemotePerkOption { id: 8437, name: "Grasp of the Undying".into(), short_desc: "Every 4s in combat heal + max HP".into() },
                RemotePerkOption { id: 8439, name: "Aftershock".into(), short_desc: "Immobilizing grants resists + burst".into() },
                RemotePerkOption { id: 8465, name: "Guardian".into(), short_desc: "Shield nearby allies on damage".into() },
            ],
            slot1: vec![
                RemotePerkOption { id: 8446, name: "Demolish".into(), short_desc: "Charge powerful attack vs towers".into() },
                RemotePerkOption { id: 8463, name: "Font of Life".into(), short_desc: "Impairing enemies heals allies".into() },
                RemotePerkOption { id: 8401, name: "Shield Bash".into(), short_desc: "Shields grant resists + bonus hit".into() },
            ],
            slot2: vec![
                RemotePerkOption { id: 8429, name: "Conditioning".into(), short_desc: "Bonus Armor and MR at 12m".into() },
                RemotePerkOption { id: 8444, name: "Second Wind".into(), short_desc: "Regenerate HP after taking poke".into() },
                RemotePerkOption { id: 8473, name: "Bone Plating".into(), short_desc: "Reduce damage from 3 combo hits".into() },
            ],
            slot3: vec![
                RemotePerkOption { id: 8451, name: "Overgrowth".into(), short_desc: "Permanent max HP from nearby minions".into() },
                RemotePerkOption { id: 8453, name: "Revitalize".into(), short_desc: "5% stronger heals and shields".into() },
                RemotePerkOption { id: 8242, name: "Unflinching".into(), short_desc: "Bonus Armor/MR when CC'd".into() },
            ],
        },
        RemoteRuneTree {
            id: 8300,
            name: "Inspiration".to_string(),
            tagline: "Creative tools and rule bending".to_string(),
            keystones: vec![
                RemotePerkOption { id: 8351, name: "Glacial Augment".into(), short_desc: "Immobilizing creates slow rays".into() },
                RemotePerkOption { id: 8360, name: "Unsealed Spellbook".into(), short_desc: "Swap Summoner Spells in game".into() },
                RemotePerkOption { id: 8369, name: "First Strike".into(), short_desc: "Bonus damage + gold when striking first".into() },
            ],
            slot1: vec![
                RemotePerkOption { id: 8306, name: "Hextech Flashtraption".into(), short_desc: "Channel Flash while on cooldown".into() },
                RemotePerkOption { id: 8304, name: "Magical Footwear".into(), short_desc: "Free boots with +10 move speed".into() },
                RemotePerkOption { id: 8321, name: "Cash Back".into(), short_desc: "6% gold back on Legendary items".into() },
            ],
            slot2: vec![
                RemotePerkOption { id: 8313, name: "Triple Tonic".into(), short_desc: "Elixirs at levels 3, 6, and 9".into() },
                RemotePerkOption { id: 8352, name: "Time Warp Tonic".into(), short_desc: "Immediate potion restoration".into() },
                RemotePerkOption { id: 8345, name: "Biscuit Delivery".into(), short_desc: "Receive 3 sustain biscuits".into() },
            ],
            slot3: vec![
                RemotePerkOption { id: 8347, name: "Cosmic Insight".into(), short_desc: "+18 Summoner Spell & Item Haste".into() },
                RemotePerkOption { id: 8410, name: "Approach Velocity".into(), short_desc: "Bonus speed toward impaired enemies".into() },
                RemotePerkOption { id: 8316, name: "Jack Of All Trades".into(), short_desc: "Haste and force per unique stat".into() },
            ],
        },
    ]
}

pub fn rune_style_name(style_id: i64) -> &'static str {
    match style_id {
        8000 => "Precision",
        8100 => "Domination",
        8200 => "Sorcery",
        8300 => "Inspiration",
        8400 => "Resolve",
        _ => "Runes",
    }
}

pub fn keystone_name_from_id(perk_id: i64) -> &'static str {
    match perk_id {
        8005 => "Press the Attack",
        8008 => "Lethal Tempo",
        8021 => "Fleet Footwork",
        8010 => "Conqueror",
        8112 => "Electrocute",
        8128 => "Dark Harvest",
        9923 => "Hail of Blades",
        8214 => "Summon Aery",
        8229 => "Arcane Comet",
        8230 => "Phase Rush",
        8437 => "Grasp of the Undying",
        8439 => "Aftershock",
        8465 => "Guardian",
        8351 => "Glacial Augment",
        8360 => "Unsealed Spellbook",
        8369 => "First Strike",
        _ => "Custom Keystone",
    }
}

/// Built-in recommended rune pages tailored to champion & lane so one tap always works offline.
pub fn recommended_pages_for(champion_id: i64, position: &str) -> Vec<RemoteRunePage> {
    let pos = position.to_ascii_uppercase();
    // Pick sensible primary page based on champion archetype / role
    let (primary_name, p_style, s_style, perks) = match (champion_id, pos.as_str()) {
        // Mages / Mid assassins (Ahri, Lux, Orianna, Syndra, Viktor, etc.)
        (103 | 238 | 7 | 99 | 61 | 134 | 112 | 518 | 142 | 4, _) => (
            "Burst & Roam (Electrocute)",
            8100,
            8200,
            vec![8112, 8139, 8138, 8106, 8226, 8210, 5008, 5008, 5001],
        ),
        // Fighters / Bruisers (Aatrox, Darius, Garen, Lee Sin, Viego, Yasuo, Yone, Riven, Camille, Jax)
        (266 | 122 | 86 | 64 | 234 | 157 | 777 | 92 | 164 | 24 | 233, _) => (
            "Sustained Skirmish (Conqueror)",
            8000,
            8400,
            vec![8010, 9111, 9105, 8299, 8444, 8451, 5005, 5008, 5001],
        ),
        // Marksmen (Jinx, Kai'Sa, Vayne, Ezreal, Caitlyn, Ashe, Jhin, Lucian)
        (222 | 145 | 67 | 81 | 51 | 22 | 202 | 236, _) | (_, "BOTTOM" | "BOT") => (
            "Marksman DPS (Lethal Tempo)",
            8000,
            8200,
            vec![8008, 9101, 9103, 8014, 8234, 8236, 5005, 5008, 5001],
        ),
        // Tank / Engage Supports & Top Tanks (Thresh, Leona, Nautilus, Blitzcrank, Ornn, Malphite, Shen)
        (412 | 89 | 111 | 53 | 516 | 54 | 98, _) | (_, "UTILITY" | "SUPPORT") => (
            "Engage & Frontline (Aftershock)",
            8400,
            8300,
            vec![8439, 8463, 8473, 8242, 8345, 8347, 5007, 5001, 5001],
        ),
        (_, "TOP") => (
            "Top Lane Duelist (Grasp)",
            8400,
            8000,
            vec![8437, 8446, 8444, 8451, 9111, 9104, 5005, 5008, 5001],
        ),
        (_, "JUNGLE") => (
            "Jungle Tempo (Conqueror)",
            8000,
            8300,
            vec![8010, 9111, 9104, 8014, 8304, 8347, 5005, 5008, 5001],
        ),
        _ => (
            "All-Round Mage (Arcane Comet)",
            8200,
            8300,
            vec![8229, 8226, 8210, 8237, 8345, 8347, 5008, 5008, 5001],
        ),
    };

    vec![
        RemoteRunePage {
            id: -1,
            name: primary_name.to_string(),
            primary_style_id: p_style,
            primary_style_name: rune_style_name(p_style).to_string(),
            sub_style_id: s_style,
            sub_style_name: rune_style_name(s_style).to_string(),
            keystone_name: keystone_name_from_id(perks[0]).to_string(),
            selected_perk_ids: perks,
            current: false,
            is_preset: true,
        },
        RemoteRunePage {
            id: -2,
            name: "Poke & Scaling (Arcane Comet)".to_string(),
            primary_style_id: 8200,
            primary_style_name: "Sorcery".to_string(),
            sub_style_id: 8300,
            sub_style_name: "Inspiration".to_string(),
            keystone_name: "Arcane Comet".to_string(),
            selected_perk_ids: vec![8229, 8226, 8210, 8236, 8345, 8347, 5008, 5008, 5001],
            current: false,
            is_preset: true,
        },
        RemoteRunePage {
            id: -3,
            name: "Safe Lane Sustain (Fleet Footwork)".to_string(),
            primary_style_id: 8000,
            primary_style_name: "Precision".to_string(),
            sub_style_id: 8400,
            sub_style_name: "Resolve".to_string(),
            keystone_name: "Fleet Footwork".to_string(),
            selected_perk_ids: vec![8021, 9101, 9104, 8014, 8444, 8451, 5005, 5008, 5001],
            current: false,
            is_preset: true,
        },
    ]
}

/// Built-in champion catalog with Riot IDs, names, and primary lanes so the mobile
/// picker and desktop roster always have rich champion names and lane filters.
pub fn builtin_champion_catalog() -> Vec<RemoteChampionOption> {
    let entries: &[(i64, &str, &[&str])] = &[
        (266, "Aatrox", &["TOP"]),
        (103, "Ahri", &["MIDDLE"]),
        (84, "Akali", &["MIDDLE", "TOP"]),
        (12, "Alistar", &["UTILITY"]),
        (32, "Amumu", &["JUNGLE", "UTILITY"]),
        (22, "Ashe", &["BOTTOM", "UTILITY"]),
        (268, "Azir", &["MIDDLE"]),
        (432, "Bard", &["UTILITY"]),
        (53, "Blitzcrank", &["UTILITY"]),
        (63, "Brand", &["UTILITY", "MIDDLE", "JUNGLE"]),
        (201, "Braum", &["UTILITY"]),
        (233, "Briar", &["JUNGLE"]),
        (51, "Caitlyn", &["BOTTOM"]),
        (164, "Camille", &["TOP"]),
        (122, "Darius", &["TOP"]),
        (131, "Diana", &["JUNGLE", "MIDDLE"]),
        (119, "Draven", &["BOTTOM"]),
        (245, "Ekko", &["JUNGLE", "MIDDLE"]),
        (81, "Ezreal", &["BOTTOM"]),
        (114, "Fiora", &["TOP"]),
        (3, "Galio", &["MIDDLE", "UTILITY"]),
        (86, "Garen", &["TOP"]),
        (79, "Gragas", &["TOP", "JUNGLE", "MIDDLE"]),
        (104, "Graves", &["JUNGLE"]),
        (887, "Gwen", &["TOP"]),
        (910, "Hwei", &["MIDDLE", "UTILITY"]),
        (39, "Irelia", &["TOP", "MIDDLE"]),
        (40, "Janna", &["UTILITY"]),
        (59, "Jarvan IV", &["JUNGLE"]),
        (24, "Jax", &["TOP", "JUNGLE"]),
        (126, "Jayce", &["TOP", "MIDDLE"]),
        (202, "Jhin", &["BOTTOM"]),
        (222, "Jinx", &["BOTTOM"]),
        (145, "Kai'Sa", &["BOTTOM"]),
        (43, "Karma", &["UTILITY", "MIDDLE"]),
        (141, "Kayn", &["JUNGLE"]),
        (121, "Kha'Zix", &["JUNGLE"]),
        (7, "LeBlanc", &["MIDDLE"]),
        (64, "Lee Sin", &["JUNGLE"]),
        (89, "Leona", &["UTILITY"]),
        (876, "Lillia", &["JUNGLE"]),
        (236, "Lucian", &["BOTTOM"]),
        (117, "Lulu", &["UTILITY"]),
        (99, "Lux", &["UTILITY", "MIDDLE"]),
        (54, "Malphite", &["TOP"]),
        (21, "Miss Fortune", &["BOTTOM"]),
        (82, "Mordekaiser", &["TOP"]),
        (25, "Morgana", &["UTILITY"]),
        (267, "Nami", &["UTILITY"]),
        (111, "Nautilus", &["UTILITY"]),
        (518, "Neeko", &["MIDDLE", "UTILITY"]),
        (61, "Orianna", &["MIDDLE"]),
        (516, "Ornn", &["TOP"]),
        (80, "Pantheon", &["TOP", "MIDDLE", "UTILITY"]),
        (555, "Pyke", &["UTILITY"]),
        (497, "Rakan", &["UTILITY"]),
        (58, "Renekton", &["TOP"]),
        (92, "Riven", &["TOP"]),
        (360, "Samira", &["BOTTOM"]),
        (113, "Sejuani", &["JUNGLE"]),
        (235, "Senna", &["UTILITY", "BOTTOM"]),
        (147, "Seraphine", &["UTILITY", "BOTTOM"]),
        (875, "Sett", &["TOP"]),
        (35, "Shaco", &["JUNGLE", "UTILITY"]),
        (98, "Shen", &["TOP"]),
        (15, "Sivir", &["BOTTOM"]),
        (901, "Smolder", &["BOTTOM", "MIDDLE"]),
        (16, "Soraka", &["UTILITY"]),
        (517, "Sylas", &["MIDDLE"]),
        (134, "Syndra", &["MIDDLE"]),
        (412, "Thresh", &["UTILITY"]),
        (18, "Tristana", &["BOTTOM", "MIDDLE"]),
        (4, "Twisted Fate", &["MIDDLE"]),
        (67, "Vayne", &["BOTTOM", "TOP"]),
        (254, "Vi", &["JUNGLE"]),
        (234, "Viego", &["JUNGLE"]),
        (112, "Viktor", &["MIDDLE"]),
        (106, "Volibear", &["TOP", "JUNGLE"]),
        (498, "Xayah", &["BOTTOM"]),
        (157, "Yasuo", &["MIDDLE", "TOP", "BOTTOM"]),
        (777, "Yone", &["MIDDLE", "TOP"]),
        (350, "Yuumi", &["UTILITY"]),
        (154, "Zac", &["JUNGLE"]),
        (238, "Zed", &["MIDDLE", "JUNGLE"]),
        (221, "Zeri", &["BOTTOM"]),
        (142, "Zoe", &["MIDDLE"]),
    ];

    entries
        .iter()
        .map(|(id, name, roles)| RemoteChampionOption {
            id: *id,
            name: (*name).to_string(),
            roles: roles.iter().map(|r| (*r).to_string()).collect(),
            pickable: true,
            bannable: true,
        })
        .collect()
}

/// Generates realistic skins for any champion so skin picking works out of the box in Demo mode
/// and falls back cleanly when LCU carousel skins are not yet loaded.
pub fn builtin_skins_for(champion_id: i64, champion_name: &str, selected_skin_id: i64) -> Vec<RemoteSkinOption> {
    if champion_id <= 0 {
        return Vec::new();
    }
    let base = champion_id * 1000;
    let current = if selected_skin_id > 0 { selected_skin_id } else { base };
    let themes = match champion_id {
        103 => vec![
            (base, "Classic Ahri"),
            (base + 1, "Dynasty Ahri"),
            (base + 4, "Popstar Ahri"),
            (base + 14, "Star Guardian Ahri"),
            (base + 27, "Spirit Blossom Ahri"),
            (base + 42, "Coven Ahri"),
            (base + 85, "Immortalized Legend Ahri"),
        ],
        64 => vec![
            (base, "Classic Lee Sin"),
            (base + 4, "Muay Thai Lee Sin"),
            (base + 11, "God Fist Lee Sin"),
            (base + 27, "Storm Dragon Lee Sin"),
            (base + 51, "Heavenscale Lee Sin"),
        ],
        222 => vec![
            (base, "Classic Jinx"),
            (base + 4, "Star Guardian Jinx"),
            (base + 13, "PROJECT: Jinx"),
            (base + 29, "Arcane Fractured Jinx"),
            (base + 37, "Battle Cat Jinx"),
        ],
        266 => vec![
            (base, "Classic Aatrox"),
            (base + 7, "Blood Moon Aatrox"),
            (base + 9, "Odyssey Aatrox"),
            (base + 21, "DRX Aatrox"),
            (base + 30, "Primordian Aatrox"),
        ],
        412 => vec![
            (base, "Classic Thresh"),
            (base + 5, "Dark Star Thresh"),
            (base + 13, "High Noon Thresh"),
            (base + 17, "Spirit Blossom Thresh"),
        ],
        _ => vec![
            (base, "Classic"),
            (base + 1, "Blood Moon"),
            (base + 2, "Star Guardian"),
            (base + 3, "Spirit Blossom"),
            (base + 4, "High Noon"),
        ],
    };

    themes
        .into_iter()
        .map(|(id, label)| {
            let name = if label.contains(champion_name) || label == "Classic" {
                if label == "Classic" {
                    format!("Classic {champion_name}")
                } else {
                    label.to_string()
                }
            } else {
                format!("{label} {champion_name}")
            };
            RemoteSkinOption {
                id,
                name,
                selected: id == current,
            }
        })
        .collect()
}
