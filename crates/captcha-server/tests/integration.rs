use std::collections::HashMap;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use captcha_core::{challenge::Challenge, pow};
use captcha_server::{
    build_router,
    config::{Config, SiteConfig},
    state::AppState,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use tower::ServiceExt;

fn test_config() -> Config {
    let mut sites = HashMap::new();
    sites.insert(
        "pk_test".to_string(),
        SiteConfig {
            secret_key: "sk_test_secret_at_least_16_bytes".to_string(),
            diff: 8,
            origins: vec![],
            argon2_m_cost: captcha_core::challenge::LEGACY_M_COST,
            argon2_t_cost: captcha_core::challenge::LEGACY_T_COST,
            argon2_p_cost: captcha_core::challenge::LEGACY_P_COST,
            bind_token_to_ip: false,
            bind_token_to_ua: false,
            secret_key_hashed: false,
        },
    );

    Config {
        secret: b"test-secret-key-must-be-at-least-32-bytes!!!".to_vec(),
        secret_previous: None,
        bind: "127.0.0.1:0".to_string(),
        sites,
        token_ttl_secs: 300,
        challenge_ttl_secs: 120,
        risk: Default::default(),
        admin_token: None,
        db_path: std::path::PathBuf::from(":memory:"),
        config_path: None,
        manifest_signing_key: None,
        admin_webhook_url: None,
    }
}

async fn post_json<T: DeserializeOwned>(
    app: &axum::Router,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, T) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let parsed: T = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "解析响应失败: {e}, body={}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, parsed)
}

async fn post_raw(app: &axum::Router, path: &str, body: serde_json::Value) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap().status()
}

async fn get_full(app: &axum::Router, path: &str) -> axum::http::Response<Body> {
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(req).await.unwrap()
}

#[derive(serde::Deserialize, Debug)]
struct ChallengeResp {
    success: bool,
    challenge: Challenge,
    sig: String,
}

#[derive(serde::Deserialize, Debug)]
struct VerifyResp {
    success: bool,
    captcha_token: String,
    #[allow(dead_code)]
    exp: u64,
}

#[derive(serde::Deserialize, Debug)]
struct SiteVerifyResp {
    success: bool,
    site_key: Option<String>,
    #[allow(dead_code)]
    challenge_id: Option<String>,
    #[allow(dead_code)]
    error: Option<String>,
}

#[tokio::test]
async fn e2e_happy_path() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (status, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ch.success);
    assert_eq!(ch.challenge.diff, 8);

    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");

    let (status, v): (_, VerifyResp) = post_json(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(v.success);
    assert!(!v.captcha_token.is_empty());

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({ "token": v.captcha_token, "secret_key": "sk_test_secret_at_least_16_bytes" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(sv.success);
    assert_eq!(sv.site_key.as_deref(), Some("pk_test"));
}

#[tokio::test]
async fn replay_rejected() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).unwrap();

    let body = json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce });
    assert_eq!(
        post_raw(&app, "/api/v1/verify", body.clone()).await,
        StatusCode::OK
    );
    assert_eq!(
        post_raw(&app, "/api/v1/verify", body).await,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn bad_sig_rejected() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;

    let bad_sig = B64.encode([0u8; 32]);
    let status = post_raw(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": bad_sig, "nonce": 0 }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_site_rejected() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );
    let status = post_raw(
        &app,
        "/api/v1/challenge",
        json!({ "site_key": "pk_unknown" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn wrong_nonce_rejected() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;

    // nonce=0 极不可能满足 diff=8（1/256 概率），大部分时候会被拒
    let mut rejected = false;
    for candidate in 0..4u64 {
        if !pow::verify_solution(&ch.challenge, candidate) {
            let status = post_raw(
                &app,
                "/api/v1/verify",
                json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": candidate }),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
            rejected = true;
            break;
        }
    }
    assert!(rejected, "前 4 个 nonce 应至少有一个不满足 diff=8");
}

#[tokio::test]
async fn siteverify_wrong_secret_key() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).unwrap();
    let (_, v): (_, VerifyResp) = post_json(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
    )
    .await;

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({ "token": v.captcha_token, "secret_key": "wrong_secret" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!sv.success);
}

#[tokio::test]
async fn healthz() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );
    let req = Request::builder()
        .method("GET")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// ───────────── SDK 加固 Tier 1 ─────────────

fn router() -> axum::Router {
    build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    )
}

/// 若 SDK 构建产物缺失，rust-embed 会内嵌空集合，/sdk 相关测试不具备有效前提，跳过。
fn sdk_assets_available() -> bool {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk/dist/pow-captcha.js")
        .exists()
}

#[tokio::test]
async fn sdk_manifest_json() {
    if !sdk_assets_available() {
        eprintln!("sdk/dist/pow-captcha.js 不存在，跳过");
        return;
    }
    let app = router();
    let res = get_full(&app, "/sdk/manifest.json").await;
    assert_eq!(res.status(), StatusCode::OK);

    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.starts_with("application/json"), "content-type={ct}");

    let corp = res
        .headers()
        .get("cross-origin-resource-policy")
        .expect("CORP 头缺失");
    assert_eq!(corp.to_str().unwrap(), "cross-origin");

    let cache = res
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cache.contains("max-age=300"), "cache-control={cache}");

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(v["version"].as_str().unwrap(), env!("CARGO_PKG_VERSION"));
    assert!(v["builtAt"].is_number());

    let art = v["artifacts"].as_object().expect("artifacts 缺失");
    assert!(art.contains_key("pow-captcha.js"));

    let js = &art["pow-captcha.js"];
    let integrity = js["integrity"].as_str().unwrap();
    assert!(
        integrity.starts_with("sha384-"),
        "integrity 格式错: {integrity}"
    );
    assert!(js["size"].as_u64().unwrap() > 0);
    assert_eq!(
        js["url"].as_str().unwrap(),
        format!("/sdk/v{}/pow-captcha.js", env!("CARGO_PKG_VERSION"))
    );
}

