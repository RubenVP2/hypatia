use hypatia_shared::{ProcessInfo, SourceId};
use std::ffi::OsStr;
use std::path::PathBuf;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

const RIOT_KEYWORDS: &[&str] = &["riot", "league", "valorant", "vgc", "vgtray"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeagueClientArgs {
    pub app_port: Option<u16>,
    pub remoting_auth_token: Option<String>,
    pub install_directory: Option<PathBuf>,
    pub riotclient_app_port: Option<u16>,
    pub riotclient_auth_token: Option<String>,
    pub app_pid: Option<u32>,
}

/// Parses command line arguments of `LeagueClientUx.exe`.
pub fn parse_lcu_args<I, S>(args: I) -> LeagueClientArgs
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut parsed = LeagueClientArgs::default();
    let string_args: Vec<String> = args
        .into_iter()
        .map(|s| s.as_ref().to_string_lossy().to_string())
        .collect();

    let mut i = 0;
    while i < string_args.len() {
        let arg = &string_args[i];

        let extract_val =
            |key: &str, current_arg: &str, next_arg: Option<&String>| -> Option<(String, usize)> {
                let key_eq = format!("{}=", key);
                if let Some(val) = current_arg.strip_prefix(&key_eq) {
                    let clean = val.trim_matches('"').trim_matches('\'').to_string();
                    return Some((clean, 1));
                }
                if current_arg == key {
                    if let Some(next) = next_arg {
                        let clean = next.trim_matches('"').trim_matches('\'').to_string();
                        return Some((clean, 2));
                    }
                }
                None
            };

        let next = string_args.get(i + 1);

        if let Some((val, advance)) = extract_val("--app-port", arg, next) {
            if let Ok(port) = val.parse::<u16>() {
                parsed.app_port = Some(port);
            }
            i += advance;
            continue;
        }

        if let Some((val, advance)) = extract_val("--remoting-auth-token", arg, next) {
            parsed.remoting_auth_token = Some(val);
            i += advance;
            continue;
        }

        if let Some((val, advance)) = extract_val("--install-directory", arg, next) {
            parsed.install_directory = Some(PathBuf::from(val));
            i += advance;
            continue;
        }

        if let Some((val, advance)) = extract_val("--riotclient-app-port", arg, next) {
            if let Ok(port) = val.parse::<u16>() {
                parsed.riotclient_app_port = Some(port);
            }
            i += advance;
            continue;
        }

        if let Some((val, advance)) = extract_val("--riotclient-auth-token", arg, next) {
            parsed.riotclient_auth_token = Some(val);
            i += advance;
            continue;
        }

        if let Some((val, advance)) = extract_val("--app-pid", arg, next) {
            if let Ok(pid) = val.parse::<u32>() {
                parsed.app_pid = Some(pid);
            }
            i += advance;
            continue;
        }

        i += 1;
    }

    parsed
}

/// Checks whether a process name matches any Riot-related keyword (case-insensitive).
pub fn is_riot_process(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    RIOT_KEYWORDS.iter().any(|&kw| lower.contains(kw))
}

/// Classifies a process into its corresponding `SourceId`.
pub fn classify_source(name: &str) -> Option<SourceId> {
    let lower = name.to_ascii_lowercase();

    // Live Game (executable is "League of Legends.exe")
    if lower.contains("league of legends") {
        return Some(SourceId::LiveGame);
    }

    // League Client UX / Client / Helpers / CrashHandler
    if lower.contains("leagueclient") || lower.contains("leaguecrashhandler") {
        return Some(SourceId::LeagueClient);
    }

    // Riot Client Services / CrashHandler
    if lower.contains("riotclient") || lower.contains("riot client") {
        return Some(SourceId::RiotClient);
    }

    // Other Riot ecosystem processes (vgc, vgtray, valorant, etc.)
    if is_riot_process(&lower) {
        return Some(SourceId::System);
    }

    None
}

