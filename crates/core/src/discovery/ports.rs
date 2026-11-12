use std::collections::HashSet;
use hypatia_shared::{ListenSocket, RemoteConn};
use netstat2::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState,
};
use tracing::{debug, warn};

/// Collects TCP listening sockets and established remote connections for a given set of PIDs.
/// Performs NO DNS resolution to preserve privacy and performance.
pub fn collect_sockets_for_pids(
    target_pids: &HashSet<u32>,
) -> (Vec<ListenSocket>, Vec<RemoteConn>) {
    let mut listen_sockets = Vec::new();
    let mut remote_connections = Vec::new();

    if target_pids.is_empty() {
        return (listen_sockets, remote_connections);
    }

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;

    let sockets_info = match get_sockets_info(af_flags, proto_flags) {
        Ok(info) => info,
        Err(e) => {
            warn!("Failed to query system socket information via netstat2: {:?}", e);
            return (listen_sockets, remote_connections);
        }
    };

    for s in sockets_info {
        if let ProtocolSocketInfo::Tcp(tcp) = s.protocol_socket_info {
            for &pid in &s.associated_pids {
                if !target_pids.contains(&pid) {
                    continue;
                }

                match tcp.state {
                    TcpState::Listen => {
                        listen_sockets.push(ListenSocket {
                            pid,
                            local: tcp.local_addr.to_string(),
                            port: tcp.local_port,
                            proto: "tcp".to_string(),
                        });
                    }
                    TcpState::Established => {
                        remote_connections.push(RemoteConn {
                            pid,
                            local: format!("{}:{}", tcp.local_addr, tcp.local_port),
                            remote: format!("{}:{}", tcp.remote_addr, tcp.remote_port),
                            state: "ESTABLISHED".to_string(),
                        });
                    }
                    _ => {
                        debug!(
                            "Ignoring TCP state {:?} for PID {} on port {}",
                            tcp.state, pid, tcp.local_port
                        );
                    }
                }
            }
        }
    }

    // Sort and deduplicate for deterministic snapshots
    listen_sockets.sort_by(|a, b| {
        a.pid
            .cmp(&b.pid)
            .then_with(|| a.port.cmp(&b.port))
            .then_with(|| a.local.cmp(&b.local))
    });
    listen_sockets.dedup();

    remote_connections.sort_by(|a, b| {
        a.pid
            .cmp(&b.pid)
            .then_with(|| a.remote.cmp(&b.remote))
            .then_with(|| a.local.cmp(&b.local))
    });
    remote_connections.dedup();

    (listen_sockets, remote_connections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_pids_returns_empty() {
        let (sockets, conns) = collect_sockets_for_pids(&HashSet::new());
        assert!(sockets.is_empty());
        assert!(conns.is_empty());
    }

    #[test]
    fn test_current_process_sockets() {
        let current_pid = std::process::id();
        let mut pids = HashSet::new();
        pids.insert(current_pid);
        let (_sockets, _conns) = collect_sockets_for_pids(&pids);
        // Does not panic and returns cleanly
    }
}
