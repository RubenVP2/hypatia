//! Command-line interface.

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "hypatia",
    version,
    about = "Local, read-only League of Legends companion"
)]
pub struct Cli {
    /// Start in Demo mode with simulated event streams
    #[arg(long)]
    pub demo: bool,

    /// Demo events generated per second
    #[arg(long, default_value_t = 2.0)]
    pub demo_rate: f32,

    /// Interface language override (en or fr)
    #[arg(long, value_name = "LANG")]
    pub lang: Option<String>,

    /// Open the Observatory (advanced view of what the Riot clients expose) on launch
    #[arg(long)]
    pub telemetry: bool,

    /// Enable Mobile Remote server and show QR pairing drawer on launch
    #[arg(long)]
    pub remote: bool,

    /// Disable automatic startup of the Mobile Remote server
    #[arg(long)]
    pub no_remote: bool,

    /// Preferred LAN port for the Mobile Remote server
    #[arg(long, default_value_t = 7879)]
    pub remote_port: u16,

    /// Start in Light (Day) theme instead of Dark (Night) theme
    #[arg(long)]
    pub light: bool,

    /// Turn every UI animation off (the OS reduced-motion setting is honoured either way)
    #[arg(long)]
    pub reduce_motion: bool,

    /// Pin a specific phase in Demo mode (lobby, queue, ready, champ-select, live, end)
    #[arg(long)]
    pub phase: Option<String>,

    /// Open the Stats page on launch at a tab (build, tiers, matchups, mine)
    #[arg(long, value_name = "TAB", num_args = 0..=1, default_missing_value = "build")]
    pub stats: Option<String>,
}

impl Cli {
    /// Canonical gameflow phase for `--phase`, if given.
    pub fn pinned_phase(&self) -> Option<&'static str> {
        let pinned = self.phase.as_deref()?;
        Some(match pinned.to_ascii_lowercase().as_str() {
            "lobby" => "Lobby",
            "queue" | "matchmaking" => "Matchmaking",
            "ready" | "readycheck" => "ReadyCheck",
            "live" | "inprogress" | "game" => "InProgress",
            "end" | "eog" | "endofgame" => "EndOfGame",
            _ => "ChampSelect",
        })
    }

    /// Stats page tab for `--stats`, if given (0 build, 1 tier list, 2 matchups, 3 my stats).
    pub fn stats_tab(&self) -> Option<i32> {
        let tab = self.stats.as_deref()?;
        Some(match tab.to_ascii_lowercase().as_str() {
            "tiers" | "tier" | "tier-list" => 1,
            "matchups" | "matchup" => 2,
            "mine" | "my-stats" | "me" => 3,
            _ => 0,
        })
    }
}
