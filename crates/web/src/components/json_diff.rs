use leptos::prelude::*;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    Added,
    Removed,
    Modified,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct DiffRow {
    pub path: String,
    pub status: DiffStatus,
    pub old_repr: Option<String>,
    pub new_repr: Option<String>,
    pub depth: usize,
}

fn format_leaf(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.len() > 100 {
                format!("\"{}...\"", &s[..100])
            } else {
                format!("\"{}\"", s)
            }
        }
        Value::Array(a) => format!("[{} items]", a.len()),
        Value::Object(m) => {
            if m.contains_key("$secret") {
                "🔒 [Hidden secret]".to_string()
            } else {
                format!("{{{}}} fields", m.len())
            }
        }
    }
}

fn compute_diff(
    prev: Option<&Value>,
    curr: Option<&Value>,
    path: &str,
    depth: usize,
    out: &mut Vec<DiffRow>,
) {
    match (prev, curr) {
        (None, Some(c)) => {
            // Complete addition
            out.push(DiffRow {
                path: path.to_string(),
                status: DiffStatus::Added,
                old_repr: None,
                new_repr: Some(format_leaf(c)),
                depth,
            });
            if let Value::Object(m) = c {
                if !m.contains_key("$secret") {
                    for (k, v) in m {
                        let sub_path = format!("{}/{}", path, k);
                        compute_diff(None, Some(v), &sub_path, depth + 1, out);
                    }
                }
            } else if let Value::Array(arr) = c {
                for (i, v) in arr.iter().enumerate() {
                    let sub_path = format!("{}/{}", path, i);
                    compute_diff(None, Some(v), &sub_path, depth + 1, out);
                }
            }
        }
        (Some(p), None) => {
            // Complete removal
            out.push(DiffRow {
                path: path.to_string(),
                status: DiffStatus::Removed,
                old_repr: Some(format_leaf(p)),
                new_repr: None,
                depth,
            });
            if let Value::Object(m) = p {
                if !m.contains_key("$secret") {
                    for (k, v) in m {
                        let sub_path = format!("{}/{}", path, k);
                        compute_diff(Some(v), None, &sub_path, depth + 1, out);
                    }
                }
            } else if let Value::Array(arr) = p {
                for (i, v) in arr.iter().enumerate() {
                    let sub_path = format!("{}/{}", path, i);
                    compute_diff(Some(v), None, &sub_path, depth + 1, out);
                }
            }
        }
        (Some(p), Some(c)) => {
            if p == c {
                out.push(DiffRow {
                    path: path.to_string(),
                    status: DiffStatus::Unchanged,
                    old_repr: Some(format_leaf(p)),
                    new_repr: Some(format_leaf(c)),
                    depth,
                });
                return;
            }

            match (p, c) {
                (Value::Object(pm), Value::Object(cm)) => {
                    let is_p_secret = pm.contains_key("$secret");
                    let is_c_secret = cm.contains_key("$secret");

                    if is_p_secret || is_c_secret {
                        out.push(DiffRow {
                            path: path.to_string(),
                            status: if is_p_secret == is_c_secret {
                                DiffStatus::Unchanged
                            } else {
                                DiffStatus::Modified
                            },
                            old_repr: Some(format_leaf(p)),
                            new_repr: Some(format_leaf(c)),
                            depth,
                        });
                        return;
                    }

                    let mut all_keys = BTreeSet::new();
                    for k in pm.keys() {
                        all_keys.insert(k.clone());
                    }
                    for k in cm.keys() {
                        all_keys.insert(k.clone());
                    }

                    for k in all_keys {
                        let sub_path = format!("{}/{}", path, k);
                        compute_diff(pm.get(&k), cm.get(&k), &sub_path, depth + 1, out);
                    }
                }
                (Value::Array(pa), Value::Array(ca)) => {
                    let max_len = pa.len().max(ca.len());
                    for i in 0..max_len {
                        let sub_path = format!("{}/{}", path, i);
                        compute_diff(pa.get(i), ca.get(i), &sub_path, depth + 1, out);
                    }
                }
                _ => {
                    out.push(DiffRow {
                        path: path.to_string(),
                        status: DiffStatus::Modified,
                        old_repr: Some(format_leaf(p)),
                        new_repr: Some(format_leaf(c)),
                        depth,
                    });
                }
            }
        }
        (None, None) => {}
    }
}