#[tokio::test]
async fn sdk_versioned_path_current_version() {
    if !sdk_assets_available() {
        eprintln!("sdk/dist/pow-captcha.js 不存在，跳过");
        return;
    }
    let app = router();
    let path = format!("/sdk/v{}/pow-captcha.js", env!("CARGO_PKG_VERSION"));
    let res = get_full(&app, &path).await;
    assert_eq!(res.status(), StatusCode::OK);

    let cache = res
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cache.contains("immutable"), "cache-control={cache}");
    assert!(cache.contains("31536000"), "cache-control={cache}");

    let corp = res
        .headers()
        .get("cross-origin-resource-policy")
        .expect("CORP 头缺失");
    assert_eq!(corp.to_str().unwrap(), "cross-origin");

    let etag = res.headers().get("etag");
    assert!(etag.is_some(), "ETag 缺失");
}

#[tokio::test]
async fn sdk_versioned_path_unknown_version_404() {
    let app = router();
    let res = get_full(&app, "/sdk/v99.99.99/pow-captcha.js").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sdk_legacy_path_backward_compatible() {
    if !sdk_assets_available() {
        eprintln!("sdk/dist/pow-captcha.js 不存在，跳过");
        return;
    }
    let app = router();
    let res = get_full(&app, "/sdk/pow-captcha.js").await;
    assert_eq!(res.status(), StatusCode::OK);

    let cache = res
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cache.contains("max-age=3600"), "cache-control={cache}");
    assert!(
        !cache.contains("immutable"),
        "旧路径不应使用 immutable: {cache}"
    );
}

#[tokio::test]
async fn sdk_unknown_file_404() {
    let app = router();
    let res = get_full(&app, "/sdk/does-not-exist.js").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ───────────── SDK 加固 Tier 2（Ed25519 签名 manifest） ─────────────

fn test_config_with_signing_key(seed: [u8; 32]) -> Config {
    let mut cfg = test_config();
    cfg.manifest_signing_key = Some(seed);
    cfg
}

#[tokio::test]
async fn manifest_unsigned_when_key_absent() {
    if !sdk_assets_available() {
        eprintln!("sdk/dist/pow-captcha.js 不存在，跳过");
        return;
    }
    let app = router();
    let res = get_full(&app, "/sdk/manifest.json").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.headers().get("x-portcullis-signature").is_none(),
        "未配置 signing key 时不应发出签名 header"
    );
}

