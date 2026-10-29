use gloo_timers::future::TimeoutFuture;
use hypatia_shared::{
    ChampSelectAlly, ChampSelectSummary, EndOfGameSummary, GamePhase, LiveGameEventSummary,
    LiveGameSummary, LobbyMemberSummary, LobbySummary, MatchmakingSummary, SourceId,
    SummonerSummary, SummarySnapshot,
};
use leptos::prelude::*;

use crate::components::{PhaseCard, PhaseTimeline};
use crate::net::fetch_summary;
use crate::state::{AppState, NavTab};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecapPreset {
    LiveApi,
    Lobby,
    Matchmaking,
    ChampSelectNormal,
    ChampSelectSolo,
    InProgress,
    EndOfGame,
}

fn synthetic_summoner(state: &str, msg: &str) -> SummonerSummary {
    SummonerSummary {
        display_name: "HypatiaObserver".to_string(),
        tag_line: "EUW".to_string(),
        summoner_level: 342,
        xp_percent: 65,
        puuid: "9a8b7c6d-5e4f-3a2b-1c0d-demo00000001".to_string(),
        rank_tier: Some("EMERALD".to_string()),
        rank_division: Some("II".to_string()),
        rank_lp: Some(54),
        rank_wins: Some(49),
        rank_losses: Some(43),
        presence_state: Some(state.to_string()),
        status_message: Some(msg.to_string()),
    }
}

