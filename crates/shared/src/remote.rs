use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
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
    #[serde(default)]
    pub lang: Lang,
    pub alert_seq: u64,
    pub alert_kind: Option<String>,
    pub alert_title: Option<String>,
    pub alert_body: Option<String>,
    pub summoner_name: String,
    #[serde(default)]
    pub profile_icon_id: i64,
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
    #[serde(default)]
    pub champion_id: i64,
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
    #[serde(default)]
    pub active_champion_id: i64,
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
    #[serde(default)]
    pub tile_path: String,
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
    /// Full length of the current timer phase in seconds (0 when unknown).
    #[serde(default)]
    pub total_time_sec: i64,
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
    #[serde(default)]
    pub my_team_ban_ids: Vec<i64>,
    #[serde(default)]
    pub their_team_ban_ids: Vec<i64>,
    pub incoming_swaps: Vec<RemoteIncomingSwap>,
    pub champions: Vec<RemoteChampionOption>,
    pub available_skins: Vec<RemoteSkinOption>,
    pub available_spells: Vec<RemoteSpellOption>,
    pub rune_pages: Vec<RemoteRunePage>,
    pub recommended_rune_pages: Vec<RemoteRunePage>,
    pub rune_trees: Vec<RemoteRuneTree>,
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
        .map(|(&(id, en_n, fr_n, en_k, fr_k, cd, en_d), &fr_d)| RemoteSpellOption {
            id,
            name: lang.pick(en_n, fr_n).to_string(),
            key: lang.pick(en_k, fr_k).to_string(),
            cooldown: cd.to_string(),
            description: lang.pick(en_d, fr_d).to_string(),
        })
        .collect()
}

pub fn spell_name_from_id_localized(spell_id: i64, lang: Lang) -> &'static str {
    match spell_id {
        1 => lang.pick("Cleanse", "Purge"),
        3 => lang.pick("Exhaust", "Fatigue"),
        4 => lang.pick("Flash", "Saut éclair"),
        6 => lang.pick("Ghost", "Fantôme"),
        7 => lang.pick("Heal", "Soins"),
        11 => lang.pick("Smite", "Châtiment"),
        12 => lang.pick("Teleport", "Téléportation"),
        13 => lang.pick("Clarity", "Clarté"),
        14 => lang.pick("Ignite", "Embrasement"),
        21 => lang.pick("Barrier", "Barrière"),
        32 => lang.pick("Mark", "Marquage"),
        _ => lang.pick("Spell", "Sort"),
    }
}

fn perk_opt(lang: Lang, id: i64, en_n: &'static str, fr_n: &'static str, en_d: &'static str, fr_d: &'static str) -> RemotePerkOption {
    RemotePerkOption {
        id,
        name: lang.pick(en_n, fr_n).to_string(),
        short_desc: lang.pick(en_d, fr_d).to_string(),
    }
}

