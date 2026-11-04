//! Shared authenticated Blackboard HTTP client. Lazy login on first use, one
//! re-login + retry on session expiry, no retry loops.
//!
//! Port of bb-client.mjs's compare-and-swap dedupe: tool calls run with
//! concurrency (e.g. search_courses_content's mapLimit fan-out), so several
//! requests can see the session expired at once. Without dedupe, each would
//! independently restart login, firing multiple logins inside the same 30s
//! TOTP window, and adAS then rejects the reused/concurrent code as replay. The
//! JS keyed this off `loginPromise === stale` (object identity of the stored
//! Promise); here a monotonic generation counter plays the same role: only
//! the caller holding the generation that is *still* current is allowed to
//! null it out and start a new login, everyone else just awaits the
//! in-flight one.

use crate::auth;
use crate::credentials;
use anyhow::{anyhow, bail, Context, Result};
use futures::future::{FutureExt, Shared};
use reqwest::Client;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

const BASE: &str = "https://ev.us.es";

#[derive(Clone)]
struct LoginResult {
    client: Client,
    user_id: String,
}

type LoginError = Arc<anyhow::Error>;
type LoginFuture = Shared<Pin<Box<dyn Future<Output = Result<LoginResult, LoginError>> + Send>>>;

pub struct BbClient {
    state: Mutex<Option<(u64, LoginFuture)>>,
    generation: AtomicU64,
}

impl Default for BbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BbClient {
    pub fn new() -> Self {
        Self { state: Mutex::new(None), generation: AtomicU64::new(0) }
    }

    /// Returns (generation, shared future) of the current/in-flight login,
    /// starting one if none is in flight. Mirrors ensureLoggedIn()'s lazy
    /// singleton loginPromise.
    async fn ensure_logged_in(&self) -> (u64, LoginFuture) {
        let mut guard = self.state.lock().await;
        if let Some(existing) = guard.as_ref() {
            return existing.clone();
        }
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let fut: Pin<Box<dyn Future<Output = Result<LoginResult, LoginError>> + Send>> =
            Box::pin(do_login());
        let shared = fut.shared();
        *guard = Some((gen, shared.clone()));
        (gen, shared)
    }

    /// Compare-and-swap re-login: only actually restarts login if `stale_gen`
    /// is still the generation currently stored.
    async fn reauth(&self, stale_gen: u64) -> (u64, LoginFuture) {
        {
            let mut guard = self.state.lock().await;
            if matches!(guard.as_ref(), Some((gen, _)) if *gen == stale_gen) {
                *guard = None;
            }
        }
        self.ensure_logged_in().await
    }

    fn looks_like_expired_session(res: &reqwest::Response) -> bool {
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            return true;
        }
        res.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("text/html"))
            .unwrap_or(false)
    }

    /// GET a Blackboard REST path (e.g. "/learn/api/public/v1/users/me") or an
    /// absolute URL (e.g. a pre-signed WebDAV resourceUrl from an Ultra
    /// document body), re-logging in once if the session has expired. One
    /// re-login attempt, no retry loops.
    pub async fn get_raw(&self, path_or_url: &str) -> Result<reqwest::Response> {
        let url = if path_or_url.starts_with("http") {
            let parsed = reqwest::Url::parse(path_or_url).context("parsing resource URL")?;
            if parsed.scheme() != "https" || parsed.host_str() != Some("ev.us.es") {
                bail!("refusing to fetch resource outside ev.us.es: {path_or_url}");
            }
            path_or_url.to_string()
        } else {
            format!("{BASE}{path_or_url}")
        };

        let (gen, fut) = self.ensure_logged_in().await;
        let login = fut.await.map_err(|e| anyhow!("{e}"))?;
        let mut res = login.client.get(&url).send().await?;
        if Self::looks_like_expired_session(&res) {
            let (_, fut2) = self.reauth(gen).await;
            let login2 = fut2.await.map_err(|e| anyhow!("{e}"))?;
            res = login2.client.get(&url).send().await?;
        }
        if !res.status().is_success() {
            bail!("{path_or_url} returned {}", res.status());
        }
        Ok(res)
    }

    pub async fn get(&self, path: &str) -> Result<serde_json::Value> {
        Ok(self.get_raw(path).await?.json().await?)
    }

    pub async fn user_id(&self) -> Result<String> {
        let (_, fut) = self.ensure_logged_in().await;
        let login = fut.await.map_err(|e| anyhow!("{e}"))?;
        Ok(login.user_id)
    }
}

async fn do_login() -> Result<LoginResult, LoginError> {
    let attempt = async {
        let creds = credentials::load_credentials()?;
        let client = auth::login(&creds).await?;
        let res = client
            .get(format!("{BASE}/learn/api/public/v1/users/me"))
            .send()
            .await?;
        if !res.status().is_success() {
            bail!("post-login /users/me returned {}", res.status());
        }
        let me: serde_json::Value = res.json().await?;
        let user_id = me
            .get("id")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .ok_or_else(|| anyhow!("post-login /users/me response missing id"))?;
        Ok(LoginResult { client, user_id })
    };
    attempt.await.map_err(Arc::new)
}
