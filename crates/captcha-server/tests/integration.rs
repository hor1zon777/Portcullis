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
        },
    );

    Config {
        secret: b"test-secret-key-must-be-at-least-32-bytes!!!".to_vec(),
        bind: "127.0.0.1:0".to_string(),
        sites,
        token_ttl_secs: 300,
        challenge_ttl_secs: 120,
        risk: Default::default(),
        admin_token: None,
        db_path: std::path::PathBuf::from(":memory:"),
        config_path: None,
        manifest_signing_key: None,
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
    assert_eq!(status, StatusCode::UNAUTHORIZED, "篡改 m_cost 应导致签名验证失败");
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
    assert_eq!(status, StatusCode::UNAUTHORIZED, "篡改 t_cost 应导致签名验证失败");
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