#[component]
pub fn JsonDiff(
    current: Value,
    previous: Option<Value>,
) -> impl IntoView {
    let only_changes = RwSignal::new(true);

    if previous.is_none() {
        return view! {
            <div class="diff-empty-state">
                <div class="empty-state-icon">"⧉"</div>
                <div class="empty-state-title">"No previous payload"</div>
                <div class="empty-state-text">
                    "This is the first event captured for this URI. Diff comparisons will appear automatically on the next event for this path."
                </div>
            </div>
        }.into_any();
    }

    let prev_val = previous.unwrap();
    let curr_val = current.clone();

    // Compute diff list
    let mut rows = Vec::new();
    compute_diff(Some(&prev_val), Some(&curr_val), "", 0, &mut rows);

    let total_rows = rows.len();
    let change_count = rows
        .iter()
        .filter(|r| r.status != DiffStatus::Unchanged)
        .count();

    view! {
        <div class="json-diff-container">
            <div class="diff-toolbar">
                <div class="diff-stats">
                    <span class="diff-stat-item tabular-nums">
                        {format!("Total: {} fields", total_rows)}
                    </span>
                    <span class="diff-stat-item mod-item tabular-nums">
                        {format!("{} change{}", change_count, if change_count == 1 { "" } else { "s" })}
                    </span>
                </div>

                <label class="diff-toggle-label">
                    <input
                        type="checkbox"
                        checked=move || only_changes.get()
                        on:change=move |ev| only_changes.set(event_target_checked(&ev))
                    />
                    <span>"Changes only"</span>
                </label>
            </div>

            <div class="diff-table" role="table" aria-label="Differential comparison">
                <div class="diff-header-row" role="row">
                    <div class="diff-col-path" role="columnheader">"Path / Key"</div>
                    <div class="diff-col-status" role="columnheader">"Status"</div>
                    <div class="diff-col-old" role="columnheader">"Previous"</div>
                    <div class="diff-col-new" role="columnheader">"Current"</div>
                </div>

                <div class="diff-body" role="rowgroup">
                    {move || {
                        let filter_changes = only_changes.get();
                        let filtered = rows
                            .iter()
                            .filter(|r| !filter_changes || r.status != DiffStatus::Unchanged)
                            .cloned()
                            .collect::<Vec<_>>();

                        if filtered.is_empty() {
                            return view! {
                                <div class="diff-empty-notice">
                                    "No changes detected between these two payloads."
                                </div>
                            }.into_any();
                        }

                        filtered.into_iter().map(|row| {
                            let (badge_txt, row_class) = match row.status {
                                DiffStatus::Added => ("+ Added", "diff-row-added"),
                                DiffStatus::Removed => ("- Removed", "diff-row-removed"),
                                DiffStatus::Modified => ("~ Modified", "diff-row-modified"),
                                DiffStatus::Unchanged => ("= Unchanged", "diff-row-unchanged"),
                            };

                            let path_label = if row.path.is_empty() {
                                "/".to_string()
                            } else {
                                row.path.clone()
                            };

                            view! {
                                <div class=format!("diff-data-row {}", row_class) role="row">
                                    <div class="diff-col-path tabular-nums" role="cell" title=path_label.clone()>
                                        <span class="diff-indent" style=format!("width: {}px;", row.depth * 12)></span>
                                        <span>{path_label.clone()}</span>
                                    </div>
                                    <div class="diff-col-status" role="cell">
                                        <span class=format!("diff-status-tag tag-{}", match row.status {
                                            DiffStatus::Added => "add",
                                            DiffStatus::Removed => "rem",
                                            DiffStatus::Modified => "mod",
                                            DiffStatus::Unchanged => "same",
                                        })>
                                            {badge_txt}
                                        </span>
                                    </div>
                                    <div class="diff-col-old" role="cell">
                                        {row.old_repr.unwrap_or_else(|| "—".to_string())}
                                    </div>
                                    <div class="diff-col-new" role="cell">
                                        {row.new_repr.unwrap_or_else(|| "—".to_string())}
                                    </div>
                                </div>
                            }
                        }).collect::<Vec<_>>().into_any()
                    }}
                </div>
            </div>
        </div>
    }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_diff_add_remove_modify() {
        let prev = serde_json::json!({
            "unchanged": 123,
            "modified": "old_val",
            "removed": true,
        });

        let curr = serde_json::json!({
            "unchanged": 123,
            "modified": "new_val",
            "added": "new_field",
        });

        let mut rows = Vec::new();
        compute_diff(Some(&prev), Some(&curr), "", 0, &mut rows);

        let added = rows.iter().find(|r| r.path == "/added");
        assert!(added.is_some());
        assert_eq!(added.unwrap().status, DiffStatus::Added);

        let removed = rows.iter().find(|r| r.path == "/removed");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().status, DiffStatus::Removed);

        let modified = rows.iter().find(|r| r.path == "/modified");
        assert!(modified.is_some());
        assert_eq!(modified.unwrap().status, DiffStatus::Modified);

        let unchanged = rows.iter().find(|r| r.path == "/unchanged");
        assert!(unchanged.is_some());
        assert_eq!(unchanged.unwrap().status, DiffStatus::Unchanged);
    }
}
