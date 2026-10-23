use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use serde_json::Value;
use crate::components::secret_value::SecretValue;

const MAX_STRING_PREVIEW_LEN: usize = 200;
const ARRAY_PAGE_SIZE: usize = 100;

fn check_secret(val: &Value) -> Option<(u64, u32, String)> {
    if let Value::Object(map) = val {
        if map.len() == 1 {
            if let Some(sec_val) = map.get("$secret") {
                if let Value::Object(sec_map) = sec_val {
                    let id = sec_map.get("id").and_then(|v| v.as_u64());
                    let len = sec_map.get("len").and_then(|v| v.as_u64()).map(|v| v as u32);
                    let hint = sec_map
                        .get("hint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("secret")
                        .to_string();
                    if let (Some(id), Some(len)) = (id, len) {
                        return Some((id, len, hint));
                    }
                }
            }
        }
    }
    None
}

fn copy_to_clipboard(text: &str, feedback: RwSignal<Option<String>>) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(text);
        feedback.set(Some(format!("Pointer {} copié !", text)));

        wasm_bindgen_futures::spawn_local(async move {
            TimeoutFuture::new(2000).await;
            feedback.set(None);
        });
    }
}

#[component]
pub fn JsonTree(value: Value) -> impl IntoView {
    let pointer_feedback = RwSignal::new(Option::<String>::None);
    let root_val = value.clone();

    view! {
        <div class="json-tree-root" role="tree" aria-label="Arborescence JSON">
            {move || {
                if let Some(feedback) = pointer_feedback.get() {
                    view! {
                        <div class="json-pointer-toast" role="status" aria-live="polite">
                            {feedback}
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}
            <JsonNode
                key="".to_string()
                value=root_val
                pointer="".to_string()
                depth=0
                pointer_feedback=pointer_feedback
            />
        </div>
    }
}

#[component]
fn JsonNode(
    key: String,
    value: Value,
    pointer: String,
    depth: usize,
    pointer_feedback: RwSignal<Option<String>>,
) -> impl IntoView {
    // Les 2 premiers niveaux (profondeur 0 et 1) sont dépliés par défaut
    let is_expanded = RwSignal::new(depth < 2);
    let show_all_str = RwSignal::new(false);
    let array_page = RwSignal::new(0usize);

    // Détection d'un secret masqué
    if let Some((sec_id, sec_len, sec_hint)) = check_secret(&value) {
        let key_display = if !key.is_empty() {
            format!("\"{}\": ", key)
        } else {
            String::new()
        };
        let p = pointer.clone();

        return view! {
            <div class="json-tree-row" role="treeitem">
                <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                {if !key.is_empty() {
                    let ptr_for_click = p.clone();
                    view! {
                        <span
                            class="json-key"
                            title=format!("Clic pour copier le pointer: {}", p)
                            on:click=move |_| copy_to_clipboard(&ptr_for_click, pointer_feedback)
                        >
                            {key_display}
                        </span>
                    }.into_any()
                } else {
                    ().into_any()
                }}
                <SecretValue id=sec_id len=sec_len hint=sec_hint />
            </div>
        }.into_any();
    }

    match value {
        Value::Null => {
            let key_display = if !key.is_empty() { format!("\"{}\": ", key) } else { String::new() };
            let p = pointer.clone();
            view! {
                <div class="json-tree-row" role="treeitem">
                    <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                    {if !key.is_empty() {
                        let ptr_click = p.clone();
                        view! {
                            <span
                                class="json-key"
                                title=format!("Clic pour copier le pointer: {}", p)
                                on:click=move |_| copy_to_clipboard(&ptr_click, pointer_feedback)
                            >
                                {key_display}
                            </span>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                    <span class="json-null">"null"</span>
                </div>
            }.into_any()
        }
        Value::Bool(b) => {
            let key_display = if !key.is_empty() { format!("\"{}\": ", key) } else { String::new() };
            let p = pointer.clone();
            view! {
                <div class="json-tree-row" role="treeitem">
                    <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                    {if !key.is_empty() {
                        let ptr_click = p.clone();
                        view! {
                            <span
                                class="json-key"
                                title=format!("Clic pour copier le pointer: {}", p)
                                on:click=move |_| copy_to_clipboard(&ptr_click, pointer_feedback)
                            >
                                {key_display}
                            </span>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                    <span class="json-bool">{b.to_string()}</span>
                </div>
            }.into_any()
        }
        Value::Number(num) => {
            let key_display = if !key.is_empty() { format!("\"{}\": ", key) } else { String::new() };
            let p = pointer.clone();
            view! {
                <div class="json-tree-row" role="treeitem">
                    <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                    {if !key.is_empty() {
                        let ptr_click = p.clone();
                        view! {
                            <span
                                class="json-key"
                                title=format!("Clic pour copier le pointer: {}", p)
                                on:click=move |_| copy_to_clipboard(&ptr_click, pointer_feedback)
                            >
                                {key_display}
                            </span>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                    <span class="json-num tabular-nums">{num.to_string()}</span>
                </div>
            }.into_any()
        }
        Value::String(s) => {
            let key_display = if !key.is_empty() { format!("\"{}\": ", key) } else { String::new() };
            let p = pointer.clone();
            let is_long = s.len() > MAX_STRING_PREVIEW_LEN;
            let full_text = s.clone();

            view! {
                <div class="json-tree-row" role="treeitem">
                    <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                    {if !key.is_empty() {
                        let ptr_click = p.clone();
                        view! {
                            <span
                                class="json-key"
                                title=format!("Clic pour copier le pointer: {}", p)
                                on:click=move |_| copy_to_clipboard(&ptr_click, pointer_feedback)
                            >
                                {key_display}
                            </span>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                    {if is_long {
                        let full = full_text.clone();
                        let truncated = format!("\"{}...\"", &s[..MAX_STRING_PREVIEW_LEN]);
                        let full_quoted = format!("\"{}\"", full);
                        let full_len = s.len();

                        view! {
                            <span class="json-str-container">
                                <span class="json-str">
                                    {move || if show_all_str.get() { full_quoted.clone() } else { truncated.clone() }}
                                </span>
                                <button
                                    type="button"
                                    class="json-toggle-btn"
                                    on:click=move |_| show_all_str.update(|v| *v = !*v)
                                >
                                    {move || if show_all_str.get() {
                                        "Tronquer".to_string()
                                    } else {
                                        format!("Afficher tout ({} car.)", full_len)
                                    }}
                                </button>
                            </span>
                        }.into_any()
                    } else {
                        view! {
                            <span class="json-str">{format!("\"{}\"", s)}</span>
                        }.into_any()
                    }}
                </div>
            }.into_any()
        }
        Value::Array(arr) => {
            let len = arr.len();
            let p = pointer.clone();
            let key_display = if !key.is_empty() { format!("\"{}\": ", key) } else { String::new() };

            view! {
                <div class="json-tree-complex-node">
                    <div
                        class="json-tree-row json-expandable"
                        role="treeitem"
                        aria-expanded=move || is_expanded.get().to_string()
                    >
                        <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                        <button
                            type="button"
                            class="json-fold-btn"
                            on:click=move |_| is_expanded.update(|v| *v = !*v)
                            aria-label=move || if is_expanded.get() { "Replier" } else { "Déplier" }
                        >
                            {move || if is_expanded.get() { "▾" } else { "▸" }}
                        </button>
                        {if !key.is_empty() {
                            let ptr_click = p.clone();
                            view! {
                                <span
                                    class="json-key"
                                    title=format!("Clic pour copier le pointer: {}", p)
                                    on:click=move |_| copy_to_clipboard(&ptr_click, pointer_feedback)
                                >
                                    {key_display}
                                </span>
                            }.into_any()
                        } else {
                            ().into_any()
                        }}
                        <span class="json-bracket">"["</span>
                        <span class="json-item-count">{format!("{} éléments", len)}</span>
                        {move || if !is_expanded.get() {
                            view! { <span class="json-bracket">"]"</span> }.into_any()
                        } else {
                            ().into_any()
                        }}
                    </div>

                    {move || {
                        if !is_expanded.get() {
                            return ().into_any();
                        }

                        let total_pages = (len + ARRAY_PAGE_SIZE - 1) / ARRAY_PAGE_SIZE;
                        let cur_page = array_page.get();
                        let start = cur_page * ARRAY_PAGE_SIZE;
                        let end = (start + ARRAY_PAGE_SIZE).min(len);
                        let slice = &arr[start..end];

                        view! {
                            <div class="json-tree-children">
                                {if len > ARRAY_PAGE_SIZE {
                                    view! {
                                        <div class="json-pagination" style=format!("margin-left: {}px;", (depth + 1) * 16)>
                                            <span>{format!("Éléments {}–{} sur {}", start + 1, end, len)}</span>
                                            <div class="json-pagination-controls">
                                                <button
                                                    type="button"
                                                    class="json-page-btn"
                                                    disabled=cur_page == 0
                                                    on:click=move |_| array_page.update(|p| *p = p.saturating_sub(1))
                                                >
                                                    "◀ Précédent"
                                                </button>
                                                <span class="tabular-nums">{format!("{}/{}", cur_page + 1, total_pages)}</span>
                                                <button
                                                    type="button"
                                                    class="json-page-btn"
                                                    disabled=cur_page + 1 >= total_pages
                                                    on:click=move |_| array_page.update(|p| *p = (*p + 1).min(total_pages - 1))
                                                >
                                                    "Suivant ▶"
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }}

                                {slice.iter().enumerate().map(|(idx, item)| {
                                    let actual_idx = start + idx;
                                    let child_ptr = format!("{}/{}", p, actual_idx);
                                    let child_val = item.clone();

                                    view! {
                                        <JsonNode
                                            key=actual_idx.to_string()
                                            value=child_val
                                            pointer=child_ptr
                                            depth=depth + 1
                                            pointer_feedback=pointer_feedback
                                        />
                                    }
                                }).collect::<Vec<_>>()}

                                <div class="json-tree-row">
                                    <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                                    <span class="json-bracket">"]"</span>
                                </div>
                            </div>
                        }.into_any()
                    }}
                </div>
            }.into_any()
        }
        Value::Object(map) => {
            let len = map.len();
            let p = pointer.clone();
            let key_display = if !key.is_empty() { format!("\"{}\": ", key) } else { String::new() };

            view! {
                <div class="json-tree-complex-node">
                    <div
                        class="json-tree-row json-expandable"
                        role="treeitem"
                        aria-expanded=move || is_expanded.get().to_string()
                    >
                        <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                        <button
                            type="button"
                            class="json-fold-btn"
                            on:click=move |_| is_expanded.update(|v| *v = !*v)
                            aria-label=move || if is_expanded.get() { "Replier" } else { "Déplier" }
                        >
                            {move || if is_expanded.get() { "▾" } else { "▸" }}
                        </button>
                        {if !key.is_empty() {
                            let ptr_click = p.clone();
                            view! {
                                <span
                                    class="json-key"
                                    title=format!("Clic pour copier le pointer: {}", p)
                                    on:click=move |_| copy_to_clipboard(&ptr_click, pointer_feedback)
                                >
                                    {key_display}
                                </span>
                            }.into_any()
                        } else {
                            ().into_any()
                        }}
                        <span class="json-bracket">"{"</span>
                        <span class="json-item-count">{format!("{} champs", len)}</span>
                        {move || if !is_expanded.get() {
                            view! { <span class="json-bracket">"}"</span> }.into_any()
                        } else {
                            ().into_any()
                        }}
                    </div>

                    {move || {
                        if !is_expanded.get() {
                            return ().into_any();
                        }

                        view! {
                            <div class="json-tree-children">
                                {map.iter().map(|(k, child_val)| {
                                    let escaped_k = k.replace('~', "~0").replace('/', "~1");
                                    let child_ptr = format!("{}/{}", p, escaped_k);
                                    let k_str = k.clone();
                                    let v_clone = child_val.clone();

                                    view! {
                                        <JsonNode
                                            key=k_str
                                            value=v_clone
                                            pointer=child_ptr
                                            depth=depth + 1
                                            pointer_feedback=pointer_feedback
                                        />
                                    }
                                }).collect::<Vec<_>>()}

                                <div class="json-tree-row">
                                    <span class="json-tree-indent" style=format!("width: {}px;", depth * 16)></span>
                                    <span class="json-bracket">"}"</span>
                                </div>
                            </div>
                        }.into_any()
                    }}
                </div>
            }.into_any()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_secret_detection() {
        let sec = serde_json::json!({
            "$secret": {
                "id": 42,
                "len": 128,
                "hint": "token"
            }
        });
        let detected = check_secret(&sec);
        assert_eq!(detected, Some((42, 128, "token".to_string())));

        let normal = serde_json::json!({
            "not_secret": "value"
        });
        assert_eq!(check_secret(&normal), None);
    }
}
