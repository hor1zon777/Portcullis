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

fn build_response(file: &str, body: Vec<u8>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, guess_mime(file))
        // 1 小时缓存。文件未做 content-hash，所以不能用 immutable
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            header::HeaderValue::from_static("*"),
        )
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|e| {
            tracing::error!("response builder 失败: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
}

fn serve_file<E: RustEmbed>(file: &str) -> Option<Response> {
    E::get(file).map(|content| build_response(file, content.data.into_owned()))
}

/// GET /sdk/{*file}
pub async fn serve_sdk(Path(file): Path<String>) -> Response {
    serve_file::<SdkDist>(&file)
        .or_else(|| serve_file::<SdkPkg>(&file))
        .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}
