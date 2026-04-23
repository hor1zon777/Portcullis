use std::collections::HashMap;
use std::sync::OnceLock;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use rust_embed::RustEmbed;
use serde::Serialize;
use sha2::{Digest, Sha256, Sha384};

use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../sdk/dist/"]
#[prefix = ""]
struct SdkDist;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../sdk/pkg/"]
#[prefix = ""]
struct SdkPkg;

/// 与 captcha-server crate 的版本同步，作为 /sdk/v{version}/... 的版本段。
pub const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");

/// /sdk/manifest.json 暴露的 artifact 白名单。
/// 仅放主站接入必需的三个文件，避免把 sourcemap / 内部模块也暴露给 SRI。
const MANIFEST_ARTIFACTS: &[&str] = &[
    "pow-captcha.js",
    "captcha_wasm.js",
    "captcha_wasm_bg.wasm",
];

/// 编译期嵌入的文件内容不变，哈希只需算一次。
struct AssetMeta {
    /// `"<sha256hex>"`，用于 ETag / If-None-Match。
    etag: String,
    /// `sha384-<base64>`，用于 SRI `integrity=` 属性。
    integrity: String,
    size: usize,
}

static META_CACHE: OnceLock<HashMap<String, AssetMeta>> = OnceLock::new();

fn make_meta(data: &[u8]) -> AssetMeta {
    let sha256 = Sha256::digest(data);
    let sha384 = Sha384::digest(data);
    AssetMeta {
        etag: format!("\"{:x}\"", sha256),
        integrity: format!("sha384-{}", B64.encode(sha384)),
        size: data.len(),
    }
}

