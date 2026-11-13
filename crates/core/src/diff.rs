//! Line-based JSON diff shown by the Telemetry Lab inspector.

use hypatia_shared::Lang;

/// Computes a human-readable line-by-line JSON diff between an optional previous JSON value
/// and a current JSON value in the requested language.
pub fn compute_json_diff_localized(
    prev: Option<&serde_json::Value>,
    curr: &serde_json::Value,
    lang: Lang,
) -> String {
    let curr_pretty = serde_json::to_string_pretty(curr).unwrap_or_else(|_| "{}".to_string());
    let Some(prev_val) = prev else {
        return format!(
            "{}\n{}\n{}",
            lang.pick(
                "--- (no previous state for this URI — initial snapshot)",
                "--- (aucun état précédent pour cette URI — instantané initial)",
            ),
            lang.pick("+++ current", "+++ actuel"),
            curr_pretty
                .lines()
                .map(|l| format!("+ {}", l))
                .collect::<Vec<_>>()
                .join("\n")
        );
    };
    if prev_val == curr {
        return format!(
            "{}\n\n{}",
            lang.pick(
                "(No changes between previous and current payload)",
                "(Aucune modification entre le payload précédent et l'actuel)",
            ),
            curr_pretty
        );
    }

    let prev_pretty = serde_json::to_string_pretty(prev_val).unwrap_or_else(|_| "{}".to_string());
    let prev_lines: Vec<&str> = prev_pretty.lines().collect();
    let curr_lines: Vec<&str> = curr_pretty.lines().collect();

    let mut out = Vec::new();
    out.push(lang.pick("--- previous payload", "--- payload précédent").to_string());
    out.push(lang.pick("+++ current payload", "+++ payload actuel").to_string());

    let max_len = prev_lines.len().max(curr_lines.len());
    let mut i = 0;
    let mut j = 0;
    while i < prev_lines.len() || j < curr_lines.len() {
        match (prev_lines.get(i), curr_lines.get(j)) {
            (Some(&p), Some(&c)) if p == c => {
                out.push(format!("  {}", c));
                i += 1;
                j += 1;
            }
            (Some(&p), Some(&c)) => {
                // Look ahead up to 4 lines for resync
                if let Some(offset) = (1..=4).find(|&off| curr_lines.get(j + off) == Some(&p)) {
                    for k in 0..offset {
                        out.push(format!("+ {}", curr_lines[j + k]));
                    }
                    j += offset;
                } else if let Some(offset) = (1..=4).find(|&off| prev_lines.get(i + off) == Some(&c)) {
                    for k in 0..offset {
                        out.push(format!("- {}", prev_lines[i + k]));
                    }
                    i += offset;
                } else {
                    out.push(format!("- {}", p));
                    out.push(format!("+ {}", c));
                    i += 1;
                    j += 1;
                }
            }
            (Some(&p), None) => {
                out.push(format!("- {}", p));
                i += 1;
            }
            (None, Some(&c)) => {
                out.push(format!("+ {}", c));
                j += 1;
            }
            (None, None) => break,
        }
        if out.len() > max_len * 2 + 32 {
            break;
        }
    }

    out.join("\n")
}