#[tokio::test]
async fn manifest_signed_verifies_with_pubkey() {
    if !sdk_assets_available() {
        eprintln!("sdk/dist/pow-captcha.js 不存在，跳过");
        return;
    }
    use ed25519_dalek::{Signature, SigningKey, Verifier};

    let seed = [0x5au8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let expected_pk = sk.verifying_key();

    let app = build_router(
        AppState::new(
            test_config_with_signing_key(seed),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let res = get_full(&app, "/sdk/manifest.json").await;
    assert_eq!(res.status(), StatusCode::OK);

    let sig_b64 = res
        .headers()
        .get("x-portcullis-signature")
        .expect("配置 signing key 时必须有签名 header")
        .to_str()
        .unwrap()
        .to_string();

    let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();

    let sig_bytes = B64.decode(&sig_b64).expect("签名 base64 解码失败");
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .expect("Ed25519 签名应是 64 字节");
    let signature = Signature::from_bytes(&sig_arr);

    expected_pk
        .verify(&body, &signature)
        .expect("公钥验签应成功");

    // 篡改 body 应导致验签失败
    let mut tampered = body.to_vec();
    tampered[0] ^= 0x01;
    assert!(expected_pk.verify(&tampered, &signature).is_err());
}

#[tokio::test]
async fn admin_manifest_pubkey_disabled() {
    let mut cfg = test_config();
    cfg.admin_token = Some("test-admin-token".to_string());
    let app = build_router(
        AppState::new(cfg, captcha_server::db::open_memory()),
        None,
        None,
    );

    let req = Request::builder()
        .method("GET")
        .uri("/admin/api/manifest-pubkey")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["enabled"], false);
    assert_eq!(v["algorithm"], "ed25519");
    assert!(v.get("pubkey").map(|p| p.is_null()).unwrap_or(true));
}

#[tokio::test]
async fn admin_manifest_pubkey_enabled_returns_matching_key() {
    use ed25519_dalek::SigningKey;

    let seed = [0xa5u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let expected_b64 = B64.encode(sk.verifying_key().to_bytes());

    let mut cfg = test_config_with_signing_key(seed);
    cfg.admin_token = Some("test-admin-token".to_string());
    let app = build_router(
        AppState::new(cfg, captcha_server::db::open_memory()),
        None,
        None,
    );

    let req = Request::builder()
        .method("GET")
        .uri("/admin/api/manifest-pubkey")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["enabled"], true);
    assert_eq!(v["algorithm"], "ed25519");
    assert_eq!(v["pubkey"].as_str().unwrap(), expected_b64);
}

#[tokio::test]
async fn admin_manifest_pubkey_requires_auth() {
    let mut cfg = test_config();
    cfg.admin_token = Some("test-admin-token".to_string());
    let app = build_router(
        AppState::new(cfg, captcha_server::db::open_memory()),
        None,
        None,
    );

    let req = Request::builder()
        .method("GET")
        .uri("/admin/api/manifest-pubkey")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_generate_manifest_key_from_empty() {
    use ed25519_dalek::{Signature, SigningKey, Verifier};

    let mut cfg = test_config();
    cfg.admin_token = Some("test-admin-token".to_string());
    // 初始无 signing key
    assert!(cfg.manifest_signing_key.is_none());

    let db = captcha_server::db::open_memory();
    let state = AppState::new(cfg, db.clone());
    let app = build_router(state.clone(), None, None);

    // 1. POST /generate
    let gen_req = Request::builder()
        .method("POST")
        .uri("/admin/api/manifest-pubkey/generate")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let gen_res = app.clone().oneshot(gen_req).await.unwrap();
    assert_eq!(gen_res.status(), StatusCode::OK);

    let bytes = to_bytes(gen_res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["enabled"], true);
    assert_eq!(v["first_time"], true);
    let pubkey_b64 = v["pubkey"].as_str().unwrap().to_string();

    // 2. DB 持久化
    let seed_from_db =
        captcha_server::db::load_server_secret_32(&db, "manifest_signing_key").unwrap();
    let expected_pk_b64 = B64.encode(
        SigningKey::from_bytes(&seed_from_db)
            .verifying_key()
            .to_bytes(),
    );
    assert_eq!(pubkey_b64, expected_pk_b64);

    // 3. ArcSwap 配置已更新：GET 应返回相同公钥
    let get_req = Request::builder()
        .method("GET")
        .uri("/admin/api/manifest-pubkey")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let get_res = app.clone().oneshot(get_req).await.unwrap();
    let bytes = to_bytes(get_res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["enabled"], true);
    assert_eq!(v["pubkey"].as_str().unwrap(), pubkey_b64);

    // 4. /sdk/manifest.json 立即带上签名，且签名能被返回的公钥验证
    if sdk_assets_available() {
        let manifest_res = get_full(&app, "/sdk/manifest.json").await;
        assert_eq!(manifest_res.status(), StatusCode::OK);
        let sig_b64 = manifest_res
            .headers()
            .get("x-portcullis-signature")
            .expect("生成密钥后 manifest 应带签名 header")
            .to_str()
            .unwrap()
            .to_string();
        let body = to_bytes(manifest_res.into_body(), usize::MAX)
            .await
            .unwrap();

        let sig_bytes = B64.decode(&sig_b64).unwrap();
        let sig_arr: [u8; 64] = sig_bytes.as_slice().try_into().unwrap();
        let sig = Signature::from_bytes(&sig_arr);

        let pk_bytes = B64.decode(&pubkey_b64).unwrap();
        let pk_arr: [u8; 32] = pk_bytes.as_slice().try_into().unwrap();
        let pk = ed25519_dalek::VerifyingKey::from_bytes(&pk_arr).unwrap();
        pk.verify(&body, &sig).expect("新生成的公钥应能验签");
    }
}

#[tokio::test]
async fn admin_generate_manifest_key_overwrite() {
    let mut cfg = test_config();
    cfg.admin_token = Some("test-admin-token".to_string());
    cfg.manifest_signing_key = Some([0x11u8; 32]);

    let db = captcha_server::db::open_memory();
    let state = AppState::new(cfg, db.clone()); // migrate 先跑
                                                // 种一个"已有 seed"状态（模拟 env/toml seed 到 DB 之后）
    captcha_server::db::save_server_secret_32(&db, "manifest_signing_key", &[0x11u8; 32]);

    let app = build_router(state, None, None);

    let req = Request::builder()
        .method("POST")
        .uri("/admin/api/manifest-pubkey/generate")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["first_time"], false);

    // DB 里的 seed 已被替换（极大概率不是 [0x11;32]）
    let seed = captcha_server::db::load_server_secret_32(&db, "manifest_signing_key").unwrap();
    assert_ne!(seed, [0x11u8; 32]);
}

#[tokio::test]
async fn admin_revoke_manifest_key() {
    let mut cfg = test_config();
    cfg.admin_token = Some("test-admin-token".to_string());
    cfg.manifest_signing_key = Some([0x22u8; 32]);

    let db = captcha_server::db::open_memory();
    let state = AppState::new(cfg, db.clone()); // migrate 先跑
    captcha_server::db::save_server_secret_32(&db, "manifest_signing_key", &[0x22u8; 32]);

    let app = build_router(state, None, None);

    // 撤销
    let req = Request::builder()
        .method("DELETE")
        .uri("/admin/api/manifest-pubkey")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["removed"], true);

    // DB 清空
    assert!(captcha_server::db::load_server_secret_32(&db, "manifest_signing_key").is_none());

    // 状态回到 enabled=false
    let get_req = Request::builder()
        .method("GET")
        .uri("/admin/api/manifest-pubkey")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let get_res = app.clone().oneshot(get_req).await.unwrap();
    let bytes = to_bytes(get_res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["enabled"], false);

    // 再次撤销应是幂等 ok:true removed:false
    let req2 = Request::builder()
        .method("DELETE")
        .uri("/admin/api/manifest-pubkey")
        .header("authorization", "Bearer test-admin-token")
        .body(Body::empty())
        .unwrap();
    let res2 = app.oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let bytes = to_bytes(res2.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["removed"], false);
}

// ───────────── v1.3.0 PoW 参数下发化 ─────────────

#[tokio::test]
async fn challenge_contains_argon2_params() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (status, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ch.success);

    // test_config 使用 LEGACY 参数 4096/1/1
    assert_eq!(ch.challenge.m_cost, 4096);
    assert_eq!(ch.challenge.t_cost, 1);
    assert_eq!(ch.challenge.p_cost, 1);
}