fn preset_snapshot(preset: RecapPreset, base: Option<SummarySnapshot>) -> SummarySnapshot {
    let sources = base.as_ref().map(|b| b.sources.clone()).unwrap_or_default();
    let now_ms = js_sys::Date::now() as i64;

    match preset {
        RecapPreset::LiveApi => base.unwrap_or_default(),
        RecapPreset::Lobby => SummarySnapshot {
            phase: Some(GamePhase {
                phase: "Lobby".to_string(),
                since: now_ms - 75_000,
            }),
            sources,
            summoner: Some(synthetic_summoner("inLobby", "Ranked Flex 5v5 (2/5)")),
            lobby: Some(LobbySummary {
                party_id: "party-7878-euw".to_string(),
                queue_id: 440,
                queue_name: "Ranked Flex 5v5".to_string(),
                members: vec![
                    LobbyMemberSummary {
                        summoner_name: "HypatiaObserver#EUW".to_string(),
                        puuid: Some("9a8b7c6d-5e4f-3a2b-1c0d-demo00000001".to_string()),
                        is_leader: true,
                        ready: true,
                        first_position: Some("MIDDLE".to_string()),
                        second_position: Some("TOP".to_string()),
                    },
                    LobbyMemberSummary {
                        summoner_name: "KronoTrigger#EUW".to_string(),
                        puuid: Some("3c4411b2-7a88-4f10-9c11-demo00000002".to_string()),
                        is_leader: false,
                        ready: true,
                        first_position: Some("JUNGLE".to_string()),
                        second_position: Some("UTILITY".to_string()),
                    },
                ],
            }),
            ..Default::default()
        },
        RecapPreset::Matchmaking => SummarySnapshot {
            phase: Some(GamePhase {
                phase: "Matchmaking".to_string(),
                since: now_ms - 38_000,
            }),
            sources,
            summoner: Some(synthetic_summoner("inQueue", "Searching for match")),
            matchmaking: Some(MatchmakingSummary {
                queue_id: 420,
                time_in_queue_sec: 38.0,
                estimated_queue_time_sec: 70.0,
                is_currently_in_queue: true,
                ready_check_state: Some("Awaiting Match".to_string()),
                player_response: Some("None".to_string()),
            }),
            ..Default::default()
        },
        RecapPreset::ChampSelectNormal => SummarySnapshot {
            phase: Some(GamePhase {
                phase: "ChampSelect".to_string(),
                since: now_ms - 24_000,
            }),
            sources,
            summoner: Some(synthetic_summoner("champSelect", "Normal Draft / Flex")),
            champ_select: Some(ChampSelectSummary {
                sub_phase: "BAN_PICK".to_string(),
                time_left_sec: 24,
                queue_type: "NORMAL_DRAFT / FLEX_5x5".to_string(),
                is_ranked_solo_anonymized: false,
                my_team: vec![
                    ChampSelectAlly {
                        cell_id: 0,
                        assigned_position: "TOP".to_string(),
                        champion_name: "Aatrox".to_string(),
                        summoner_name: "BrokenBlade#EUW".to_string(),
                        puuid: "7f12a800-1122-4c00-9a01".to_string(),
                        rank_label: Some("DIAMOND IV • 61 LP (56% WR)".to_string()),
                        is_anonymized: false,
                        is_self: false,
                        is_premade: false,
                    },
                    ChampSelectAlly {
                        cell_id: 1,
                        assigned_position: "JUNGLE".to_string(),
                        champion_name: "Lee Sin".to_string(),
                        summoner_name: "KronoTrigger#EUW".to_string(),
                        puuid: "3c4411b2-7a88-4f10-11b2".to_string(),
                        rank_label: Some("DIAMOND IV • 12 LP (54% WR)".to_string()),
                        is_anonymized: false,
                        is_self: false,
                        is_premade: true,
                    },
                    ChampSelectAlly {
                        cell_id: 2,
                        assigned_position: "MIDDLE".to_string(),
                        champion_name: "Ahri".to_string(),
                        summoner_name: "HypatiaObserver#EUW".to_string(),
                        puuid: "9a8b7c6d-5e4f-3a2b-0001".to_string(),
                        rank_label: Some("EMERALD II • 54 LP (53% WR)".to_string()),
                        is_anonymized: false,
                        is_self: true,
                        is_premade: false,
                    },
                    ChampSelectAlly {
                        cell_id: 3,
                        assigned_position: "BOTTOM".to_string(),
                        champion_name: "Jinx".to_string(),
                        summoner_name: "HansSama#EUW".to_string(),
                        puuid: "1a99c401-8899-4d21-4e88".to_string(),
                        rank_label: Some("EMERALD I • 88 LP (58% WR)".to_string()),
                        is_anonymized: false,
                        is_self: false,
                        is_premade: false,
                    },
                    ChampSelectAlly {
                        cell_id: 4,
                        assigned_position: "UTILITY".to_string(),
                        champion_name: "Thresh".to_string(),
                        summoner_name: "Mikyx#EUW".to_string(),
                        puuid: "5d01e720-3344-4b18-7c33".to_string(),
                        rank_label: Some("EMERALD III • 20 LP (51% WR)".to_string()),
                        is_anonymized: false,
                        is_self: false,
                        is_premade: false,
                    },
                ],
                my_team_bans: vec![
                    "Zed".to_string(),
                    "Yasuo".to_string(),
                    "Shaco".to_string(),
                ],
                their_team_bans: vec![
                    "Yone".to_string(),
                    "Viego".to_string(),
                    "Briar".to_string(),
                ],
            }),
            ..Default::default()
        },
        RecapPreset::ChampSelectSolo => SummarySnapshot {
            phase: Some(GamePhase {
                phase: "ChampSelect".to_string(),
                since: now_ms - 19_000,
            }),
            sources,
            summoner: Some(synthetic_summoner("champSelect", "Ranked Solo/Duo")),
            champ_select: Some(ChampSelectSummary {
                sub_phase: "BAN_PICK".to_string(),
                time_left_sec: 19,
                queue_type: "RANKED_SOLO_5x5".to_string(),
                is_ranked_solo_anonymized: true,
                my_team: vec![
                    ChampSelectAlly {
                        cell_id: 0,
                        assigned_position: "TOP".to_string(),
                        champion_name: "Aatrox".to_string(),
                        summoner_name: "Ally 1 (Masked by Riot)".to_string(),
                        puuid: String::new(),
                        rank_label: None,
                        is_anonymized: true,
                        is_self: false,
                        is_premade: false,
                    },
                    ChampSelectAlly {
                        cell_id: 1,
                        assigned_position: "JUNGLE".to_string(),
                        champion_name: "Lee Sin".to_string(),
                        summoner_name: "KronoTrigger#EUW".to_string(),
                        puuid: "3c4411b2-7a88-4f10-11b2".to_string(),
                        rank_label: Some("DIAMOND IV • 12 LP (via /lol-lobby)".to_string()),
                        is_anonymized: false,
                        is_self: false,
                        is_premade: true,
                    },
                    ChampSelectAlly {
                        cell_id: 2,
                        assigned_position: "MIDDLE".to_string(),
                        champion_name: "Ahri".to_string(),
                        summoner_name: "HypatiaObserver#EUW".to_string(),
                        puuid: "9a8b7c6d-5e4f-3a2b-0001".to_string(),
                        rank_label: Some("EMERALD II • 54 LP (LocalPlayer)".to_string()),
                        is_anonymized: false,
                        is_self: true,
                        is_premade: false,
                    },
                    ChampSelectAlly {
                        cell_id: 3,
                        assigned_position: "BOTTOM".to_string(),
                        champion_name: "Jinx".to_string(),
                        summoner_name: "Ally 4 (Masked by Riot)".to_string(),
                        puuid: String::new(),
                        rank_label: None,
                        is_anonymized: true,
                        is_self: false,
                        is_premade: false,
                    },
                    ChampSelectAlly {
                        cell_id: 4,
                        assigned_position: "UTILITY".to_string(),
                        champion_name: "Thresh".to_string(),
                        summoner_name: "Ally 5 (Masked by Riot)".to_string(),
                        puuid: String::new(),
                        rank_label: None,
                        is_anonymized: true,
                        is_self: false,
                        is_premade: false,
                    },
                ],
                my_team_bans: vec![
                    "Zed".to_string(),
                    "Yasuo".to_string(),
                    "Shaco".to_string(),
                ],
                their_team_bans: vec![
                    "Yone".to_string(),
                    "Viego".to_string(),
                    "Briar".to_string(),
                ],
            }),
            ..Default::default()
        },
        RecapPreset::InProgress => SummarySnapshot {
            phase: Some(GamePhase {
                phase: "InProgress".to_string(),
                since: now_ms - 1_122_000,
            }),
            sources,
            summoner: Some(synthetic_summoner("inGame", "Playing Ahri (18m 42s)")),
            live_game: Some(LiveGameSummary {
                game_mode: "CLASSIC".to_string(),
                map_name: "Summoner's Rift".to_string(),
                game_time_sec: 1122.0,
                active_champion: "Ahri".to_string(),
                level: 13,
                current_gold: 1840,
                kills: 6,
                deaths: 1,
                assists: 8,
                creep_score: 154,
                recent_events: vec![
                    LiveGameEventSummary {
                        event_id: 42,
                        event_name: "ChampionKill".to_string(),
                        event_time_sec: 1090.0,
                        description: "HypatiaObserver → EnemyMid".to_string(),
                    },
                    LiveGameEventSummary {
                        event_id: 39,
                        event_name: "DragonKill".to_string(),
                        event_time_sec: 1005.0,
                        description: "Hextech Dragon claimed".to_string(),
                    },
                    LiveGameEventSummary {
                        event_id: 31,
                        event_name: "TurretKilled".to_string(),
                        event_time_sec: 860.0,
                        description: "Outer Mid Turret destroyed".to_string(),
                    },
                ],
            }),
            ..Default::default()
        },
        RecapPreset::EndOfGame => SummarySnapshot {
            phase: Some(GamePhase {
                phase: "EndOfGame".to_string(),
                since: now_ms - 15_000,
            }),
            sources,
            summoner: Some(synthetic_summoner("endOfGame", "Post-match lobby")),
            end_of_game: Some(EndOfGameSummary {
                result: "Victory".to_string(),
                game_length_sec: 1694,
                champion_name: "Ahri".to_string(),
                kills: 9,
                deaths: 2,
                assists: 14,
                total_gold: 14950,
                damage_to_champions: 34820,
            }),
            ..Default::default()
        },
    }
}

