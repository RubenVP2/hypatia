use leptos::prelude::*;

#[component]
pub fn AboutView() -> impl IntoView {
    view! {
        <main class="live-view-container" style="padding: var(--gap-6); max-width: 800px; margin: 0 auto; overflow-y: auto;">
            <article style="display: flex; flex-direction: column; gap: var(--gap-4);">
                <header>
                    <h1 style="font-family: var(--font-display); font-size: var(--fs-2xl); font-weight: 700; color: var(--text);">
                        "À PROPOS D'HYPATIA"
                    </h1>
                    <p style="color: var(--text-muted); font-size: var(--fs-sm); margin-top: 4px;">
                        "Observatoire local en lecture seule des services et API Riot Games."
                    </p>
                </header>

                <section style="background: var(--surface); border: 1px solid var(--line); padding: var(--gap-4); border-radius: var(--radius); display: flex; flex-direction: column; gap: var(--gap-3);">
                    <h2 style="font-family: var(--font-display); font-size: var(--fs-lg); color: var(--src-lcu);">
                        "Garantie de lecture seule & Confidentialité"
                    </h2>
                    <p style="color: var(--text); font-size: var(--fs-sm); line-height: 1.6;">
                        "Hypatia n'émet strictement aucune requête de modification (POST, PUT, DELETE) vers les clients Riot. Seules des requêtes GET et des abonnements WebSocket passifs sont effectués. Vos identifiants et tokens de session restent masqués par défaut et ne sortent jamais de votre machine locale (127.0.0.1)."
                    </p>
                </section>

                <section style="background: var(--surface); border: 1px solid var(--line); padding: var(--gap-4); border-radius: var(--radius); display: flex; flex-direction: column; gap: var(--gap-3);">
                    <h2 style="font-family: var(--font-display); font-size: var(--fs-lg); color: var(--src-live);">
                        "Replay API"
                    </h2>
                    <p style="color: var(--text); font-size: var(--fs-sm); line-height: 1.6;">
                        "Pour observer les données de partie et les replays sur le port 2999, activez l'API dans le fichier de configuration de League of Legends en ajoutant :"
                    </p>
                    <pre style="background: var(--bg); border: 1px solid var(--line); padding: var(--gap-2); font-family: var(--font-data); font-size: var(--fs-xs); color: var(--kind-create);">
                        "[General]\nEnableReplayApi=1"
                    </pre>
                </section>
            </article>
        </main>
    }
}
