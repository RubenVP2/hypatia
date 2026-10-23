use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    pub allowed_ports: Vec<u16>,
    pub dev_mode: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_ports: vec![7878, 8334],
            dev_mode: false,
        }
    }
}

impl SecurityConfig {
    pub fn new(allowed_ports: Vec<u16>, dev_mode: bool) -> Self {
        Self {
            allowed_ports,
            dev_mode,
        }
    }
}

/// Validates that the `Host` header corresponds strictly to `127.0.0.1` or `localhost`.
/// If a port is specified and `allowed_ports` is non-empty, the port must be in `allowed_ports`.
pub fn validate_host(host_header: &str, allowed_ports: &[u16]) -> bool {
    let host_header = host_header.trim();
    if host_header.is_empty() {
        return false;
    }

    let (hostname, port) = if let Some(stripped) = host_header.strip_prefix('[') {
        if let Some(end) = stripped.find(']') {
            let host = &stripped[..end];
            let rest = &stripped[end + 1..];
            let port = if let Some(p) = rest.strip_prefix(':') {
                p.parse::<u16>().ok()
            } else {
                None
            };
            (host, port)
        } else {
            return false;
        }
    } else if let Some((h, p)) = host_header.rsplit_once(':') {
        if let Ok(port_num) = p.parse::<u16>() {
            (h, Some(port_num))
        } else {
            (host_header, None)
        }
    } else {
        (host_header, None)
    };

    let valid_host = hostname.eq_ignore_ascii_case("127.0.0.1")
        || hostname.eq_ignore_ascii_case("localhost")
        || hostname == "::1";

    if !valid_host {
        return false;
    }

    if let Some(p) = port {
        if !allowed_ports.is_empty() && !allowed_ports.contains(&p) {
            return false;
        }
    }

    true
}

/// Validates that the `Origin` header (if present) points to a trusted local origin:
/// `http://127.0.0.1:{port}`, `http://localhost:{port}`, or Trunk dev server (`:8080`).
pub fn validate_origin(origin_header: &str, allowed_ports: &[u16], dev_mode: bool) -> bool {
    let origin = origin_header.trim();
    if origin.is_empty() {
        return true;
    }

    let without_scheme = if let Some(stripped) = origin.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = origin.strip_prefix("https://") {
        stripped
    } else {
        return false;
    };

    let authority = without_scheme.trim_end_matches('/');

    let (hostname, port) = if let Some((h, p)) = authority.rsplit_once(':') {
        if let Ok(port_num) = p.parse::<u16>() {
            (h, Some(port_num))
        } else {
            (authority, None)
        }
    } else {
        (authority, None)
    };

    let valid_host = hostname.eq_ignore_ascii_case("127.0.0.1")
        || hostname.eq_ignore_ascii_case("localhost");

    if !valid_host {
        return false;
    }

    // Always allow Trunk dev server port 8080 in dev_mode or general development
    if let Some(p) = port {
        if p == 8080 || (dev_mode && p == 8080) {
            return true;
        }
        if !allowed_ports.is_empty() && !allowed_ports.contains(&p) {
            return false;
        }
    }

    true
}