fn format_mm_ss(total_sec: f64) -> String {
    let sec = total_sec.max(0.0) as u64;
    format!("{:02}:{:02}", sec / 60, sec % 60)
}

#[component]
pub fn RecapView() -> impl IntoView {
    let state = expect_context::<AppState>();
    let live_summary = RwSignal::new(Option::<SummarySnapshot>::None);
    let preset = RwSignal::new(RecapPreset::LiveApi);
    let show_xray = RwSignal::new(true);
    let show_raw_json = RwSignal::new(false);

    // Poll /api/summary periodically while RecapView is mounted
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            if let Ok(snap) = fetch_summary().await {
                live_summary.set(Some(snap));
            }
            TimeoutFuture::new(1500).await;
        }
    });

    let active_snapshot = Signal::derive(move || {
        preset_snapshot(preset.get(), live_summary.get())
    });

    let jump_to_endpoint = {
        let state = state.clone();
        move |source: SourceId, uri: &'static str| {
            state.endpoint_store.filter_source.set(Some(source));
            state.endpoint_store.filter_query.set(uri.to_string());
            state.active_tab.set(NavTab::Endpoints);
        }
    };

    view! {
        <main class="recap-view-container" role="main" aria-label="Recap Summary View">
            <PhaseCard />
            <PhaseTimeline />

            // Control Strip: Stage Preset Switcher + Endpoint X-Ray Toggle
            <section class="recap-control-strip" aria-label="Recap display controls">
                <div class="recap-preset-group" role="group" aria-label="Stage view selector">
                    <button
                        type="button"
                        class=move || format!("recap-preset-btn {}", if preset.get() == RecapPreset::LiveApi { "active" } else { "" })
                        on:click=move |_| preset.set(RecapPreset::LiveApi)
                    >
                        <span class="recap-live-indicator"></span>
                        "LIVE (/api/summary)"
                    </button>
                    <button
                        type="button"
                        class=move || format!("recap-preset-btn {}", if preset.get() == RecapPreset::Lobby { "active" } else { "" })
                        on:click=move |_| preset.set(RecapPreset::Lobby)
                    >
                        "LOBBY"
                    </button>
                    <button
                        type="button"
                        class=move || format!("recap-preset-btn {}", if preset.get() == RecapPreset::Matchmaking { "active" } else { "" })
                        on:click=move |_| preset.set(RecapPreset::Matchmaking)
                    >
                        "IN QUEUE"
                    </button>
                    <button
                        type="button"
                        class=move || format!("recap-preset-btn {}", if preset.get() == RecapPreset::ChampSelectNormal { "active" } else { "" })
                        on:click=move |_| preset.set(RecapPreset::ChampSelectNormal)
                    >
                        "DRAFT (NORMAL/FLEX)"
                    </button>
                    <button
                        type="button"
                        class=move || format!("recap-preset-btn {}", if preset.get() == RecapPreset::ChampSelectSolo { "active" } else { "" })
                        on:click=move |_| preset.set(RecapPreset::ChampSelectSolo)
                    >
                        "DRAFT (SOLO/DUO)"
                    </button>
                    <button
                        type="button"
                        class=move || format!("recap-preset-btn {}", if preset.get() == RecapPreset::InProgress { "active" } else { "" })
                        on:click=move |_| preset.set(RecapPreset::InProgress)
                    >
                        "IN GAME (:2999)"
                    </button>
                    <button
                        type="button"
                        class=move || format!("recap-preset-btn {}", if preset.get() == RecapPreset::EndOfGame { "active" } else { "" })
                        on:click=move |_| preset.set(RecapPreset::EndOfGame)
                    >
                        "END OF GAME"
                    </button>
                </div>

                <div class="recap-action-group">
                    <button
                        type="button"
                        class=move || format!("recap-xray-btn {}", if show_xray.get() { "active" } else { "" })
                        on:click=move |_| show_xray.update(|v| *v = !*v)
                        title="Show or hide the Riot / LCU endpoints feeding each telemetry block"
                    >
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <circle cx="11" cy="11" r="8"></circle>
                            <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
                        </svg>
                        <span>{move || if show_xray.get() { "ENDPOINTS: VISIBLE" } else { "ENDPOINTS: HIDDEN" }}</span>
                    </button>

                    <button
                        type="button"
                        class=move || format!("recap-xray-btn {}", if show_raw_json.get() { "active" } else { "" })
                        on:click=move |_| show_raw_json.update(|v| *v = !*v)
                    >
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <polyline points="16 18 22 12 16 6"></polyline>
                            <polyline points="8 6 2 12 8 18"></polyline>
                        </svg>
                        <span>"JSON /api/summary"</span>
                    </button>
                </div>
            </section>

            // Optional collapsible /api/summary JSON payload viewer
            {move || {
                if show_raw_json.get() {
                    let json_str = serde_json::to_string_pretty(&active_snapshot.get())
                        .unwrap_or_else(|_| "{}".to_string());
                    view! {
                        <section class="recap-json-drawer" aria-label="Synthesized JSON payload">
                            <div class="recap-pane-header">
                                <h2 class="recap-pane-title">"GET /api/summary — SYNTHESIZED PAYLOAD"</h2>
                                <span class="recap-tag-lcu">"READ-ONLY AGGREGATION"</span>
                            </div>
                            <pre class="recap-json-pre tabular-nums">{json_str}</pre>
                        </section>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}

            // Main Two-Column Broadcast Telemetry Matrix
            <div class="recap-matrix">
                // Left Pane: Local Summoner Identity & Standing
                <section class="recap-pane recap-identity-pane" aria-labelledby="recap-summoner-heading">
                    <div class="recap-pane-header">
                        <h2 id="recap-summoner-heading" class="recap-pane-title">"LOCAL SUMMONER & STANDING"</h2>
                        {move || {
                            let snap = active_snapshot.get();
                            let state_lbl = snap
                                .summoner
                                .as_ref()
                                .and_then(|s| s.presence_state.clone())
                                .unwrap_or_else(|| "online".to_string());
                            view! {
                                <span class="recap-presence-pill">{state_lbl}</span>
                            }
                        }}
                    </div>

                    {move || {
                        let snap = active_snapshot.get();
                        let sum = snap.summoner.unwrap_or_else(|| synthetic_summoner("online", "Connected"));
                        let rank_text = match (&sum.rank_tier, &sum.rank_division, sum.rank_lp) {
                            (Some(t), Some(d), Some(lp)) => format!("{t} {d} — {lp} LP"),
                            (Some(t), _, _) => t.clone(),
                            _ => "UNRANKED".to_string(),
                        };
                        let record_text = match (sum.rank_wins, sum.rank_losses) {
                            (Some(w), Some(l)) if w + l > 0 => {
                                let wr = ((w as f64 / (w + l) as f64) * 100.0).round() as u32;
                                format!("{w}W {l}L ({wr}%)")
                            }
                            _ => "No ranked games recorded".to_string(),
                        };

                        view! {
                            <dl class="recap-dl tabular-nums">
                                <div class="recap-dl-row">
                                    <dt>"RIOT ID"</dt>
                                    <dd class="recap-val-primary">
                                        {sum.display_name}
                                        <span class="recap-tag-muted">{format!(" #{}", sum.tag_line)}</span>
                                    </dd>
                                </div>
                                <div class="recap-dl-row">
                                    <dt>"SUMMONER LEVEL"</dt>
                                    <dd>{format!("Level {} ({}% XP)", sum.summoner_level, sum.xp_percent)}</dd>
                                </div>
                                <div class="recap-dl-row">
                                    <dt>"SOLO/DUO STANDING"</dt>
                                    <dd class="recap-val-accent">{rank_text}</dd>
                                </div>
                                <div class="recap-dl-row">
                                    <dt>"WIN / LOSS RECORD"</dt>
                                    <dd>{record_text}</dd>
                                </div>
                                <div class="recap-dl-row">
                                    <dt>"STATUS MESSAGE"</dt>
                                    <dd>{sum.status_message.unwrap_or_else(|| "—".to_string())}</dd>
                                </div>
                                <div class="recap-dl-row">
                                    <dt>"ACCOUNT PUUID"</dt>
                                    <dd class="recap-val-mono">{sum.puuid}</dd>
                                </div>
                            </dl>
                        }
                    }}

                    {move || {
                        if show_xray.get() {
                            let jump1 = jump_to_endpoint.clone();
                            let jump2 = jump_to_endpoint.clone();
                            let jump3 = jump_to_endpoint.clone();
                            view! {
                                <footer class="recap-xray-footer">
                                    <button
                                        type="button"
                                        class="recap-endpoint-chip src-lcu"
                                        on:click=move |_| jump1(SourceId::LeagueClient, "/lol-summoner/v1/current-summoner")
                                    >
                                        <span class="chip-method">"GET"</span>
                                        <span>"/lol-summoner/v1/current-summoner"</span>
                                    </button>
                                    <button
                                        type="button"
                                        class="recap-endpoint-chip src-lcu"
                                        on:click=move |_| jump2(SourceId::LeagueClient, "/lol-ranked/v1/current-ranked-stats")
                                    >
                                        <span class="chip-method">"GET"</span>
                                        <span>"/lol-ranked/v1/current-ranked-stats"</span>
                                    </button>
                                    <button
                                        type="button"
                                        class="recap-endpoint-chip src-riot"
                                        on:click=move |_| jump3(SourceId::RiotClient, "/chat/v4/presences")
                                    >
                                        <span class="chip-method">"GET"</span>
                                        <span>"/chat/v4/presences"</span>
                                    </button>
                                </footer>
                            }.into_any()
                        } else {
                            ().into_any()
                        }
                    }}
                </section>

                // Right Pane: Stage-Specific Context Telemetry
                <section class="recap-pane recap-stage-pane" aria-labelledby="recap-stage-heading">
                    {move || {
                        let snap = active_snapshot.get();
                        let phase_str = snap
                            .phase
                            .as_ref()
                            .map(|p| p.phase.as_str())
                            .unwrap_or("Lobby");

                        match phase_str {
                            "ChampSelect" => {
                                let cs = snap.champ_select.unwrap_or_else(|| {
                                    preset_snapshot(RecapPreset::ChampSelectSolo, None)
                                        .champ_select
                                        .unwrap()
                                });
                                let is_anon = cs.is_ranked_solo_anonymized;
                                let my_bans = if cs.my_team_bans.is_empty() {
                                    "None".to_string()
                                } else {
                                    cs.my_team_bans.join(" · ")
                                };
                                let their_bans = if cs.their_team_bans.is_empty() {
                                    "None".to_string()
                                } else {
                                    cs.their_team_bans.join(" · ")
                                };

                                let jump_cs = jump_to_endpoint.clone();
                                let jump_sum = jump_to_endpoint.clone();
                                let jump_rk = jump_to_endpoint.clone();

                                view! {
                                    <div class="recap-pane-header">
                                        <h2 id="recap-stage-heading" class="recap-pane-title">
                                            "CHAMPION SELECT — ALLIED TEAM ROSTER"
                                        </h2>
                                        <div class="recap-header-badges">
                                            <span class=if is_anon { "recap-badge-warn" } else { "recap-badge-ok" }>
                                                {if is_anon {
                                                    "RANKED SOLO/DUO — RIOT ANONYMITY ACTIVE"
                                                } else {
                                                    "NORMAL / FLEX — ALLIED PROFILES VISIBLE"
                                                }}
                                            </span>
                                            <span class="recap-tag-lcu tabular-nums">
                                                {format!("{} • {}s", cs.sub_phase, cs.time_left_sec)}
                                            </span>
                                        </div>
                                    </div>

                                    <div class="recap-bans-bar">
                                        <div class="recap-ban-group">
                                            <span class="recap-ban-label">"ALLIED BANS:"</span>
                                            <span class="recap-ban-list">{my_bans}</span>
                                        </div>
                                        <div class="recap-ban-group">
                                            <span class="recap-ban-label">"ENEMY BANS:"</span>
                                            <span class="recap-ban-list">{their_bans}</span>
                                        </div>
                                    </div>

                                    <div class="recap-table-wrap">
                                        <table class="system-data-table recap-roster-table tabular-nums">
                                            <thead>
                                                <tr>
                                                    <th>"POS"</th>
                                                    <th>"CHAMPION"</th>
                                                    <th>"SUMMONER"</th>
                                                    <th>"EXPOSED PUUID"</th>
                                                    <th>"RESOLVED RANK & STANDING"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {cs.my_team.into_iter().map(|ally| {
                                                    let row_class = if ally.is_self {
                                                        "recap-row-self"
                                                    } else if ally.is_premade {
                                                        "recap-row-premade"
                                                    } else {
                                                        ""
                                                    };
                                                    let puuid_display = if ally.is_anonymized {
                                                        "\"\" (summonerId: 0)".to_string()
                                                    } else {
                                                        ally.puuid.clone()
                                                    };
                                                    let rank_display = ally.rank_label.unwrap_or_else(|| {
                                                        if ally.is_anonymized {
                                                            "Hidden by Riot server until loading screen".to_string()
                                                        } else {
                                                            "Query via /lol-ranked/v1/ranked-stats/{puuid}".to_string()
                                                        }
                                                    });

                                                    view! {
                                                        <tr class=row_class>
                                                            <td class="recap-cell-pos">{ally.assigned_position}</td>
                                                            <td class="font-bold">{ally.champion_name}</td>
                                                            <td class=if ally.is_anonymized { "text-muted" } else { "font-bold" }>
                                                                {ally.summoner_name}
                                                                {if ally.is_self {
                                                                    view! { <span class="recap-inline-pill">"YOU"</span> }.into_any()
                                                                } else if ally.is_premade {
                                                                    view! { <span class="recap-inline-pill premade">"DUO"</span> }.into_any()
                                                                } else {
                                                                    ().into_any()
                                                                }}
                                                            </td>
                                                            <td class="font-mono text-muted">{puuid_display}</td>
                                                            <td class=if ally.is_anonymized { "text-muted" } else { "recap-val-accent" }>
                                                                {rank_display}
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>

                                    {move || {
                                        if show_xray.get() {
                                            let j1 = jump_cs.clone();
                                            let j2 = jump_sum.clone();
                                            let j3 = jump_rk.clone();
                                            view! {
                                                <footer class="recap-xray-footer">
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-lcu"
                                                        on:click=move |_| j1(SourceId::LeagueClient, "/lol-champ-select/v1/session")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/lol-champ-select/v1/session"</span>
                                                    </button>
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-lcu"
                                                        on:click=move |_| j2(SourceId::LeagueClient, "/lol-summoner/v2/summoners/puuid/{puuid}")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/lol-summoner/v2/summoners/puuid/{puuid}"</span>
                                                    </button>
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-lcu"
                                                        on:click=move |_| j3(SourceId::LeagueClient, "/lol-ranked/v1/ranked-stats/{puuid}")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/lol-ranked/v1/ranked-stats/{puuid}"</span>
                                                    </button>
                                                </footer>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }
                                    }}
                                }.into_any()
                            }
                            "InProgress" => {
                                let lg = snap.live_game.unwrap_or_else(|| {
                                    preset_snapshot(RecapPreset::InProgress, None)
                                        .live_game
                                        .unwrap()
                                });
                                let jump_ag = jump_to_endpoint.clone();
                                let jump_ap = jump_to_endpoint.clone();
                                let jump_ev = jump_to_endpoint.clone();

                                view! {
                                    <div class="recap-pane-header">
                                        <h2 id="recap-stage-heading" class="recap-pane-title">
                                            "LIVE CLIENT TELEMETRY (:2999)"
                                        </h2>
                                        <div class="recap-header-badges">
                                            <span class="recap-badge-live tabular-nums">
                                                {format!("{} • {}", lg.map_name, format_mm_ss(lg.game_time_sec))}
                                            </span>
                                        </div>
                                    </div>

                                    <dl class="recap-dl recap-dl-inline tabular-nums">
                                        <div class="recap-dl-row">
                                            <dt>"ACTIVE CHAMPION"</dt>
                                            <dd class="recap-val-live">{format!("{} (Level {})", lg.active_champion, lg.level)}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"SCORE (K / D / A)"</dt>
                                            <dd class="recap-val-accent">{format!("{} / {} / {} ({} CS)", lg.kills, lg.deaths, lg.assists, lg.creep_score)}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"CURRENT GOLD"</dt>
                                            <dd class="font-mono">{format!("{} G", lg.current_gold)}</dd>
                                        </div>
                                    </dl>

                                    <div class="recap-table-wrap">
                                        <table class="system-data-table tabular-nums">
                                            <thead>
                                                <tr>
                                                    <th>"EVENT ID"</th>
                                                    <th>"GAME TIME"</th>
                                                    <th>"EVENT TYPE"</th>
                                                    <th>"SUMMARY"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {lg.recent_events.into_iter().map(|ev| {
                                                    view! {
                                                        <tr>
                                                            <td class="font-mono text-muted">{format!("#{}", ev.event_id)}</td>
                                                            <td class="font-mono">{format_mm_ss(ev.event_time_sec)}</td>
                                                            <td class="recap-val-live font-bold">{ev.event_name}</td>
                                                            <td>{ev.description}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>

                                    {move || {
                                        if show_xray.get() {
                                            let j1 = jump_ap.clone();
                                            let j2 = jump_ag.clone();
                                            let j3 = jump_ev.clone();
                                            view! {
                                                <footer class="recap-xray-footer">
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-live"
                                                        on:click=move |_| j1(SourceId::LiveGame, "/liveclientdata/activeplayer")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/liveclientdata/activeplayer"</span>
                                                    </button>
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-live"
                                                        on:click=move |_| j2(SourceId::LiveGame, "/liveclientdata/allgamedata")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/liveclientdata/allgamedata"</span>
                                                    </button>
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-live"
                                                        on:click=move |_| j3(SourceId::LiveGame, "/liveclientdata/eventdata")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/liveclientdata/eventdata"</span>
                                                    </button>
                                                </footer>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }
                                    }}
                                }.into_any()
                            }
                            "Matchmaking" | "ReadyCheck" => {
                                let mm = snap.matchmaking.unwrap_or_else(|| {
                                    preset_snapshot(RecapPreset::Matchmaking, None)
                                        .matchmaking
                                        .unwrap()
                                });
                                let jump_mm = jump_to_endpoint.clone();

                                view! {
                                    <div class="recap-pane-header">
                                        <h2 id="recap-stage-heading" class="recap-pane-title">
                                            "MATCHMAKING QUEUE TELEMETRY"
                                        </h2>
                                        <span class="recap-tag-lcu">{format!("QUEUE #{}", mm.queue_id)}</span>
                                    </div>

                                    <dl class="recap-dl tabular-nums">
                                        <div class="recap-dl-row">
                                            <dt>"TIME IN QUEUE"</dt>
                                            <dd class="recap-val-primary">{format_mm_ss(mm.time_in_queue_sec)}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"ESTIMATED WAIT"</dt>
                                            <dd>{format_mm_ss(mm.estimated_queue_time_sec)}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"QUEUE STATUS"</dt>
                                            <dd class="recap-val-accent">
                                                {if mm.is_currently_in_queue { "SEARCHING FOR MATCH" } else { "IDLE" }}
                                            </dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"READY CHECK RESPONSE"</dt>
                                            <dd>{mm.player_response.unwrap_or_else(|| "Pending".to_string())}</dd>
                                        </div>
                                    </dl>

                                    {move || {
                                        if show_xray.get() {
                                            let j1 = jump_mm.clone();
                                            view! {
                                                <footer class="recap-xray-footer">
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-lcu"
                                                        on:click=move |_| j1(SourceId::LeagueClient, "/lol-matchmaking/v1/search")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/lol-matchmaking/v1/search"</span>
                                                    </button>
                                                </footer>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }
                                    }}
                                }.into_any()
                            }
                            "EndOfGame" | "PreEndOfGame" | "WaitingForStats" => {
                                let eog = snap.end_of_game.unwrap_or_else(|| {
                                    preset_snapshot(RecapPreset::EndOfGame, None)
                                        .end_of_game
                                        .unwrap()
                                });
                                let jump_eog = jump_to_endpoint.clone();

                                view! {
                                    <div class="recap-pane-header">
                                        <h2 id="recap-stage-heading" class="recap-pane-title">
                                            "POST-GAME HONORS & STATS BLOCK"
                                        </h2>
                                        <span class="recap-badge-ok">{eog.result.to_uppercase()}</span>
                                    </div>

                                    <dl class="recap-dl tabular-nums">
                                        <div class="recap-dl-row">
                                            <dt>"CHAMPION PLAYED"</dt>
                                            <dd class="recap-val-primary">{eog.champion_name}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"MATCH DURATION"</dt>
                                            <dd>{format_mm_ss(eog.game_length_sec as f64)}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"FINAL K / D / A"</dt>
                                            <dd class="recap-val-accent">{format!("{} / {} / {}", eog.kills, eog.deaths, eog.assists)}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"GOLD EARNED"</dt>
                                            <dd>{format!("{} G", eog.total_gold)}</dd>
                                        </div>
                                        <div class="recap-dl-row">
                                            <dt>"DAMAGE TO CHAMPIONS"</dt>
                                            <dd>{format!("{}", eog.damage_to_champions)}</dd>
                                        </div>
                                    </dl>

                                    {move || {
                                        if show_xray.get() {
                                            let j1 = jump_eog.clone();
                                            view! {
                                                <footer class="recap-xray-footer">
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-lcu"
                                                        on:click=move |_| j1(SourceId::LeagueClient, "/lol-end-of-game/v1/eog-stats-block")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/lol-end-of-game/v1/eog-stats-block"</span>
                                                    </button>
                                                </footer>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }
                                    }}
                                }.into_any()
                            }
                            _ => {
                                // Default / Lobby view
                                let lob = snap.lobby.unwrap_or_else(|| {
                                    preset_snapshot(RecapPreset::Lobby, None).lobby.unwrap()
                                });
                                let jump_lob = jump_to_endpoint.clone();

                                view! {
                                    <div class="recap-pane-header">
                                        <h2 id="recap-stage-heading" class="recap-pane-title">
                                            "PARTY & LOBBY FORMATION"
                                        </h2>
                                        <div class="recap-header-badges">
                                            <span class="recap-tag-lcu">{lob.queue_name}</span>
                                            <span class="font-mono text-muted">{lob.party_id}</span>
                                        </div>
                                    </div>

                                    <div class="recap-table-wrap">
                                        <table class="system-data-table tabular-nums">
                                            <thead>
                                                <tr>
                                                    <th>"SUMMONER"</th>
                                                    <th>"ROLE PREFERENCES"</th>
                                                    <th>"PARTY PUUID (PREMADE CACHE)"</th>
                                                    <th>"STATUS"</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                {lob.members.into_iter().map(|m| {
                                                    let roles = match (m.first_position, m.second_position) {
                                                        (Some(f), Some(s)) => format!("{f} / {s}"),
                                                        (Some(f), None) => f,
                                                        _ => "FILL".to_string(),
                                                    };
                                                    view! {
                                                        <tr>
                                                            <td class="font-bold">
                                                                {m.summoner_name}
                                                                {if m.is_leader {
                                                                    view! { <span class="recap-inline-pill">"LEADER"</span> }.into_any()
                                                                } else {
                                                                    ().into_any()
                                                                }}
                                                            </td>
                                                            <td class="recap-cell-pos">{roles}</td>
                                                            <td class="font-mono text-muted">
                                                                {m.puuid.unwrap_or_else(|| "—".to_string())}
                                                            </td>
                                                            <td>
                                                                <span class=if m.ready { "recap-badge-ok" } else { "recap-tag-muted" }>
                                                                    {if m.ready { "READY" } else { "NOT READY" }}
                                                                </span>
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </div>

                                    {move || {
                                        if show_xray.get() {
                                            let j1 = jump_lob.clone();
                                            view! {
                                                <footer class="recap-xray-footer">
                                                    <button
                                                        type="button"
                                                        class="recap-endpoint-chip src-lcu"
                                                        on:click=move |_| j1(SourceId::LeagueClient, "/lol-lobby/v2/lobby")
                                                    >
                                                        <span class="chip-method">"GET"</span>
                                                        <span>"/lol-lobby/v2/lobby"</span>
                                                    </button>
                                                </footer>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }
                                    }}
                                }.into_any()
                            }
                        }
                    }}
                </section>
            </div>
        </main>
    }
}
