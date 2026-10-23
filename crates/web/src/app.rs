use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <main style="font-family: sans-serif; padding: 24px; background: #0B1220; color: #E8ECF4; min-height: 100vh;">
            <h1>"Hello Hypatia"</h1>
            <p>"Observatoire local des services Riot en cours d'initialisation."</p>
        </main>
    }
}
