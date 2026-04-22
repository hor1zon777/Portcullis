use std::collections::HashMap;
use std::sync::OnceLock;

use axum::extract::Path;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../sdk/dist/"]
#[prefix = ""]
struct SdkDist;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../sdk/pkg/"]
#[prefix = ""]
struct SdkPkg;

/// 编译期嵌入的文件内容不变，ETag 只需算一次。
static ETAG_CACHE: OnceLock<HashMap<String, String>> = OnceLock::new();

fn get_etag_map() -> &'static HashMap<String, String> {
    ETAG_CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        for name in SdkDist::iter() {
            if let Some(f) = SdkDist::get(&name) {
                let hash = Sha256::digest(&f.data);
                map.insert(name.to_string(), format!("\"{:x}\"", hash));
            }
        }
        for name in SdkPkg::iter() {
            if !map.contains_key(name.as_ref()) {
                if let Some(f) = SdkPkg::get(&name) {
                    let hash = Sha256::digest(&f.data);
                    map.insert(name.to_string(), format!("\"{:x}\"", hash));
                }
            }
        }
        map
    })
}

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
    let etag = get_etag_map()
        .get(file)
        .cloned()
        .unwrap_or_default();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, guess_mime(file))
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .header(header::ETAG, &etag)
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

fn serve_file<E: RustEmbed>(file: &str) -> Option<(String, Vec<u8>)> {
    E::get(file).map(|content| {
        let etag = get_etag_map()
            .get(file)
            .cloned()
            .unwrap_or_default();
        (etag, content.data.into_owned())
    })
}

/// GET /sdk/{*file}
/// 支持 ETag + If-None-Match → 304 Not Modified。
pub async fn serve_sdk(headers: HeaderMap, Path(file): Path<String>) -> Response {
    let found = serve_file::<SdkDist>(&file).or_else(|| serve_file::<SdkPkg>(&file));

    let Some((etag, body)) = found else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // 304 如果 ETag 匹配
    if let Some(inm) = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) {
        if !etag.is_empty() && inm.contains(&etag) {
            return Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, &etag)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(axum::body::Body::empty())
                .unwrap_or_else(|_| StatusCode::NOT_MODIFIED.into_response());
        }
    }

    build_response(&file, body)
}