#[tokio::test]
async fn challenge_params_covered_by_signature() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;

    // 篡改 m_cost 后提交 verify，签名应失败
    let mut tampered = ch.challenge.clone();
    tampered.m_cost = 65536;

    let status = post_raw(
        &app,
        "/api/v1/verify",
        json!({ "challenge": tampered, "sig": ch.sig, "nonce": 0 }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "篡改 m_cost 应导致签名验证失败"
    );
}

#[tokio::test]
async fn challenge_tampered_t_cost_rejected() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;

    let mut tampered = ch.challenge.clone();
    tampered.t_cost = 10;

    let status = post_raw(
        &app,
        "/api/v1/verify",
        json!({ "challenge": tampered, "sig": ch.sig, "nonce": 0 }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "篡改 t_cost 应导致签名验证失败"
    );
}

fn test_config_with_custom_argon2() -> Config {
    let mut cfg = test_config();
    // pk_test 使用自定义参数
    if let Some(site) = cfg.sites.get_mut("pk_test") {
        site.argon2_m_cost = 8192;
        site.argon2_t_cost = 3;
        site.argon2_p_cost = 1;
    }
    // 添加第二个站点使用不同参数
    cfg.sites.insert(
        "pk_site2".to_string(),
        SiteConfig {
            secret_key: "sk_site2_secret_key_16b".to_string(),
            diff: 8,
            origins: vec![],
            argon2_m_cost: 32768,
            argon2_t_cost: 2,
            argon2_p_cost: 1,
            bind_token_to_ip: false,
            bind_token_to_ua: false,
            secret_key_hashed: false,
        },
    );
    cfg
}