/// Lists Riot-related processes and extracts the LeagueClientUx command-line arguments.
///
/// Pass 1 refreshes only the process table (names), which is a single cheap system call.
/// Pass 2 queries details for the handful of Riot processes: the command line once per
/// process, plus CPU / memory / executable path when `detailed` is set.
pub fn scan_processes(
    sys: &mut System,
    detailed: bool,
) -> (Vec<ProcessInfo>, Option<LeagueClientArgs>) {
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());

    let riot_pids: Vec<Pid> = sys
        .processes()
        .iter()
        .filter(|(_, proc)| is_riot_process(&proc.name().to_string_lossy()))
        .map(|(pid, _)| *pid)
        .collect();
    if riot_pids.is_empty() {
        return (Vec::new(), None);
    }

    let mut kind = ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet);
    if detailed {
        kind = kind
            .with_cpu()
            .with_memory()
            .with_exe(UpdateKind::OnlyIfNotSet);
    }
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&riot_pids), false, kind);

    let mut riot_processes = Vec::with_capacity(riot_pids.len());
    let mut lcu_args: Option<LeagueClientArgs> = None;

    for pid in &riot_pids {
        let Some(proc) = sys.process(*pid) else {
            continue;
        };
        let name = proc.name().to_string_lossy().into_owned();

        // The main LeagueClientUx process (not a helper / renderer) carries the credentials.
        let lower_name = name.to_ascii_lowercase();
        if lower_name.contains("leagueclientux")
            && !lower_name.contains("helper")
            && !lower_name.contains("render")
        {
            let parsed = parse_lcu_args(proc.cmd());
            if parsed.remoting_auth_token.is_some() || parsed.app_port.is_some() {
                lcu_args = Some(parsed);
            }
        }

        riot_processes.push(ProcessInfo {
            pid: pid.as_u32(),
            source: classify_source(&name),
            exe: proc.exe().map(|p| p.to_string_lossy().into_owned()),
            started: proc.start_time() as i64,
            cpu: proc.cpu_usage(),
            mem_bytes: proc.memory(),
            name,
        });
    }

    riot_processes.sort_by_key(|p| p.pid);
    (riot_processes, lcu_args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_riot_process() {
        assert!(is_riot_process("LeagueClientUx.exe"));
        assert!(is_riot_process("RiotClientServices.exe"));
        assert!(is_riot_process("League of Legends.exe"));
        assert!(is_riot_process("vgc.exe"));
        assert!(is_riot_process("vgtray.exe"));
        assert!(is_riot_process("VALORANT-Win64-Shipping.exe"));

        assert!(!is_riot_process("chrome.exe"));
        assert!(!is_riot_process("explorer.exe"));
        assert!(!is_riot_process("cargo.exe"));
    }

    #[test]
    fn test_classify_source() {
        assert_eq!(
            classify_source("LeagueClientUx.exe"),
            Some(SourceId::LeagueClient)
        );
        assert_eq!(
            classify_source("LeagueClient.exe"),
            Some(SourceId::LeagueClient)
        );
        assert_eq!(
            classify_source("League of Legends.exe"),
            Some(SourceId::LiveGame)
        );
        assert_eq!(
            classify_source("RiotClientServices.exe"),
            Some(SourceId::RiotClient)
        );
        assert_eq!(classify_source("vgc.exe"), Some(SourceId::System));
        assert_eq!(classify_source("vgtray.exe"), Some(SourceId::System));
        assert_eq!(classify_source("VALORANT.exe"), Some(SourceId::System));
    }

    #[test]
    fn test_scan_processes_does_not_panic() {
        let mut sys = System::new();
        let (procs, _lcu_args) = scan_processes(&mut sys, true);
        // On machine with running Riot processes, procs should contain them
        for proc in procs {
            assert!(is_riot_process(&proc.name));
        }
    }

    #[test]
    fn test_parse_lcu_args() {
        let cmd = vec![
            r#""C:/Riot Games/League of Legends/LeagueClientUx.exe""#,
            r#"--riotclient-auth-token=JsOV_TyvvR63GstTZo8OHg"#,
            r#"--riotclient-app-port=12840"#,
            r#"--remoting-auth-token=yLgkr1iQZbglyRh3N9thJA"#,
            r#"--app-port=13061"#,
            r#"--install-directory=C:\Riot Games\League of Legends"#,
            r#"--app-pid=4372"#,
        ];

        let parsed = parse_lcu_args(&cmd);
        assert_eq!(parsed.app_port, Some(13061));
        assert_eq!(
            parsed.remoting_auth_token.as_deref(),
            Some("yLgkr1iQZbglyRh3N9thJA")
        );
        assert_eq!(
            parsed.install_directory.as_deref(),
            Some(Path::new(r"C:\Riot Games\League of Legends"))
        );
        assert_eq!(parsed.riotclient_app_port, Some(12840));
        assert_eq!(
            parsed.riotclient_auth_token.as_deref(),
            Some("JsOV_TyvvR63GstTZo8OHg")
        );
        assert_eq!(parsed.app_pid, Some(4372));
    }
}