/// Checks whether an HTTP request complies with local security rules:
/// 1. Host header validation
/// 2. Origin header validation
/// 3. Strict HTTP method restrictions on `/api/*`
pub fn check_security(
    headers: &HeaderMap,
    method: &Method,
    path: &str,
    config: &SecurityConfig,
) -> Result<(), StatusCode> {
    // 1. Host header validation
    if let Some(host_val) = headers.get("host").and_then(|h| h.to_str().ok()) {
        if !validate_host(host_val, &config.allowed_ports) {
            tracing::warn!("Forbidden: Invalid Host header '{}'", host_val);
            return Err(StatusCode::FORBIDDEN);
        }
    } else if !config.allowed_ports.is_empty() {
        tracing::warn!("Forbidden: Missing Host header");
        return Err(StatusCode::FORBIDDEN);
    }

    // 2. Origin header validation for /ws, POST requests, and /api/* routes
    let requires_origin_check = path == "/ws" || *method == Method::POST || path.starts_with("/api/");
    if requires_origin_check {
        if let Some(origin_val) = headers.get("origin").and_then(|o| o.to_str().ok()) {
            if !validate_origin(origin_val, &config.allowed_ports, config.dev_mode) {
                tracing::warn!("Forbidden: Untrusted Origin header '{}'", origin_val);
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    // 3. Method validation on /api/*:
    // Only GET is permitted, EXCEPT `POST /api/secrets/{id}/reveal`.
    if path.starts_with("/api/") {
        let is_secret_reveal = *method == Method::POST
            && path.starts_with("/api/secrets/")
            && path.ends_with("/reveal");

        if *method != Method::GET && !is_secret_reveal {
            tracing::warn!(
                "Method Not Allowed: {} on {} (only GET and POST /api/secrets/{{id}}/reveal are allowed)",
                method,
                path
            );
            return Err(StatusCode::METHOD_NOT_ALLOWED);
        }
    }

    Ok(())
}

/// Axum middleware enforcing the security policy.
pub async fn security_middleware(
    State(config): State<SecurityConfig>,
    req: Request,
    next: Next,
) -> Response {
    let headers = req.headers();
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    match check_security(headers, &method, &path, &config) {
        Ok(()) => next.run(req).await,
        Err(status) => {
            let msg = if status == StatusCode::METHOD_NOT_ALLOWED {
                "Method Not Allowed"
            } else {
                "Forbidden"
            };
            (status, msg).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_validate_host() {
        let allowed = vec![7878, 8334];

        assert!(validate_host("127.0.0.1:7878", &allowed));
        assert!(validate_host("localhost:7878", &allowed));
        assert!(validate_host("127.0.0.1:8334", &allowed));
        assert!(validate_host("localhost:8334", &allowed));
        assert!(validate_host("127.0.0.1", &allowed));
        assert!(validate_host("localhost", &allowed));

        // Third-party hosts
        assert!(!validate_host("evil.test", &allowed));
        assert!(!validate_host("evil.test:7878", &allowed));
        assert!(!validate_host("attacker.com", &allowed));
        assert!(!validate_host("192.168.1.10:7878", &allowed));

        // Disallowed port
        assert!(!validate_host("127.0.0.1:9999", &allowed));
    }

    #[test]
    fn test_validate_origin() {
        let allowed = vec![7878];

        assert!(validate_origin("http://127.0.0.1:7878", &allowed, false));
        assert!(validate_origin("http://localhost:7878", &allowed, false));
        assert!(validate_origin("http://127.0.0.1:8080", &allowed, true));
        assert!(validate_origin("http://localhost:8080", &allowed, false));

        // Untrusted origin
        assert!(!validate_origin("http://evil.test", &allowed, false));
        assert!(!validate_origin("https://evil.test", &allowed, false));
        assert!(!validate_origin("http://attacker.com:7878", &allowed, false));
        assert!(!validate_origin("http://127.0.0.1:9999", &allowed, false));
    }

    #[test]
    fn test_check_security_method_and_paths() {
        let config = SecurityConfig::new(vec![7878], false);
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("127.0.0.1:7878"));

        // GET on /api/call is allowed
        assert!(check_security(&headers, &Method::GET, "/api/call/LeagueClient", &config).is_ok());

        // POST on /api/call is METHOD_NOT_ALLOWED
        assert_eq!(
            check_security(&headers, &Method::POST, "/api/call/LeagueClient", &config),
            Err(StatusCode::METHOD_NOT_ALLOWED)
        );

        // DELETE on /api/events/1 is METHOD_NOT_ALLOWED
        assert_eq!(
            check_security(&headers, &Method::DELETE, "/api/events/1", &config),
            Err(StatusCode::METHOD_NOT_ALLOWED)
        );

        // POST on /api/secrets/1/reveal is allowed
        assert!(check_security(&headers, &Method::POST, "/api/secrets/1/reveal", &config).is_ok());

        // PUT on /api/secrets/1/reveal is METHOD_NOT_ALLOWED
        assert_eq!(
            check_security(&headers, &Method::PUT, "/api/secrets/1/reveal", &config),
            Err(StatusCode::METHOD_NOT_ALLOWED)
        );

        // Evil host returns FORBIDDEN
        let mut evil_headers = HeaderMap::new();
        evil_headers.insert("host", HeaderValue::from_static("evil.test"));
        assert_eq!(
            check_security(&evil_headers, &Method::GET, "/api/health", &config),
            Err(StatusCode::FORBIDDEN)
        );

        // Evil origin on /ws returns FORBIDDEN
        let mut evil_origin_headers = HeaderMap::new();
        evil_origin_headers.insert("host", HeaderValue::from_static("127.0.0.1:7878"));
        evil_origin_headers.insert("origin", HeaderValue::from_static("http://evil.test"));
        assert_eq!(
            check_security(&evil_origin_headers, &Method::GET, "/ws", &config),
            Err(StatusCode::FORBIDDEN)
        );
    }
}