#[tokio::test]
async fn different_sites_different_argon2_params() {
    let cfg = test_config_with_custom_argon2();
    let app = build_router(
        AppState::new(cfg, captcha_server::db::open_memory()),
        None,
        None,
    );

    // pk_test 应返回 8192/3/1
    let (_, ch1): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    assert_eq!(ch1.challenge.m_cost, 8192);
    assert_eq!(ch1.challenge.t_cost, 3);
    assert_eq!(ch1.challenge.p_cost, 1);

    // pk_site2 应返回 32768/2/1
    let (_, ch2): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_site2" })).await;
    assert_eq!(ch2.challenge.m_cost, 32768);
    assert_eq!(ch2.challenge.t_cost, 2);
    assert_eq!(ch2.challenge.p_cost, 1);
}

#[tokio::test]
async fn e2e_with_custom_argon2_params() {
    let cfg = test_config_with_custom_argon2();
    let app = build_router(
        AppState::new(cfg, captcha_server::db::open_memory()),
        None,
        None,
    );

    let (status, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ch.challenge.m_cost, 8192);
    assert_eq!(ch.challenge.t_cost, 3);

    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");

    let (status, v): (_, VerifyResp) = post_json(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(v.success);
    assert!(!v.captcha_token.is_empty());

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({ "token": v.captcha_token, "secret_key": "sk_test_secret_at_least_16_bytes" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(sv.success);
}

#[tokio::test]
async fn legacy_json_fallback_default_params() {
    // 模拟旧客户端发来不含 m/t/p 的 challenge JSON
    let legacy_json = r#"{
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "salt": "AQIDBAUGBwgJCgsMDQ4PEA==",
        "diff": 8,
        "exp": 99999999999999,
        "site_key": "pk_test"
    }"#;

    let ch: captcha_core::challenge::Challenge = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(ch.m_cost, captcha_core::challenge::LEGACY_M_COST);
    assert_eq!(ch.t_cost, captcha_core::challenge::LEGACY_T_COST);
    assert_eq!(ch.p_cost, captcha_core::challenge::LEGACY_P_COST);
}

// ───────────── v1.4.0 CaptchaToken 客户端身份绑定 ─────────────

async fn post_json_hdr<T: DeserializeOwned>(
    app: &axum::Router,
    path: &str,
    body: serde_json::Value,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, T) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = builder
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let parsed: T = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "解析响应失败: {e}, body={}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, parsed)
}

fn test_config_with_ip_binding() -> Config {
    let mut cfg = test_config();
    if let Some(site) = cfg.sites.get_mut("pk_test") {
        site.bind_token_to_ip = true;
    }
    cfg
}

fn test_config_with_ua_binding() -> Config {
    let mut cfg = test_config();
    if let Some(site) = cfg.sites.get_mut("pk_test") {
        site.bind_token_to_ua = true;
    }
    cfg
}

fn test_config_with_both_bindings() -> Config {
    let mut cfg = test_config();
    if let Some(site) = cfg.sites.get_mut("pk_test") {
        site.bind_token_to_ip = true;
        site.bind_token_to_ua = true;
    }
    cfg
}

/// E2E：IP 绑定开启 → /verify 带 XFF → /siteverify 带同 IP → 通过
#[tokio::test]
async fn e2e_ip_binding_matches() {
    let app = build_router(
        AppState::new(
            test_config_with_ip_binding(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) = post_json_hdr(
        &app,
        "/api/v1/challenge",
        json!({ "site_key": "pk_test" }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;

    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");

    let (status, v): (_, VerifyResp) = post_json_hdr(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
            "client_ip": "203.0.113.5",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(sv.success);
}

/// IP 绑定开启 → 发放时来自 A IP → /siteverify 报 B IP → 拒绝
#[tokio::test]
async fn ip_binding_mismatch_rejected() {
    let app = build_router(
        AppState::new(
            test_config_with_ip_binding(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) = post_json_hdr(
        &app,
        "/api/v1/challenge",
        json!({ "site_key": "pk_test" }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");
    let (_, v): (_, VerifyResp) = post_json_hdr(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
            "client_ip": "198.51.100.9",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!sv.success, "IP 不匹配应当被拒绝");
    assert!(sv.error.is_some());
}

/// IP 绑定开启 → /siteverify 未提供 client_ip → 拒绝
#[tokio::test]
async fn ip_binding_missing_client_ip_rejected() {
    let app = build_router(
        AppState::new(
            test_config_with_ip_binding(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) = post_json_hdr(
        &app,
        "/api/v1/challenge",
        json!({ "site_key": "pk_test" }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");
    let (_, v): (_, VerifyResp) = post_json_hdr(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!sv.success);
    assert!(sv.error.as_deref().unwrap_or("").contains("IP 绑定"));
}

/// IP 绑定开启 → /verify 无法识别 IP → 400
#[tokio::test]
async fn ip_binding_missing_ip_at_verify_rejected() {
    let app = build_router(
        AppState::new(
            test_config_with_ip_binding(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    // /challenge 不强制要求 IP，但 /verify 需要；用不带 XFF 的请求
    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");

    let status = post_raw(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
    )
    .await;
    // 本测试在 oneshot 模式下 ConnectInfo 不可用，也无 XFF，应返回 400
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// E2E：UA 绑定开启 → /verify 带 UA → /siteverify 带同 UA → 通过
#[tokio::test]
async fn e2e_ua_binding_matches() {
    let app = build_router(
        AppState::new(
            test_config_with_ua_binding(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let ua = "Mozilla/5.0 (X11; Linux x86_64) TestUA/1.0";
    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");

    let (_, v): (_, VerifyResp) = post_json_hdr(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
        &[("user-agent", ua)],
    )
    .await;

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
            "user_agent": ua,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(sv.success);
}

/// UA 绑定开启 → siteverify 缺失 user_agent → 拒绝
#[tokio::test]
async fn ua_binding_missing_rejected() {
    let app = build_router(
        AppState::new(
            test_config_with_ua_binding(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let ua = "Mozilla/5.0 TestUA";
    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");
    let (_, v): (_, VerifyResp) = post_json_hdr(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
        &[("user-agent", ua)],
    )
    .await;

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!sv.success);
    assert!(sv.error.as_deref().unwrap_or("").contains("UA 绑定"));
}

/// IP + UA 双绑定 → 全部匹配 → 通过
#[tokio::test]
async fn e2e_ip_and_ua_binding_both_match() {
    let app = build_router(
        AppState::new(
            test_config_with_both_bindings(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let ua = "Mozilla/5.0 DualBind";
    let (_, ch): (_, ChallengeResp) = post_json_hdr(
        &app,
        "/api/v1/challenge",
        json!({ "site_key": "pk_test" }),
        &[("x-forwarded-for", "10.0.0.1"), ("user-agent", ua)],
    )
    .await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");
    let (_, v): (_, VerifyResp) = post_json_hdr(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
        &[("x-forwarded-for", "10.0.0.1"), ("user-agent", ua)],
    )
    .await;

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
            "client_ip": "10.0.0.1",
            "user_agent": ua,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(sv.success);
}

/// 未启用绑定的 site（默认）→ token 不含 hash → siteverify 传入 client_ip/user_agent 被忽略
#[tokio::test]
async fn no_binding_extra_fields_ignored() {
    let app = build_router(
        AppState::new(test_config(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");
    let (_, v): (_, VerifyResp) = post_json(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
    )
    .await;

    // 传入乱七八糟的 client_ip / user_agent，应当被忽略
    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
            "client_ip": "1.2.3.4",
            "user_agent": "Totally-Different-UA",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        sv.success,
        "未绑定的 site 不应该因 client_ip/UA 不一致而失败"
    );
}

/// 非法 client_ip 字符串 → 绑定的 site 应当返回失败并给出明确 error
#[tokio::test]
async fn ip_binding_invalid_client_ip_rejected() {
    let app = build_router(
        AppState::new(
            test_config_with_ip_binding(),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) = post_json_hdr(
        &app,
        "/api/v1/challenge",
        json!({ "site_key": "pk_test" }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");
    let (_, v): (_, VerifyResp) = post_json_hdr(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
        &[("x-forwarded-for", "203.0.113.5")],
    )
    .await;

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
            "client_ip": "not-an-ip",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!sv.success);
    assert!(sv.error.as_deref().unwrap_or("").contains("client_ip"));
}

// ───────────── v1.5.0 服务端密钥与审计硬化 ─────────────

/// 构造 v1.5.0 风格 config：secret_key 已 HMAC 化，模拟启动迁移后的状态。
fn test_config_v1_5_hashed() -> Config {
    let mut cfg = test_config();
    let master = cfg.secret.clone();
    let site = cfg.sites.get_mut("pk_test").unwrap();
    // 在测试里手动做 hash，模拟 Config::load 的启动迁移
    site.secret_key =
        captcha_server::site_secret::hash("sk_test_secret_at_least_16_bytes", &master);
    site.secret_key_hashed = true;
    cfg
}

#[tokio::test]
async fn siteverify_accepts_hashed_secret_key() {
    let app = build_router(
        AppState::new(test_config_v1_5_hashed(), captcha_server::db::open_memory()),
        None,
        None,
    );

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).expect("solve 失败");
    let (_, v): (_, VerifyResp) = post_json(
        &app,
        "/api/v1/verify",
        json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce }),
    )
    .await;

    // 业务后端持有明文 secret_key，调 siteverify 应成功（服务端 HMAC 再比对）
    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": v.captcha_token,
            "secret_key": "sk_test_secret_at_least_16_bytes",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(sv.success, "hash 存储 + 明文 siteverify 应通过");

    // 篡改明文 secret_key 应被拒
    let (status, sv2): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": "dummy.dummy",
            "secret_key": "wrong-secret",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!sv2.success);
}

#[tokio::test]
async fn dual_key_rotation_accepts_previous_signed_token() {
    // 手工流程：
    // 1. 轮换 CAPTCHA_SECRET: 把原始 master 作为 previous（保留 stored_hash 的有效性），
    //    生成新 current
    // 2. 用 previous 直接签 token（模拟轮换前发出、尚未过期）
    // 3. siteverify 应当接受
    let mut cfg = test_config_v1_5_hashed();
    let original_master = cfg.secret.clone();
    let new_current: Vec<u8> = b"new-current-secret-rotation-at-least-32-bytes-long!".to_vec();
    cfg.secret = new_current;
    cfg.secret_previous = Some(original_master.clone());

    let app = build_router(
        AppState::new(cfg, captcha_server::db::open_memory()),
        None,
        None,
    );

    // 手工用 previous 签发 token
    let (token_str, _exp) = captcha_server::token::generate(
        "manual-cid-1",
        "pk_test",
        300,
        &original_master,
        None,
        None,
    );

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": token_str,
            "secret_key": "sk_test_secret_at_least_16_bytes",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(sv.success, "轮换后 previous 签的 token 仍应被接受");
}

#[tokio::test]
async fn dual_key_rotation_rejects_unknown_secret() {
    let mut cfg = test_config_v1_5_hashed();
    let original_master = cfg.secret.clone();
    let new_current: Vec<u8> = b"new-current-secret-rotation-at-least-32-bytes-long!".to_vec();
    cfg.secret = new_current;
    cfg.secret_previous = Some(original_master);

    let app = build_router(
        AppState::new(cfg, captcha_server::db::open_memory()),
        None,
        None,
    );

    // 用完全无关的第三方 key 签 token
    let alien: &[u8] = b"alien-secret-key-not-in-rotation-at-least-32-bytes!";
    let (token_str, _) =
        captcha_server::token::generate("alien-cid-1", "pk_test", 300, alien, None, None);

    let (status, sv): (_, SiteVerifyResp) = post_json(
        &app,
        "/api/v1/siteverify",
        json!({
            "token": token_str,
            "secret_key": "sk_test_secret_at_least_16_bytes",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !sv.success,
        "既非 current 也非 previous 签的 token 必须拒绝"
    );
}

#[tokio::test]
async fn v1_5_site_secret_migration_db_level() {
    let db = captcha_server::db::open_memory();
    captcha_server::db::migrate(&db);

    // 直接插入一条明文 secret_key（模拟 v1.4.x 遗留行）
    let mut plaintext_site = captcha_server::config::SiteConfig {
        secret_key: "sk_legacy_plain_1234".to_string(),
        diff: 18,
        origins: vec![],
        argon2_m_cost: 19456,
        argon2_t_cost: 2,
        argon2_p_cost: 1,
        bind_token_to_ip: false,
        bind_token_to_ua: false,
        secret_key_hashed: false,
    };
    captcha_server::db::insert_site(&db, "pk_legacy", &plaintext_site);

    // 触发 v1.5.0 迁移
    let master = b"master-secret-for-migration-32-bytes!";
    captcha_server::db::migrate_site_secret_keys(&db, master);

    // 重新加载
    let loaded = captcha_server::db::load_sites(&db);
    let row = loaded.get("pk_legacy").expect("站点应存在");
    assert!(row.secret_key_hashed, "迁移后应标记为 hashed");
    assert_ne!(row.secret_key, "sk_legacy_plain_1234", "明文已被覆盖");

    let expected = captcha_server::site_secret::hash("sk_legacy_plain_1234", master);
    assert_eq!(row.secret_key, expected, "存储的应为 HMAC 的 base64");

    // 幂等性：再次迁移不应改变
    plaintext_site.secret_key = row.secret_key.clone();
    plaintext_site.secret_key_hashed = true;
    captcha_server::db::migrate_site_secret_keys(&db, master);
    let loaded2 = captcha_server::db::load_sites(&db);
    assert_eq!(loaded2["pk_legacy"].secret_key, row.secret_key);
}

fn test_config_with_admin(admin_token: &str) -> Config {
    let mut cfg = test_config_v1_5_hashed();
    cfg.admin_token = Some(admin_token.to_string());
    cfg
}

async fn post_json_auth<T: DeserializeOwned>(
    app: &axum::Router,
    path: &str,
    body: serde_json::Value,
    token: &str,
) -> (StatusCode, T) {
    post_json_hdr(
        app,
        path,
        body,
        &[("authorization", &format!("Bearer {token}"))],
    )
    .await
}

async fn get_json_auth<T: DeserializeOwned>(
    app: &axum::Router,
    path: &str,
    token: &str,
) -> (StatusCode, T) {
    let req = Request::builder()
        .method("GET")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let parsed: T = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "解析响应失败: {e}, body={}",
            String::from_utf8_lossy(&bytes)
        )
    });
    (status, parsed)
}

#[derive(serde::Deserialize, Debug)]
struct AuditEntryView {
    #[allow(dead_code)]
    id: i64,
    action: String,
    target: Option<String>,
    #[allow(dead_code)]
    ip: Option<String>,
    success: bool,
    #[allow(dead_code)]
    meta_json: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct AuditListResp {
    total: i64,
    entries: Vec<AuditEntryView>,
}

#[tokio::test]
async fn audit_list_records_site_create() {
    let token = "admin-token-sample-at-least-16-chars";
    let app = build_router(
        AppState::new(
            test_config_with_admin(token),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    // 通过 admin API 创建一个新站点
    let (status, _): (_, serde_json::Value) = post_json_auth(
        &app,
        "/admin/api/sites",
        json!({"diff": 10, "origins": []}),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // audit endpoint 应当至少记录一条 site.create
    // 由于写入是 spawn_blocking，需要一点时间；简单等一下
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (status, audit_resp): (_, AuditListResp) =
        get_json_auth(&app, "/admin/api/audit?limit=50", token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(audit_resp.total >= 1);
    let create_entry = audit_resp
        .entries
        .iter()
        .find(|e| e.action == "site.create")
        .expect("应记录 site.create");
    assert!(create_entry.success);
    assert!(create_entry
        .target
        .as_deref()
        .unwrap_or("")
        .starts_with("pk_"));
}

#[tokio::test]
async fn admin_login_fail_recorded_in_audit() {
    let token = "admin-token-sample-at-least-16-chars";
    let app = build_router(
        AppState::new(
            test_config_with_admin(token),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    // 用错误 token 调一个 admin endpoint
    let req = Request::builder()
        .method("GET")
        .uri("/admin/api/sites")
        .header("authorization", "Bearer WRONG-TOKEN-xxxxxxxxxxxxxxxxxxx")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // 查审计（用合法 token）
    let (status, audit_resp): (_, AuditListResp) =
        get_json_auth(&app, "/admin/api/audit?action=login.fail", token).await;
    assert_eq!(status, StatusCode::OK);
    assert!(audit_resp.total >= 1, "login.fail 应当至少有 1 条记录");
    assert!(
        audit_resp.entries.iter().all(|e| e.action == "login.fail"),
        "过滤应只返回 login.fail"
    );
}

#[tokio::test]
async fn admin_ban_after_many_failures_returns_429() {
    let token = "admin-token-ban-test-long-enough!!!!";
    let app = build_router(
        AppState::new(
            test_config_with_admin(token),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    // 失败阈值为 30（rate_limit.rs::ADMIN_FAIL_BAN_THRESHOLD）。
    // 连续 30 次错误 → 第 30 次起进入 ban。
    for i in 0..30 {
        let req = Request::builder()
            .method("GET")
            .uri("/admin/api/sites")
            .header(
                "authorization",
                format!("Bearer wrong-{i}-xxxxxxxxxxxxxxxx"),
            )
            .header("x-forwarded-for", "203.0.113.99")
            .body(Body::empty())
            .unwrap();
        let _ = app.clone().oneshot(req).await.unwrap();
    }

    // 第 31 次应该被 ban（返回 429）
    let req = Request::builder()
        .method("GET")
        .uri("/admin/api/sites")
        .header("authorization", "Bearer yet-another-wrong-xxxxxxxxxxxxxxx")
        .header("x-forwarded-for", "203.0.113.99")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn create_site_returns_plaintext_secret_key_once() {
    let token = "admin-token-sample-at-least-16-chars";
    let app = build_router(
        AppState::new(
            test_config_with_admin(token),
            captcha_server::db::open_memory(),
        ),
        None,
        None,
    );

    // 创建时返回明文
    let (status, body): (_, serde_json::Value) = post_json_auth(
        &app,
        "/admin/api/sites",
        json!({"diff": 10, "origins": []}),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let site_key = body["key"].as_str().unwrap().to_string();
    let plain_secret = body["secret_key"].as_str().unwrap().to_string();
    assert!(!plain_secret.is_empty());
    assert_ne!(plain_secret, "(hashed)");

    // list 接口返回 "(hashed)" 占位（明文不再可从管理面板取回）
    let (_, sites): (_, serde_json::Value) = get_json_auth(&app, "/admin/api/sites", token).await;
    let arr = sites.as_array().unwrap();
    let this = arr.iter().find(|s| s["key"] == site_key).unwrap();
    assert_eq!(this["secret_key"], "(hashed)");
    assert_eq!(this["secret_key_hashed"], true);
}
