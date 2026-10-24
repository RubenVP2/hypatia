use leptos::prelude::*;

#[component]
pub fn AboutView() -> impl IntoView {
    view! {
        <main class="live-view-container" style="padding: var(--gap-6); max-width: 800px; margin: 0 auto; overflow-y: auto;">
            <article style="display: flex; flex-direction: column; gap: var(--gap-4);">
                <header>
                    <h1 style="font-family: var(--font-display); font-size: var(--fs-2xl); font-weight: 700; color: var(--text);">
                        "ABOUT HYPATIA"
                    </h1>
                    <p style="color: var(--text-muted); font-size: var(--fs-sm); margin-top: 4px;">
                        "Local read-only observatory for Riot Games services and APIs."
                    </p>
                </header>

                <section style="background: var(--surface); border: 1px solid var(--line); padding: var(--gap-4); border-radius: var(--radius); display: flex; flex-direction: column; gap: var(--gap-3);">
                    <h2 style="font-family: var(--font-display); font-size: var(--fs-lg); color: var(--src-lcu);">
                        "Read-Only Guarantee & Privacy"
                    </h2>
                    <p style="color: var(--text); font-size: var(--fs-sm); line-height: 1.6;">
                        "Hypatia strictly issues no modifying requests (POST, PUT, DELETE) to Riot clients. Only GET requests and passive WebSocket subscriptions are performed. Your credentials and session tokens remain masked by default and never leave your local machine (127.0.0.1)."
                    </p>
                </section>

                <section style="background: var(--surface); border: 1px solid var(--line); padding: var(--gap-4); border-radius: var(--radius); display: flex; flex-direction: column; gap: var(--gap-3);">
                    <h2 style="font-family: var(--font-display); font-size: var(--fs-lg); color: var(--src-live);">
                        "Replay API"
                    </h2>
                    <p style="color: var(--text); font-size: var(--fs-sm); line-height: 1.6;">
                        "To observe game data and replays on port 2999, enable the API in the League of Legends configuration file by adding:"
                    </p>
                    <pre style="background: var(--bg); border: 1px solid var(--line); padding: var(--gap-2); font-family: var(--font-data); font-size: var(--fs-xs); color: var(--kind-create);">
                        "[General]\nEnableReplayApi=1"
                    </pre>
                </section>
            </article>
        </main>
    }
}
