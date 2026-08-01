use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose, Engine as _};
use std::sync::Arc;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tracing::{debug, error};

use crate::{models::InternalUser, AppState};

const TOKEN_VALIDATION_TTL: Duration = Duration::from_secs(60);
/// Upper bound on the exponential login backoff below.
const MAX_LOGIN_BACKOFF: Duration = Duration::from_secs(60);

/// Constant-time string comparison for secrets (passwords, api keys), so a
/// mismatch can't be timed to learn how many leading bytes were correct.
/// Like virtually all constant-time compare implementations, this still
/// short-circuits on length -- that leaks length, not content, which is an
/// accepted trade-off shared by e.g. `ring` and `ct_codecs`.
pub(crate) fn secure_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

pub struct AuthUser(pub InternalUser);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    Arc<AppState>: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = Arc::<AppState>::from_ref(state);
        // 1. Check OPDS_NO_AUTH
        if state.config.opds_no_auth {
            if !state.config.abs_noauth_username.is_empty()
                && !state.config.abs_noauth_password.is_empty()
            {
                // Check local cached anonymous user
                {
                    let cache = state.anonymous_user.read().await;
                    if let Some((user, expires)) = &*cache {
                        if tokio::time::Instant::now() < *expires {
                            return Ok(AuthUser(user.clone()));
                        }
                    }
                }
                // Acquire write lock and refresh
                let mut cache = state.anonymous_user.write().await;
                // Double check in case another thread populated it while we waited for write lock
                if let Some((user, expires)) = &*cache {
                    if tokio::time::Instant::now() < *expires {
                        return Ok(AuthUser(user.clone()));
                    }
                }
                match state
                    .api_client
                    .login(
                        &state.config.abs_noauth_username,
                        &state.config.abs_noauth_password,
                    )
                    .await
                {
                    Ok(user) => {
                        let expires =
                            tokio::time::Instant::now() + std::time::Duration::from_secs(500); // 500s (<10min TTL)
                        *cache = Some((user.clone(), expires));
                        return Ok(AuthUser(user));
                    }
                    Err(e) => {
                        error!("Auto-login failed for default user: {}", e);
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            format!("Authentication failed: {}", e),
                        )
                            .into_response());
                    }
                }
            } else {
                error!("OPDS_NO_AUTH enabled but credentials missing");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Server configuration error",
                )
                    .into_response());
            }
        }

        // 2. Check Basic Auth
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok());

        match auth_header {
            Some(header) if header.starts_with("Basic ") => {
                let code = &header[6..];
                if let Ok(decoded) = general_purpose::STANDARD.decode(code) {
                    if let Ok(creds) = String::from_utf8(decoded) {
                        if let Some((username, password)) = creds.split_once(':') {
                            if is_locked_out(&state, username).await {
                                debug!("Login attempts for {} are backed off", username);
                            } else {
                                // Check internal users first
                                if let Some(internal_user) =
                                    state.config.internal_users.iter().find(|u| {
                                        u.name.eq_ignore_ascii_case(username)
                                            && u.password
                                                .as_deref()
                                                .is_some_and(|p| secure_eq(p, password))
                                    })
                                {
                                    debug!("Internal user authenticated: {}", username);
                                    clear_login_attempts(&state, username).await;
                                    return Ok(AuthUser(internal_user.clone()));
                                }

                                // Check ABS login
                                debug!("Attempting ABS login for: {}", username);
                                match state.api_client.login(username, password).await {
                                    Ok(user) => {
                                        debug!("ABS user authenticated: {}", username);
                                        clear_login_attempts(&state, username).await;
                                        return Ok(AuthUser(user));
                                    }
                                    Err(e) => {
                                        debug!(
                                            "Authentication failed for user {}: {}",
                                            username, e
                                        );
                                        record_failed_login(&state, username).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                // If Authorization header is not present, check query parameter ?token=...
                if let Some(query) = parts.uri.query() {
                    if let Some(raw_token) = get_token_from_query(query) {
                        // Clients that percent-encode reserved characters (a
                        // token containing '+' or '/' run through
                        // encodeURIComponent, for example) produce %2B/%2F
                        // sequences here; decode them back before comparing
                        // or forwarding as a bearer token.
                        let token = percent_encoding::percent_decode_str(raw_token)
                            .decode_utf8()
                            .map(|s| s.into_owned())
                            .unwrap_or_else(|_| raw_token.to_string());

                        if let Some(internal_user) = state
                            .config
                            .internal_users
                            .iter()
                            .find(|u| secure_eq(&u.api_key, &token))
                        {
                            debug!("Token-authenticated internal user: {}", internal_user.name);
                            return Ok(AuthUser(internal_user.clone()));
                        }

                        // Not a configured internal user's key: only accept
                        // this as an ABS bearer token if ABS itself confirms
                        // it's valid. Previously any string here was accepted
                        // unchecked, which turned the proxy into an
                        // unauthenticated relay to whatever the ABS host
                        // would serve an arbitrary bearer token.
                        if let Some(user) = validate_bearer_token(&state, &token).await {
                            return Ok(AuthUser(user));
                        }

                        debug!("Query token did not validate against ABS");
                    }
                }
            }
        }

        // Failed
        let mut res = (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
        res.headers_mut().insert(
            "WWW-Authenticate",
            axum::http::HeaderValue::from_static("Basic realm=\"OPDS\""),
        );
        Err(res)
    }
}

/// Confirms a raw `?token=` value is a working ABS bearer token by using it
/// to call an authenticated ABS endpoint, since there is no local secret to
/// check it against. Successes are cached briefly (TOKEN_VALIDATION_TTL) so
/// repeat requests from the same reader don't each cost an extra upstream
/// round trip; failures are never cached, so probing many invalid tokens
/// can't grow the cache.
async fn validate_bearer_token(state: &Arc<AppState>, token: &str) -> Option<InternalUser> {
    if token.is_empty() {
        return None;
    }

    {
        let cache = state.validated_tokens.read().await;
        if let Some((user, expires)) = cache.get(token) {
            if tokio::time::Instant::now() < *expires {
                return Some(user.clone());
            }
        }
    }

    let candidate = InternalUser {
        name: "abs_user".to_string(),
        api_key: token.to_string(),
        password: None,
    };

    match state.api_client.get_libraries(&candidate).await {
        Ok(_) => {
            let expires = tokio::time::Instant::now() + TOKEN_VALIDATION_TTL;
            let mut cache = state.validated_tokens.write().await;
            let now = tokio::time::Instant::now();
            cache.retain(|_, (_, exp)| now < *exp);
            cache.insert(token.to_string(), (candidate.clone(), expires));
            Some(candidate)
        }
        Err(e) => {
            debug!("Bearer token validation against ABS failed: {}", e);
            None
        }
    }
}

/// Returns true if `username` is currently backed off from a run of failed
/// login attempts. Doesn't distinguish "wrong password" from "unknown
/// username" -- both count, since the point is to slow down guessing either.
async fn is_locked_out(state: &Arc<AppState>, username: &str) -> bool {
    let attempts = state.login_attempts.read().await;
    attempts
        .get(username)
        .is_some_and(|(_, not_before)| tokio::time::Instant::now() < *not_before)
}

/// Records a failed login for `username` and sets an exponentially growing
/// not-before time (2^failures seconds, capped at MAX_LOGIN_BACKOFF).
async fn record_failed_login(state: &Arc<AppState>, username: &str) {
    let mut attempts = state.login_attempts.write().await;
    let now = tokio::time::Instant::now();
    // Opportunistic cleanup: drop entries whose backoff expired a while ago,
    // bounding memory use the same way api.rs's caches do on each write.
    attempts.retain(|_, (_, not_before)| now < *not_before + MAX_LOGIN_BACKOFF);

    let entry = attempts.entry(username.to_string()).or_insert((0, now));
    entry.0 = entry.0.saturating_add(1);
    let backoff = Duration::from_secs(2u64.saturating_pow(entry.0.min(6))).min(MAX_LOGIN_BACKOFF);
    entry.1 = now + backoff;
}

async fn clear_login_attempts(state: &Arc<AppState>, username: &str) {
    let mut attempts = state.login_attempts.write().await;
    attempts.remove(username);
}

pub(crate) fn get_token_from_query(query: &str) -> Option<&str> {
    for param in query.split('&') {
        if let Some((key, val)) = param.split_once('=') {
            if key == "token" {
                return Some(val);
            }
        }
    }
    None
}