fn meta_map() -> &'static HashMap<String, AssetMeta> {
    META_CACHE.get_or_init(|| {
        let mut map: HashMap<String, AssetMeta> = HashMap::new();
        for name in SdkDist::iter() {
            if let Some(f) = SdkDist::get(&name) {
                map.insert(name.to_string(), make_meta(&f.data));
            }
        }
        for name in SdkPkg::iter() {
            if !map.contains_key(name.as_ref()) {
                if let Some(f) = SdkPkg::get(&name) {
                    map.insert(name.to_string(), make_meta(&f.data));
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

fn read_asset(file: &str) -> Option<Vec<u8>> {
    SdkDist::get(file)
        .map(|c| c.data.into_owned())
        .or_else(|| SdkPkg::get(file).map(|c| c.data.into_owned()))
}

fn not_modified(etag: &str, cache_control: &'static str) -> Response {
    Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, cache_control)
        .header(
            "Cross-Origin-Resource-Policy",
            HeaderValue::from_static("cross-origin"),
        )
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| StatusCode::NOT_MODIFIED.into_response())
}

fn try_not_modified(
    headers: &HeaderMap,
    etag: &str,
    cache_control: &'static str,
) -> Option<Response> {
    if etag.is_empty() {
        return None;
    }
    let inm = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())?;
    if inm.contains(etag) {
        Some(not_modified(etag, cache_control))
    } else {
        None
    }
}

fn build_asset_response(file: &str, body: Vec<u8>, cache_control: &'static str) -> Response {
    let etag = meta_map()
        .get(file)
        .map(|m| m.etag.as_str())
        .unwrap_or_default();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, guess_mime(file))
        .header(header::CACHE_CONTROL, cache_control)
        .header(header::ETAG, etag)
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            "Cross-Origin-Resource-Policy",
            HeaderValue::from_static("cross-origin"),
        )
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|e| {
            tracing::error!("response builder 失败: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        })
}

fn serve_asset(headers: &HeaderMap, file: &str, cache_control: &'static str) -> Response {
    let Some(body) = read_asset(file) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let etag = meta_map()
        .get(file)
        .map(|m| m.etag.clone())
        .unwrap_or_default();
    if let Some(nm) = try_not_modified(headers, &etag, cache_control) {
        return nm;
    }
    build_asset_response(file, body, cache_control)
}

// ───────────── Manifest ─────────────

#[derive(Serialize)]
struct ManifestArtifact {
    url: String,
    integrity: String,
    size: usize,
}

#[derive(Serialize)]
struct Manifest {
    version: &'static str,
    #[serde(rename = "builtAt")]
    built_at: u64,
    artifacts: HashMap<&'static str, ManifestArtifact>,
}

fn render_manifest(signing_key: Option<&SigningKey>) -> Response {
    let meta = meta_map();
    let mut artifacts = HashMap::new();
    for name in MANIFEST_ARTIFACTS {
        if let Some(m) = meta.get(*name) {
            artifacts.insert(
                *name,
                ManifestArtifact {
                    url: format!("/sdk/v{}/{}", SDK_VERSION, name),
                    integrity: m.integrity.clone(),
                    size: m.size,
                },
            );
        }
    }

    let built_at: u64 = BUILD_TIMESTAMP.parse().unwrap_or(0);
    let manifest = Manifest {
        version: SDK_VERSION,
        built_at,
        artifacts,
    };

    let body = match serde_json::to_vec(&manifest) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("manifest 序列化失败: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        )
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=300"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            "Cross-Origin-Resource-Policy",
            HeaderValue::from_static("cross-origin"),
        );

    // 配置了 Ed25519 signing key 时签 body 字节，放 response header
    if let Some(sk) = signing_key {
        let sig = sk.sign(&body);
        let sig_b64 = B64.encode(sig.to_bytes());
        match HeaderValue::from_str(&sig_b64) {
            Ok(v) => {
                builder = builder.header("X-Portcullis-Signature", v);
            }
            Err(e) => {
                tracing::error!("X-Portcullis-Signature header 构造失败: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    builder.body(axum::body::Body::from(body)).unwrap_or_else(|e| {
        tracing::error!("manifest response builder 失败: {e}");
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })
}

// ───────────── Route handler ─────────────

const CACHE_LEGACY: &str = "public, max-age=3600";
const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// 统一入口：`GET /sdk/*file`
///
/// - `manifest.json` → 版本 + SRI 清单；配置了签名密钥时带 `X-Portcullis-Signature` 响应头
/// - `v{SDK_VERSION}/<asset>` → 版本化只读路径（immutable 长缓存）
/// - `v{其他版本}/<asset>` → 404（避免旧版本字节被静默返回）
/// - 其它 → 旧路径，向后兼容（短缓存）
pub async fn serve_sdk(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(file): Path<String>,
) -> Response {
    if file == "manifest.json" {
        let cfg = state.config.load();
        let sk = cfg.manifest_signing_key.map(|seed| SigningKey::from_bytes(&seed));
        return render_manifest(sk.as_ref());
    }

    if let Some(rest) = file.strip_prefix('v') {
        if let Some((ver, inner)) = rest.split_once('/') {
            // 只有版本段形如 "1.1.2" 的才按版本路径判定，
            // 防止未来出现名字以 'v' 开头的真实文件被误拒。
            if looks_like_version(ver) {
                if ver == SDK_VERSION {
                    return serve_asset(&headers, inner, CACHE_IMMUTABLE);
                }
                return StatusCode::NOT_FOUND.into_response();
            }
        }
    }

    serve_asset(&headers, &file, CACHE_LEGACY)
}

fn looks_like_version(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
        && s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier};

    #[test]
    fn version_matches_cargo() {
        // 保证 SDK_VERSION 和 Cargo.toml 同步，manifest.url 与版本路径才一致
        assert!(!SDK_VERSION.is_empty());
        assert!(SDK_VERSION.chars().next().unwrap().is_ascii_digit());
    }

    #[test]
    fn looks_like_version_positive() {
        assert!(looks_like_version("1.1.2"));
        assert!(looks_like_version("1.0.0-alpha"));
        assert!(looks_like_version("2.0.0"));
    }

    #[test]
    fn looks_like_version_negative() {
        assert!(!looks_like_version(""));
        assert!(!looks_like_version("abc"));
        assert!(!looks_like_version(".1.2"));
        assert!(!looks_like_version("/foo"));
    }

    #[test]
    fn sri_format() {
        // 人工构造，不依赖 rust-embed（它在集成测试里测）
        let data = b"hello";
        let sha384 = Sha384::digest(data);
        let integrity = format!("sha384-{}", B64.encode(sha384));
        assert!(integrity.starts_with("sha384-"));
        assert!(integrity.len() > "sha384-".len());
    }

    #[test]
    fn ed25519_sign_verify_roundtrip() {
        // 验证 Tier 2 签名协议本身可 roundtrip：
        // 服务端 seed → SigningKey → sign(body) → base64 → (wire)
        // 主站 base64 → Signature → VerifyingKey.verify(body, sig) == Ok
        let seed = [7u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let pk = sk.verifying_key();

        let body = b"{\"version\":\"1.1.2\",\"artifacts\":{}}";
        let sig = sk.sign(body);
        let sig_b64 = B64.encode(sig.to_bytes());

        let sig_bytes = B64.decode(&sig_b64).unwrap();
        let sig_array: [u8; 64] = sig_bytes.as_slice().try_into().unwrap();
        let parsed = Signature::from_bytes(&sig_array);
        assert!(pk.verify(body, &parsed).is_ok());

        // 篡改 body 应验签失败
        let tampered = b"{\"version\":\"1.1.3\",\"artifacts\":{}}";
        assert!(pk.verify(tampered, &parsed).is_err());
    }
}