/// Localized Rune Trees (Precision, Domination, Sorcery, Resolve, Inspiration) with real Riot Perk IDs.
pub fn default_rune_trees_localized(lang: Lang) -> Vec<RemoteRuneTree> {
    vec![
        RemoteRuneTree {
            id: 8000,
            name: rune_style_name_localized(8000, lang).to_string(),
            tagline: lang.pick("Improved attacks and sustained damage", "Attaques améliorées et dégâts soutenus").to_string(),
            keystones: vec![
                perk_opt(lang, 8005, "Press the Attack", "Attaque soutenue", "3 hits expose target", "3 attaques exposent la cible"),
                perk_opt(lang, 8008, "Lethal Tempo", "Tempo mortel", "Attack speed on hit", "Vitesse d'attaque à l'impact"),
                perk_opt(lang, 8021, "Fleet Footwork", "Jeu de jambes", "Energized heal + speed", "Soin énergisé + vitesse"),
                perk_opt(lang, 8010, "Conqueror", "Conquérant", "Stacking adaptive force + heal", "Force adaptative cumulable + soin"),
            ],
            slot1: vec![
                perk_opt(lang, 9101, "Absorb Life", "Absorption vitale", "Killing targets heals", "Éliminer une cible vous soigne"),
                perk_opt(lang, 9111, "Triumph", "Triomphe", "Takedowns restore 10% missing HP", "Les éliminations rendent 10 % des PV manquants"),
                perk_opt(lang, 8009, "Presence of Mind", "Présence d'esprit", "Restore mana/energy on takedown", "Rend du mana/énergie sur élimination"),
            ],
            slot2: vec![
                perk_opt(lang, 9104, "Legend: Alacrity", "Légende : alacrité", "Takedowns grant attack speed", "Vitesse d'attaque par élimination"),
                perk_opt(lang, 9105, "Legend: Haste", "Légende : accélération", "Takedowns grant basic ability haste", "Accélération de compétence par élimination"),
                perk_opt(lang, 9103, "Legend: Bloodline", "Légende : sangsue", "Takedowns grant life steal", "Vol de vie par élimination"),
            ],
            slot3: vec![
                perk_opt(lang, 8014, "Coup de Grace", "Coup de grâce", "Bonus damage to low HP enemies", "Dégâts bonus aux ennemis affaiblis"),
                perk_opt(lang, 8017, "Cut Down", "Abattage", "Bonus damage to high HP enemies", "Dégâts bonus contre les cibles résistantes"),
                perk_opt(lang, 8299, "Last Stand", "Baroud d'honneur", "Bonus damage while low HP", "Dégâts bonus quand vos PV sont bas"),
            ],
        },
        RemoteRuneTree {
            id: 8100,
            name: rune_style_name_localized(8100, lang).to_string(),
            tagline: lang.pick("Burst damage and target access", "Dégâts explosifs et accès aux cibles").to_string(),
            keystones: vec![
                perk_opt(lang, 8112, "Electrocute", "Électrocution", "3 separate hits deal burst damage", "3 coups distincts infligent un burst"),
                perk_opt(lang, 8128, "Dark Harvest", "Moisson noire", "Damage low HP champs for souls", "Récolte des âmes sur cibles faibles"),
                perk_opt(lang, 9923, "Hail of Blades", "Déluge de lames", "110% attack speed for 3 attacks", "+110 % vitesse d'attaque sur 3 coups"),
            ],
            slot1: vec![
                perk_opt(lang, 8126, "Cheap Shot", "Coup bas", "True damage to impaired targets", "Dégâts bruts sur cibles entravées"),
                perk_opt(lang, 8139, "Taste of Blood", "Goût du sang", "Heal when damaging an enemy", "Soigne en blessant un ennemi"),
                perk_opt(lang, 8143, "Sudden Impact", "Ruée offensive", "Bonus damage after dash/stealth", "Dégâts bonus après ruée ou furtivité"),
            ],
            slot2: vec![
                perk_opt(lang, 8136, "Zombie Ward", "Balise zombie", "Takedowns on wards spawn allied wards", "Détruire une balise crée une balise alliée"),
                perk_opt(lang, 8120, "Ghost Poro", "Poro fantôme", "Expired wards leave a Ghost Poro", "Les balises expirées laissent un Poro fantôme"),
                perk_opt(lang, 8138, "Eyeball Collection", "Arracheur d'œil", "Stacking adaptive force on takedowns", "Force adaptative cumulée sur élimination"),
            ],
            slot3: vec![
                perk_opt(lang, 8135, "Treasure Hunter", "Chasseur de trésors", "Bonus gold on unique takedowns", "Or bonus sur éliminations uniques"),
                perk_opt(lang, 8105, "Relentless Hunter", "Chasseur acharné", "Out-of-combat movement speed", "Vitesse de déplacement hors combat"),
                perk_opt(lang, 8106, "Ultimate Hunter", "Chasseur ultime", "Ultimate ability haste stacks", "Accélération d'ultime cumulable"),
            ],
        },
        RemoteRuneTree {
            id: 8200,
            name: rune_style_name_localized(8200, lang).to_string(),
            tagline: lang.pick("Empowered abilities and resource manipulation", "Compétences renforcées et gestion des ressources").to_string(),
            keystones: vec![
                perk_opt(lang, 8214, "Summon Aery", "Invocation d'Aery", "Damage enemies or shield allies", "Blesse les ennemis ou protège les alliés"),
                perk_opt(lang, 8229, "Arcane Comet", "Comète arcanique", "Ability hits hurl a comet", "Vos sorts lancent une comète"),
                perk_opt(lang, 8230, "Phase Rush", "Rush phasique", "3 hits grant burst of speed", "3 coups donnent un boost de vitesse"),
            ],
            slot1: vec![
                perk_opt(lang, 8224, "Nullifying Orb", "Orbe anti-magie", "Magic shield at low HP", "Bouclier magique à PV faibles"),
                perk_opt(lang, 8226, "Manaflow Band", "Ruban de mana", "+250 max mana and regen", "+250 mana max et régénération"),
                perk_opt(lang, 8275, "Nimbus Cloak", "Manteau nuageux", "Speed after Summoner Spell", "Vitesse après sort d'invocateur"),
            ],
            slot2: vec![
                perk_opt(lang, 8210, "Transcendence", "Transcendance", "Ability haste + cooldown refund", "Accélération + réduction des délais"),
                perk_opt(lang, 8234, "Celerity", "Célérité", "Movement speed bonuses amplified", "Bonus de vitesse de déplacement amplifiés"),
                perk_opt(lang, 8233, "Absolute Focus", "Concentration absolue", "Adaptive force above 70% HP", "Force adaptative au-dessus de 70 % PV"),
            ],
            slot3: vec![
                perk_opt(lang, 8237, "Scorch", "Brûlure", "Abilities burn enemies in lane", "Les sorts brûlent les ennemis en voie"),
                perk_opt(lang, 8232, "Waterwalking", "Marche sur l'eau", "Speed and force in the river", "Vitesse et force dans la rivière"),
                perk_opt(lang, 8236, "Gathering Storm", "Tempête menaçante", "Scaling adaptive force every 10m", "Force adaptative croissante chaque 10 min"),
            ],
        },
        RemoteRuneTree {
            id: 8400,
            name: rune_style_name_localized(8400, lang).to_string(),
            tagline: lang.pick("Durability and crowd control", "Durabilité et contrôle de foule").to_string(),
            keystones: vec![
                perk_opt(lang, 8437, "Grasp of the Undying", "Poigne de l'immortel", "Every 4s in combat heal + max HP", "Soin + PV max toutes les 4s en combat"),
                perk_opt(lang, 8439, "Aftershock", "Après-coup", "Immobilizing grants resists + burst", "Immobiliser confère résistances + explosion"),
                perk_opt(lang, 8465, "Guardian", "Gardien", "Shield nearby allies on damage", "Bouclier sur allié proche subissant des dégâts"),
            ],
            slot1: vec![
                perk_opt(lang, 8446, "Demolish", "Démolition", "Charge powerful attack vs towers", "Charge une attaque puissante sur tourelles"),
                perk_opt(lang, 8463, "Font of Life", "Fontaine de vie", "Impairing enemies heals allies", "Entraver un ennemi soigne les alliés"),
                perk_opt(lang, 8401, "Shield Bash", "Coup de bouclier", "Shields grant resists + bonus hit", "Bouclier confère résistances + coup bonus"),
            ],
            slot2: vec![
                perk_opt(lang, 8429, "Conditioning", "Conditionnement", "Bonus Armor and MR at 12m", "Armure et RM bonus à 12 min"),
                perk_opt(lang, 8444, "Second Wind", "Second souffle", "Regenerate HP after taking poke", "Régénère des PV après avoir subi des dégâts"),
                perk_opt(lang, 8473, "Bone Plating", "Plaque d'os", "Reduce damage from 3 combo hits", "Réduit les dégâts de 3 coups rapides"),
            ],
            slot3: vec![
                perk_opt(lang, 8451, "Overgrowth", "Surcroissance", "Permanent max HP from nearby minions", "PV max permanents près des sbires"),
                perk_opt(lang, 8453, "Revitalize", "Revitalisation", "5% stronger heals and shields", "Soins et boucliers 5 % plus efficaces"),
                perk_opt(lang, 8242, "Unflinching", "Inébranlable", "Bonus Armor/MR when CC'd", "Armure/RM bonus sous contrôle de foule"),
            ],
        },
        RemoteRuneTree {
            id: 8300,
            name: rune_style_name_localized(8300, lang).to_string(),
            tagline: lang.pick("Creative tools and rule bending", "Outils créatifs et détournement des règles").to_string(),
            keystones: vec![
                perk_opt(lang, 8351, "Glacial Augment", "Optimisation glaciale", "Immobilizing creates slow rays", "Immobiliser crée des rayons ralentissants"),
                perk_opt(lang, 8360, "Unsealed Spellbook", "Grimoire déchaîné", "Swap Summoner Spells in game", "Changez de sort d'invocateur en partie"),
                perk_opt(lang, 8369, "First Strike", "Premier coup", "Bonus damage + gold when striking first", "Dégâts bonus + or en frappant le premier"),
            ],
            slot1: vec![
                perk_opt(lang, 8306, "Hextech Flashtraption", "Canaliportation Hextech", "Channel Flash while on cooldown", "Canalise un Saut éclair en récupération"),
                perk_opt(lang, 8304, "Magical Footwear", "Chaussures magiques", "Free boots with +10 move speed", "Bottes gratuites avec +10 vitesse"),
                perk_opt(lang, 8321, "Cash Back", "Remboursement", "6% gold back on Legendary items", "6 % d'or rendu par objet légendaire"),
            ],
            slot2: vec![
                perk_opt(lang, 8313, "Triple Tonic", "Triple tonique", "Elixirs at levels 3, 6, and 9", "Élixirs aux niveaux 3, 6 et 9"),
                perk_opt(lang, 8352, "Time Warp Tonic", "Philtre de chronodistorsion", "Immediate potion restoration", "Soin instantané via les potions"),
                perk_opt(lang, 8345, "Biscuit Delivery", "Livraison de biscuits", "Receive 3 sustain biscuits", "Reçoit 3 biscuits de régénération"),
            ],
            slot3: vec![
                perk_opt(lang, 8347, "Cosmic Insight", "Savoir cosmique", "+18 Summoner Spell & Item Haste", "+18 accélération sorts & objets"),
                perk_opt(lang, 8410, "Approach Velocity", "Vitesse d'approche", "Bonus speed toward impaired enemies", "Vitesse bonus vers ennemis entravés"),
                perk_opt(lang, 8316, "Jack Of All Trades", "Touche-à-tout", "Haste and force per unique stat", "Accélération et force par stat unique"),
            ],
        },
    ]
}

