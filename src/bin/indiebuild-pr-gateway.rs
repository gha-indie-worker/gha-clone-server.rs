use std::{collections::HashMap, env, sync::Arc, time::Duration};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use tokio::time::{sleep, Instant};

const SERVICE_NAME: &str = "indiebuild-pr-gateway";
const STATUS_CONTEXT: &str = "indiebuild.dev/ci";

#[derive(Clone)]
struct Config {
    webhook_secret: String,
    github_status_token: String,
    build_server_url: String,
    build_server_auth: String,
    public_base_url: String,
    repo_profiles: HashMap<String, String>,
    poll_interval: Duration,
    poll_deadline: Duration,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let webhook_secret = required_env("INDIEBUILD_GITHUB_WEBHOOK_SECRET")?;
        let github_status_token = env_first(&["INDIEBUILD_GITHUB_STATUS_TOKEN", "GH_PAT"])
            .ok_or_else(|| "INDIEBUILD_GITHUB_STATUS_TOKEN (or GH_PAT) is required".to_string())?;
        let build_server_url = required_env("INDIEBUILD_BUILD_SERVER_URL")?
            .trim_end_matches('/')
            .to_string();
        let build_server_auth = required_env("INDIEBUILD_BUILD_SERVER_AUTH")?;
        let public_base_url = env::var("INDIEBUILD_PUBLIC_BASE_URL")
            .unwrap_or_else(|_| "https://indiebuild.dev".to_string())
            .trim_end_matches('/')
            .to_string();
        let repo_profiles_raw = required_env("INDIEBUILD_REPO_PROFILES")?;
        let repo_profiles: HashMap<String, String> = serde_json::from_str(&repo_profiles_raw)
            .map_err(|error| format!("INDIEBUILD_REPO_PROFILES must be a JSON object: {error}"))?;
        if repo_profiles.is_empty() {
            return Err("INDIEBUILD_REPO_PROFILES must contain at least one repository".to_string());
        }

        let poll_interval = Duration::from_secs(env_u64("INDIEBUILD_POLL_SECONDS", 2));
        let poll_deadline =
            Duration::from_secs(env_u64("INDIEBUILD_POLL_DEADLINE_SECONDS", 3600));

        Ok(Self {
            webhook_secret,
            github_status_token,
            build_server_url,
            build_server_auth,
            public_base_url,
            repo_profiles,
            poll_interval,
            poll_deadline,
        })
    }
}

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildAccepted {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildSnapshot {
    status: String,
    error: Option<String>,
}

fn required_env(key: &str) -> Result<String, String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn env_first(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn env_u64(key: &str, fallback: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn verify_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let Some(hex_signature) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let actual = mac.finalize().into_bytes();
    actual.as_slice().ct_eq(expected.as_slice()).into()
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

async fn healthz() -> impl IntoResponse {
    Json(json!({ "ok": true, "service": SERVICE_NAME }))
}

async fn github_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if header(&headers, "x-github-event") == "ping" {
        return (StatusCode::OK, Json(json!({ "ok": true, "event": "ping" }))).into_response();
    }
    if header(&headers, "x-github-event") != "pull_request" {
        return (
            StatusCode::OK,
            Json(json!({ "ok": true, "action": "ignored", "reason": "event" })),
        )
            .into_response();
    }

    let signature = header(&headers, "x-hub-signature-256");
    if !verify_signature(&state.config.webhook_secret, &body, signature) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or missing X-Hub-Signature-256" })),
        )
            .into_response();
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid GitHub webhook JSON: {error}") })),
            )
                .into_response();
        }
    };

    let action = payload.get("action").and_then(Value::as_str).unwrap_or_default();
    if !matches!(
        action,
        "opened" | "synchronize" | "reopened" | "ready_for_review"
    ) {
        return (
            StatusCode::OK,
            Json(json!({ "ok": true, "action": "ignored", "reason": "pull request action" })),
        )
            .into_response();
    }

    let repository = payload
        .pointer("/repository/full_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(profile) = state.config.repo_profiles.get(repository).cloned() else {
        return (
            StatusCode::OK,
            Json(json!({ "ok": true, "action": "ignored", "reason": "repository is not allowlisted" })),
        )
            .into_response();
    };

    let draft = payload
        .pointer("/pull_request/draft")
        .and_then(Value::as_bool)
        == Some(true);
    if draft {
        return (
            StatusCode::OK,
            Json(json!({ "ok": true, "action": "ignored", "reason": "draft pull request" })),
        )
            .into_response();
    }

    let head_repo = payload
        .pointer("/pull_request/head/repo/full_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !head_repo.eq_ignore_ascii_case(repository) {
        // Repository code is executed by the external worker. Fork PRs therefore
        // fail closed rather than running untrusted code with worker credentials.
        return (
            StatusCode::OK,
            Json(json!({ "ok": true, "action": "ignored", "reason": "fork pull requests are not executed" })),
        )
            .into_response();
    }

    let head_ref = payload
        .pointer("/pull_request/head/ref")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let sha = payload
        .pointer("/pull_request/head/sha")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let pr_number = payload
        .get("number")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if head_ref.is_empty() || !valid_sha(sha) || pr_number == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "pull request head ref, number, and SHA are required" })),
        )
            .into_response();
    }

    let response_repository = repository.to_string();
    let response_sha = sha.to_string();
    let response_profile = profile.clone();
    let task_state = state.clone();
    let task_repository = response_repository.clone();
    let task_sha = response_sha.clone();
    let task_profile = response_profile.clone();
    let task_head_ref = head_ref.to_string();
    tokio::spawn(async move {
        if let Err(error) = run_external_ci(
            &task_state,
            &task_repository,
            pr_number,
            &task_head_ref,
            &task_sha,
            &task_profile,
        )
        .await
        {
            tracing::error!(
                repository = task_repository,
                pr_number,
                sha = task_sha,
                error = %error,
                "external PR CI failed"
            );
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "ok": true,
            "action": "queued",
            "repository": response_repository,
            "sha": response_sha,
            "profile": response_profile,
            "statusContext": STATUS_CONTEXT
        })),
    )
        .into_response()
}

