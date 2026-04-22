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
            // origins 留空，测试客户端不发 Origin header，避免 CORS/Origin 拦截
            origins: vec![],
        },
    );

    Config {
        secret: b"test-secret-key-must-be-at-least-32-bytes!!!".to_vec(),
        bind: "127.0.0.1:0".to_string(),
        sites,
        token_ttl_secs: 300,
        challenge_ttl_secs: 120,
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

async fn post_raw(
    app: &axum::Router,
    path: &str,
    body: serde_json::Value,
) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    app.clone().oneshot(req).await.unwrap().status()
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
    let app = build_router(AppState::new(test_config()), None, None);

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
    let app = build_router(AppState::new(test_config()), None, None);

    let (_, ch): (_, ChallengeResp) =
        post_json(&app, "/api/v1/challenge", json!({ "site_key": "pk_test" })).await;
    let (nonce, _) = pow::solve(&ch.challenge, 1_000_000, 0, |_| {}).unwrap();

    let body = json!({ "challenge": ch.challenge, "sig": ch.sig, "nonce": nonce });
    assert_eq!(post_raw(&app, "/api/v1/verify", body.clone()).await, StatusCode::OK);
    assert_eq!(post_raw(&app, "/api/v1/verify", body).await, StatusCode::CONFLICT);
}

#[tokio::test]
async fn bad_sig_rejected() {
    let app = build_router(AppState::new(test_config()), None, None);

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
    let app = build_router(AppState::new(test_config()), None, None);
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
    let app = build_router(AppState::new(test_config()), None, None);

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
    let app = build_router(AppState::new(test_config()), None, None);

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
    let app = build_router(AppState::new(test_config()), None, None);
    let req = Request::builder()
        .method("GET")
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
