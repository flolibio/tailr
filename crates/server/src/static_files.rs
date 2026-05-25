use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use include_dir::{include_dir, Dir};

static FRONTEND_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../frontend/dist");

pub fn routes() -> Router {
    Router::new().fallback(get(serve_frontend))
}

async fn serve_frontend(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the file from embedded dist
    if !path.is_empty() {
        if let Some(file) = FRONTEND_DIR.get_file(path) {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            return (
                [(header::CONTENT_TYPE, mime)],
                file.contents().to_vec(),
            )
                .into_response();
        }
    }

    // SPA fallback: serve index.html for any unmatched route
    if let Some(index) = FRONTEND_DIR.get_file("index.html") {
        return Html(String::from_utf8_lossy(index.contents()).to_string()).into_response();
    }

    // If no frontend was built, show a helpful message
    (
        StatusCode::OK,
        Html(
            r#"<!DOCTYPE html>
<html><head><title>Logtailer</title></head>
<body style="font-family:monospace;background:#1e1e1e;color:#d4d4d4;padding:40px">
<h1>Logtailer</h1>
<p>Frontend not built. Run <code>cd frontend && npm run build</code> first.</p>
</body></html>"#
                .to_string(),
        ),
    )
        .into_response()
}