fn valid_sha(value: &str) -> bool {
    (7..=64).contains(&value.len()) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

async fn run_external_ci(
    state: &AppState,
    repository: &str,
    pr_number: u64,
    head_ref: &str,
    sha: &str,
    profile: &str,
) -> Result<(), String> {
    let description = format!("IndieBuild queued {profile}");
    post_status(state, repository, sha, "pending", &description).await?;

    let request_id = format!("github-pr:{repository}:{pr_number}:{sha}:{profile}");
    let submit_url = format!("{}/builds", state.config.build_server_url);
    let response = state
        .http
        .post(&submit_url)
        .header("x-server-auth", &state.config.build_server_auth)
        .json(&json!({
            "schemaVersion": "build-server.v1",
            "jobKind": "run-profile",
            "repoUrl": format!("https://github.com/{repository}.git"),
            "gitRef": head_ref,
            "profile": profile,
            "push": false,
            "requestId": request_id
        }))
        .send()
        .await
        .map_err(|error| format!("build submission failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let message = format!("IndieBuild submit failed ({status})");
        let _ = post_status(state, repository, sha, "failure", &message).await;
        return Err(format!("build server rejected request: {status} {body}"));
    }

    let accepted: BuildAccepted = response
        .json()
        .await
        .map_err(|error| format!("invalid build-server acceptance response: {error}"))?;
    let deadline = Instant::now() + state.config.poll_deadline;
    let status_url = format!("{}/builds/{}", state.config.build_server_url, accepted.id);

    loop {
        if Instant::now() >= deadline {
            let message = "IndieBuild timed out waiting for external CI";
            let _ = post_status(state, repository, sha, "error", message).await;
            return Err(message.to_string());
        }

        sleep(state.config.poll_interval).await;
        let response = state
            .http
            .get(&status_url)
            .header("x-server-auth", &state.config.build_server_auth)
            .send()
            .await
            .map_err(|error| format!("build status request failed: {error}"))?;
        if !response.status().is_success() {
            continue;
        }
        let snapshot: BuildSnapshot = match response.json().await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::warn!(error = %error, "ignored malformed build status response");
                continue;
            }
        };
        match snapshot.status.to_ascii_lowercase().as_str() {
            "queued" | "running" => {}
            "succeeded" => {
                post_status(
                    state,
                    repository,
                    sha,
                    "success",
                    "IndieBuild external CI passed",
                )
                .await?;
                return Ok(());
            }
            "failed" => {
                let detail = snapshot.error.as_deref().unwrap_or("external CI failed");
                let description = truncate_description(&format!("IndieBuild failed: {detail}"));
                post_status(state, repository, sha, "failure", &description).await?;
                return Err(detail.to_string());
            }
            other => {
                tracing::warn!(status = other, "unknown build status; continuing to poll");
            }
        }
    }
}

fn truncate_description(value: &str) -> String {
    value.chars().take(140).collect()
}

async fn post_status(
    state: &AppState,
    repository: &str,
    sha: &str,
    status: &str,
    description: &str,
) -> Result<(), String> {
    let url = format!("https://api.github.com/repos/{repository}/statuses/{sha}");
    let response = state
        .http
        .post(url)
        .bearer_auth(&state.config.github_status_token)
        .header("accept", "application/vnd.github+json")
        .header("user-agent", SERVICE_NAME)
        .header("x-github-api-version", "2022-11-28")
        .json(&json!({
            "state": status,
            "context": STATUS_CONTEXT,
            "description": truncate_description(description),
            "target_url": state.config.public_base_url
        }))
        .send()
        .await
        .map_err(|error| format!("GitHub status request failed: {error}"))?;
    if !response.status().is_success() {
        let code = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("GitHub status rejected: {code} {body}"));
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config =
        Config::from_env().unwrap_or_else(|error| panic!("configuration error: {error}"));
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8127);
    let state = AppState {
        config: Arc::new(config),
        http: reqwest::Client::new(),
    };
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/webhooks/github", post(github_webhook))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind((host.as_str(), port))
        .await
        .unwrap_or_else(|error| panic!("failed to bind {host}:{port}: {error}"));
    tracing::info!(%host, port, "{SERVICE_NAME} listening");
    axum::serve(listener, app)
        .await
        .unwrap_or_else(|error| panic!("server failed: {error}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_round_trip() {
        let secret = "secret";
        let body = br#"{"action":"synchronize"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_signature(secret, body, &signature));
        assert!(!verify_signature("wrong", body, &signature));
    }

    #[test]
    fn sha_validation_is_strict() {
        assert!(valid_sha("0123456789abcdef0123456789abcdef01234567"));
        assert!(!valid_sha("not-a-sha"));
        assert!(!valid_sha("123"));
    }

    #[test]
    fn descriptions_are_bounded_for_github() {
        assert_eq!(truncate_description(&"x".repeat(200)).len(), 140);
    }
}