pub fn rune_style_name_localized(style_id: i64, lang: Lang) -> &'static str {
    match style_id {
        8000 => lang.pick("Precision", "Précision"),
        8100 => "Domination",
        8200 => lang.pick("Sorcery", "Sorcellerie"),
        8300 => "Inspiration",
        8400 => lang.pick("Resolve", "Volonté"),
        _ => "Runes",
    }
}

pub fn keystone_name_from_id_localized(perk_id: i64, lang: Lang) -> &'static str {
    match perk_id {
        8005 => lang.pick("Press the Attack", "Attaque soutenue"),
        8008 => lang.pick("Lethal Tempo", "Tempo mortel"),
        8021 => lang.pick("Fleet Footwork", "Jeu de jambes"),
        8010 => lang.pick("Conqueror", "Conquérant"),
        8112 => lang.pick("Electrocute", "Électrocution"),
        8128 => lang.pick("Dark Harvest", "Moisson noire"),
        9923 => lang.pick("Hail of Blades", "Déluge de lames"),
        8214 => lang.pick("Summon Aery", "Invocation d'Aery"),
        8229 => lang.pick("Arcane Comet", "Comète arcanique"),
        8230 => lang.pick("Phase Rush", "Rush phasique"),
        8437 => lang.pick("Grasp of the Undying", "Poigne de l'immortel"),
        8439 => lang.pick("Aftershock", "Après-coup"),
        8465 => lang.pick("Guardian", "Gardien"),
        8351 => lang.pick("Glacial Augment", "Optimisation glaciale"),
        8360 => lang.pick("Unsealed Spellbook", "Grimoire déchaîné"),
        8369 => lang.pick("First Strike", "Premier coup"),
        _ => lang.pick("Custom Keystone", "Rune fondamentale"),
    }
}

