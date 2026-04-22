use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../sdk/dist/"]
#[prefix = ""]
struct SdkDist;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../sdk/pkg/"]
#[prefix = ""]
struct SdkPkg;

fn guess_mime(file: &str) -> &'static str {
    match file.rsplit('.').next() {
        Some("js") => "application/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("map") => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn serve_file<E: RustEmbed>(file: &str) -> Option<Response> {
    E::get(file).map(|content| {
        let body = content.data.into_owned();
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, guess_mime(file))
            .header(header::CACHE_CONTROL, "public, max-age=3600, immutable")
            .header(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                header::HeaderValue::from_static("*"),
            )
            .body(axum::body::Body::from(body))
            .unwrap()
    })
}

/// GET /sdk/{*file}
pub async fn serve_sdk(Path(file): Path<String>) -> Response {
    serve_file::<SdkDist>(&file)
        .or_else(|| serve_file::<SdkPkg>(&file))
        .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}
