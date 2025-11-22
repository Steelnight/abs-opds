use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose, Engine as _};
use std::sync::Arc;
use tracing::{debug, error};

use crate::{models::InternalUser, AppState};

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
                match state
                    .api_client
                    .login(
                        &state.config.abs_noauth_username,
                        &state.config.abs_noauth_password,
                    )
                    .await
                {
                    Ok(user) => return Ok(AuthUser(user)),
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
                            // Check internal users first
                            if let Some(internal_user) =
                                state.config.internal_users.iter().find(|u| {
                                    u.name.eq_ignore_ascii_case(username)
                                        && u.password.as_deref() == Some(password)
                                })
                            {
                                debug!("Internal user authenticated: {}", username);
                                return Ok(AuthUser(internal_user.clone()));
                            }

                            // Check ABS login
                            debug!("Attempting ABS login for: {}", username);
                            match state.api_client.login(username, password).await {
                                Ok(user) => {
                                    debug!("ABS user authenticated: {}", username);
                                    return Ok(AuthUser(user));
                                }
                                Err(e) => {
                                    debug!("Authentication failed for user {}: {}", username, e);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
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