/// Localized built-in recommended rune pages tailored to champion & lane so one tap always works offline.
pub fn recommended_pages_for_localized(champion_id: i64, position: &str, lang: Lang) -> Vec<RemoteRunePage> {
    let pos = position.to_ascii_uppercase();
    // Pick sensible primary page based on champion archetype / role
    let (en_name, fr_name, p_style, s_style, perks) = match (champion_id, pos.as_str()) {
        // Mages / Mid assassins (Ahri, Lux, Orianna, Syndra, Viktor, etc.)
        (103 | 238 | 7 | 99 | 61 | 134 | 112 | 518 | 142 | 4, _) => (
            "Burst & Roam (Electrocute)",
            "Burst & Décalage (Électrocution)",
            8100,
            8200,
            vec![8112, 8139, 8138, 8106, 8226, 8210, 5008, 5008, 5001],
        ),
        // Fighters / Bruisers (Aatrox, Darius, Garen, Lee Sin, Viego, Yasuo, Yone, Riven, Camille, Jax)
        (266 | 122 | 86 | 64 | 234 | 157 | 777 | 92 | 164 | 24 | 233, _) => (
            "Sustained Skirmish (Conqueror)",
            "Escarmouche prolongée (Conquérant)",
            8000,
            8400,
            vec![8010, 9111, 9105, 8299, 8444, 8451, 5005, 5008, 5001],
        ),
        // Marksmen (Jinx, Kai'Sa, Vayne, Ezreal, Caitlyn, Ashe, Jhin, Lucian)
        (222 | 145 | 67 | 81 | 51 | 22 | 202 | 236, _) | (_, "BOTTOM" | "BOT") => (
            "Marksman DPS (Lethal Tempo)",
            "Tireur DPS (Tempo mortel)",
            8000,
            8200,
            vec![8008, 9101, 9103, 8014, 8234, 8236, 5005, 5008, 5001],
        ),
        // Tank / Engage Supports & Top Tanks (Thresh, Leona, Nautilus, Blitzcrank, Ornn, Malphite, Shen)
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
            name: lang.pick("Poke & Scaling (Arcane Comet)", "Poke & Scaling (Comète arcanique)").to_string(),
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
            name: lang.pick("Safe Lane Sustain (Fleet Footwork)", "Tenue de voie sûre (Jeu de jambes)").to_string(),
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
    let current = if selected_skin_id > 0 { selected_skin_id } else { base };
    let themes: Vec<(i64, &str, &str)> = match champion_id {
        103 => vec![
            (base, "Classic Ahri", "Ahri classique"),
            (base + 1, "Dynasty Ahri", "Ahri dynastique"),
            (base + 4, "Popstar Ahri", "Popstar Ahri"),
            (base + 14, "Star Guardian Ahri", "Ahri gardienne des étoiles"),
            (base + 27, "Spirit Blossom Ahri", "Ahri fleur spirituelle"),
            (base + 42, "Coven Ahri", "Ahri congrégation des sorcières"),
            (base + 85, "Immortalized Legend Ahri", "Ahri légende immortalisée"),
        ],
        64 => vec![
            (base, "Classic Lee Sin", "Lee Sin classique"),
            (base + 4, "Muay Thai Lee Sin", "Lee Sin muay thaï"),
            (base + 11, "God Fist Lee Sin", "Lee Sin poing divin"),
            (base + 27, "Storm Dragon Lee Sin", "Lee Sin dragon des tempêtes"),
            (base + 51, "Heavenscale Lee Sin", "Lee Sin écaille divine"),
        ],
        222 => vec![
            (base, "Classic Jinx", "Jinx classique"),
            (base + 4, "Star Guardian Jinx", "Jinx gardienne des étoiles"),
            (base + 13, "PROJECT: Jinx", "PROJET : Jinx"),
            (base + 29, "Arcane Fractured Jinx", "Jinx fracturée d'Arcane"),
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
            (base + 17, "Spirit Blossom Thresh", "Thresh fleur spirituelle"),
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
                tile_path: format!("/lol-game-data/assets/v1/champion-tiles/{champion_id}/{id}.jpg"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localizes_spells_runes_and_skins_for_en_and_fr() {
        assert_eq!(spell_name_from_id_localized(4, Lang::En), "Flash");
        assert_eq!(spell_name_from_id_localized(4, Lang::Fr), "Saut éclair");
        assert_eq!(spell_name_from_id_localized(14, Lang::Fr), "Embrasement");

        assert_eq!(rune_style_name_localized(8000, Lang::En), "Precision");
        assert_eq!(rune_style_name_localized(8000, Lang::Fr), "Précision");
        assert_eq!(keystone_name_from_id_localized(8112, Lang::En), "Electrocute");
        assert_eq!(keystone_name_from_id_localized(8112, Lang::Fr), "Électrocution");

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
