use leptos::prelude::*;
use crate::state::AppState;

#[component]
pub fn Inspector() -> impl IntoView {
    let state = expect_context::<AppState>();
    let selected_event_sig = state.event_store.selected_event;

    view! {
        <aside class="inspector-pane" aria-label="Inspecteur de données et détails">
            {move || {
                match selected_event_sig.get() {
                    Some(ev) => {
                        let uri_clone = ev.uri.clone();
                        view! {
                            <div class="inspector-header">
                                <span class="inspector-title">"INSPECTEUR D'ÉVÉNEMENT"</span>
                                <div class="inspector-uri">{uri_clone}</div>
                                <div class="inspector-meta-row tabular-nums">
                                    <span>{format!("ID #{}", ev.id)}</span>
                                    <span>"•"</span>
                                    <span>{format!("{:?}", ev.source)}</span>
                                    <span>"•"</span>
                                    <span>{format!("{:?}", ev.kind)}</span>
                                    <span>"•"</span>
                                    <span>{format!("{} octets", ev.size)}</span>
                                    {if ev.has_secrets {
                                        view! { <span>"• 🔒 Secrets"</span> }.into_any()
                                    } else {
                                        ().into_any()
                                    }}
                                </div>
                            </div>

                            <div class="inspector-body">
                                <div style="font-size: var(--fs-xs); color: var(--text-muted); text-transform: uppercase; font-family: var(--font-display); font-weight: 600;">
                                    "Aperçu du payload masqué"
                                </div>
                                <pre class="inspector-payload-preview">
                                    {if ev.preview.is_empty() {
                                        "(Payload vide ou non capturé)".to_string()
                                    } else {
                                        ev.preview.clone()
                                    }}
                                </pre>
                            </div>
                        }.into_any()
                    }
                    None => {
                        view! {
                            <div class="empty-state" style="margin-top: 60px;">
                                <div class="empty-state-icon">"⌕"</div>
                                <div class="empty-state-text">
                                    "Sélectionnez un événement dans le tableau ou dans le bandeau inférieur pour inspecter ses données."
                                </div>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </aside>
    }
}
