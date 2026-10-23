use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use crate::net::reveal_secret;

#[component]
pub fn SecretValue(
    id: u64,
    len: u32,
    #[prop(into)] hint: String,
) -> impl IntoView {
    let is_revealed = RwSignal::new(false);
    let revealed_text = RwSignal::new(String::new());
    let countdown = RwSignal::new(30u32);
    let is_loading = RwSignal::new(false);
    let copy_feedback = RwSignal::new(false);
    let error_msg = RwSignal::new(Option::<String>::None);

    let on_reveal = move |_| {
        if is_loading.get_untracked() || is_revealed.get_untracked() {
            return;
        }

        is_loading.set(true);
        error_msg.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match reveal_secret(id).await {
                Ok(val) => {
                    revealed_text.set(val);
                    is_revealed.set(true);
                    countdown.set(30);
                    is_loading.set(false);

                    // Compte à rebours de 30 secondes exactement
                    for rem in (1..=30).rev() {
                        countdown.set(rem);
                        TimeoutFuture::new(1000).await;
                        // Si l'utilisateur a masqué manuellement entre-temps
                        if !is_revealed.get_untracked() {
                            return;
                        }
                    }

                    // 30 secondes écoulées : re-masquage automatique
                    is_revealed.set(false);
                    revealed_text.set(String::new());
                    countdown.set(30);
                }
                Err(e) => {
                    is_loading.set(false);
                    error_msg.set(Some(e));
                }
            }
        });
    };

    let on_hide = move |_| {
        is_revealed.set(false);
        revealed_text.set(String::new());
        countdown.set(30);
    };

    let on_copy = move |_| {
        let text = revealed_text.get_untracked();
        if text.is_empty() {
            return;
        }

        if let Some(window) = web_sys::window() {
            let clipboard = window.navigator().clipboard();
            let _ = clipboard.write_text(&text);
            copy_feedback.set(true);

            wasm_bindgen_futures::spawn_local(async move {
                TimeoutFuture::new(2000).await;
                copy_feedback.set(false);
            });
        }
    };

    view! {
        <span class="secret-value-container" role="region" aria-label="Valeur masquée">
            {move || {
                if is_revealed.get() {
                    let text = revealed_text.get();
                    let rem = countdown.get();
                    let copied = copy_feedback.get();

                    view! {
                        <span class="secret-revealed-box">
                            <span class="secret-clear-text">{text}</span>
                            <span class="secret-countdown tabular-nums" title="Temps restant avant masquage automatique">
                                {format!("({}s)", rem)}
                            </span>
                            <button
                                type="button"
                                class="secret-btn copy-btn"
                                on:click=on_copy
                                title="Copier la valeur"
                            >
                                {if copied { "Copié !" } else { "Copier" }}
                            </button>
                            <button
                                type="button"
                                class="secret-btn hide-btn"
                                on:click=on_hide
                                title="Masquer immédiatement"
                            >
                                "Masquer"
                            </button>
                        </span>
                    }.into_any()
                } else {
                    let loading = is_loading.get();
                    let err = error_msg.get();
                    let hint_val = hint.clone();

                    view! {
                        <span class="secret-masked-box">
                            <span class="secret-masked-text">
                                {format!("🔒 •••• ({} octets) - {}", len, hint_val)}
                            </span>
                            <button
                                type="button"
                                class="secret-btn reveal-btn"
                                on:click=on_reveal
                                disabled=loading
                                title="Révéler la valeur pendant 30 secondes"
                            >
                                {if loading { "Chargement..." } else { "Révéler" }}
                            </button>
                            {if let Some(e) = err {
                                view! { <span class="secret-error">{e}</span> }.into_any()
                            } else {
                                ().into_any()
                            }}
                        </span>
                    }.into_any()
                }
            }}
        </span>
    }
}
