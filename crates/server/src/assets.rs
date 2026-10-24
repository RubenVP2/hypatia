use axum::{
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../web/dist/"]
pub struct Assets;

/// Axum handler to serve compiled static assets for the WebAssembly frontend.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let target = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(target) {
        let mime_str = if target.ends_with(".wasm") {
            "application/wasm".to_string()
        } else if target.ends_with(".js") {
            "application/javascript".to_string()
        } else if target.ends_with(".css") {
            "text/css".to_string()
        } else if target.ends_with(".html") {
            "text/html; charset=utf-8".to_string()
        } else {
            mime_guess::from_path(target)
                .first_or_octet_stream()
                .to_string()
        };

        let content_type = HeaderValue::try_from(mime_str)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));

        return ([(header::CONTENT_TYPE, content_type)], file.data).into_response();
    }

    // Standard SPA fallback: if the request does not target the API or WebSocket, serve index.html
    if !path.starts_with("api/") && path != "ws" && !path.starts_with("ws/") {
        if let Some(index) = Assets::get("index.html") {
            return (
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/html; charset=utf-8"),
                )],
                index.data,
            )
                .into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_assets_exist() {
        assert!(
            Assets::get("index.html").is_some(),
            "index.html must be embedded"
        );
    }
}
