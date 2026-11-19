use std::env;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use hypatia_shared::{RiotClientLockfile, SecretRef};
use tracing::{debug, warn};

use crate::redaction::SecretVault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredCredentials {
    pub name: String,
    pub pid: u32,
    pub port: u16,
    pub protocol: String,
    pub secret_ref: SecretRef,
    pub raw_password: String,
}

/// Attempts to locate the Riot Client lockfile on Windows or WSL.
pub fn locate_riot_client_lockfile() -> Option<PathBuf> {
    if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
        let p = PathBuf::from(local_appdata)
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile");
        if p.exists() {
            return Some(p);
        }
    }

    if let Ok(user_profile) = env::var("USERPROFILE") {
        let p = PathBuf::from(user_profile)
            .join("AppData")
            .join("Local")
            .join("Riot Games")
            .join("Riot Client")
            .join("Config")
            .join("lockfile");
        if p.exists() {
            return Some(p);
        }
    }

    // Fallback search in /mnt/c/ if running in WSL
    #[cfg(unix)]
    {
        if let Ok(entries) = std::fs::read_dir("/mnt/c/Users") {
            for entry in entries.flatten() {
                let candidate = entry
                    .path()
                    .join("AppData")
                    .join("Local")
                    .join("Riot Games")
                    .join("Riot Client")
                    .join("Config")
                    .join("lockfile");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Attempts to locate the League of Legends client lockfile.
/// Priority:
/// 1. `<install_directory>\lockfile` (if provided)
/// 2. `C:\Riot Games\League of Legends\lockfile`
/// 3. WSL `/mnt/c/Riot Games/League of Legends/lockfile`
pub fn locate_league_client_lockfile(install_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = install_dir {
        let candidate = dir.join("lockfile");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let default_win = PathBuf::from(r"C:\Riot Games\League of Legends\lockfile");
    if default_win.exists() {
        return Some(default_win);
    }

    let default_d = PathBuf::from(r"D:\Riot Games\League of Legends\lockfile");
    if default_d.exists() {
        return Some(default_d);
    }

    #[cfg(unix)]
    {
        let default_wsl = PathBuf::from("/mnt/c/Riot Games/League of Legends/lockfile");
        if default_wsl.exists() {
            return Some(default_wsl);
        }
    }

    None
}

/// Opens a file with shared read access so that concurrent writers (e.g. Riot Client)
/// do not cause a sharing violation error.
#[cfg(windows)]
fn open_shared_read(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_SHARE_READ (1) | FILE_SHARE_WRITE (2) | FILE_SHARE_DELETE (4) = 7
    let share_mode = 0x00000001 | 0x00000002 | 0x00000004;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(share_mode)
        .open(path)
}

#[cfg(not(windows))]
fn open_shared_read(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

/// Reads the lockfile tolerantly. Retries up to 3 times if the file is temporarily locked
/// or being written. Never panics.
pub fn read_lockfile_tolerant(path: &Path) -> Option<RiotClientLockfile> {
    const MAX_ATTEMPTS: usize = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(25);

    for attempt in 1..=MAX_ATTEMPTS {
        match open_shared_read(path) {
            Ok(mut file) => {
                let mut content = String::new();
                if file.read_to_string(&mut content).is_ok() {
                    if let Some(parsed) = RiotClientLockfile::parse(&content) {
                        return Some(parsed);
                    }
                    debug!(
                        "Lockfile {:?} read successfully but failed parsing: {:?}",
                        path, content
                    );
                }
            }
            Err(e) => {
                debug!(
                    "Attempt {}/{} to read lockfile {:?} failed: {:?}",
                    attempt, MAX_ATTEMPTS, path, e
                );
            }
        }

        if attempt < MAX_ATTEMPTS {
            thread::sleep(RETRY_DELAY);
        }
    }

    warn!("Failed to read lockfile at {:?}", path);
    None
}

/// Discovers Riot Client credentials from its lockfile and stores the password in the `SecretVault`.
pub fn discover_riot_client_lockfile(vault: &SecretVault) -> Option<DiscoveredCredentials> {
    let path = locate_riot_client_lockfile()?;
    let parsed = read_lockfile_tolerant(&path)?;
    let secret_ref = vault.store(parsed.password.clone(), "riotclient_token".to_string());

    Some(DiscoveredCredentials {
        name: parsed.name,
        pid: parsed.pid,
        port: parsed.port,
        protocol: parsed.protocol,
        secret_ref,
        raw_password: parsed.password,
    })
}

/// Discovers League Client credentials from its lockfile and stores the password in the `SecretVault`.
pub fn discover_league_client_lockfile(
    install_dir: Option<&Path>,
    vault: &SecretVault,
) -> Option<DiscoveredCredentials> {
    let path = locate_league_client_lockfile(install_dir)?;
    let parsed = read_lockfile_tolerant(&path)?;
    let secret_ref = vault.store(parsed.password.clone(), "lcu_lockfile_token".to_string());

    Some(DiscoveredCredentials {
        name: parsed.name,
        pid: parsed.pid,
        port: parsed.port,
        protocol: parsed.protocol,
        secret_ref,
        raw_password: parsed.password,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_lockfile_tolerant() {
        let mut temp = NamedTempFile::new().unwrap();
        writeln!(temp, "LeagueClient:4372:13061:2qLgXWI6CkO_wYIzH36blA:https").unwrap();

        let parsed = read_lockfile_tolerant(temp.path()).expect("Should parse valid lockfile");
        assert_eq!(parsed.name, "LeagueClient");
        assert_eq!(parsed.pid, 4372);
        assert_eq!(parsed.port, 13061);
        assert_eq!(parsed.password, "2qLgXWI6CkO_wYIzH36blA");
        assert_eq!(parsed.protocol, "https");
    }

    #[test]
    fn test_read_lockfile_invalid_content() {
        let mut temp = NamedTempFile::new().unwrap();
        writeln!(temp, "invalid_lockfile_format").unwrap();

        let parsed = read_lockfile_tolerant(temp.path());
        assert!(parsed.is_none());
    }

    #[test]
    fn test_discover_stores_secret_in_vault() {
        let vault = SecretVault::new();
        let mut temp = NamedTempFile::new().unwrap();
        writeln!(temp, "Riot Client:14852:12840:test_password_123:https").unwrap();

        let parsed = read_lockfile_tolerant(temp.path()).unwrap();
        let secret_ref = vault.store(parsed.password.clone(), "test".to_string());

        assert_eq!(
            vault.reveal(secret_ref.id),
            Some("test_password_123".to_string())
        );
    }
}
