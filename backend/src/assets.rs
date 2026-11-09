use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// The production React build, embedded at compile time. CI builds the
/// frontend first; `frontend/dist/index.html` placeholder exists for
/// backend-only development builds.
#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
        return serve(path, file);
    }
    // SPA fallback: unknown extension-less paths route client-side.
    if !path.contains('.') {
        if let Some(index) = Assets::get("index.html") {
            return serve("index.html", index);
        }
    }
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn serve(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // Vite emits content-hashed filenames under assets/ — safe to cache hard.
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        file.data.into_owned(),
    )
        .into_response()
}
