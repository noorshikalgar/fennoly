//! Passwords, sessions and the `CurrentUser` / `AdminUser` extractors.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Duration, Utc};
use fennoly_core::Role;
use rand::RngCore;
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "fennoly_session";
const SESSION_DAYS: i64 = 30;
pub const MIN_PASSWORD_LEN: usize = 8;

pub async fn hash_password(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("hashing failed: {e}")))
    })
    .await?
}

pub async fn verify_password(password: String, hash: String) -> AppResult<bool> {
    Ok(tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
            .unwrap_or(false)
    })
    .await?)
}

pub fn check_password_strength(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::bad(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

/// Usernames are short, lower-case and safe to use as a folder name.
pub fn normalize_username(raw: &str) -> AppResult<String> {
    let name = raw.trim().to_lowercase();
    let ok_len = (2..=32).contains(&name.chars().count());
    let ok_chars = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    let ok_start = name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if ok_len && ok_chars && ok_start {
        Ok(name)
    } else {
        Err(AppError::bad(
            "username must be 2-32 characters: letters a-z, digits, dot, dash or underscore",
        ))
    }
}

fn token_hash(token: &str) -> Vec<u8> {
    blake3::hash(token.as_bytes()).as_bytes().to_vec()
}

/// Creates a session and returns the cookie that carries its token.
pub async fn start_session(
    state: &AppState,
    user_id: Uuid,
    user_agent: Option<String>,
) -> AppResult<Cookie<'static>> {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, user_agent, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(token_hash(&token))
    .bind(user_agent.map(|ua| ua.chars().take(200).collect::<String>()))
    .bind(Utc::now() + Duration::days(SESSION_DAYS))
    .execute(&state.db)
    .await?;
    Ok(session_cookie(state, token, time_days(SESSION_DAYS)))
}

fn time_days(days: i64) -> time::Duration {
    time::Duration::days(days)
}

fn session_cookie(state: &AppState, value: String, max_age: time::Duration) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.cfg.secure_cookies)
        .max_age(max_age)
        .build()
}

pub fn clear_cookie(state: &AppState) -> Cookie<'static> {
    session_cookie(state, String::new(), time::Duration::ZERO)
}

pub async fn end_session(state: &AppState, jar: &CookieJar) -> AppResult<()> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(token_hash(c.value()))
            .execute(&state.db)
            .await?;
    }
    Ok(())
}

/// The signed-in user making the request.
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CurrentUser {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub locale: String,
    pub theme: String,
    pub avatar_color: String,
    pub quota_bytes: Option<i64>,
    #[serde(skip)]
    pub session_id: Uuid,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        Role::parse(&self.role) == Some(Role::Admin)
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).ok_or(AppError::Unauthorized)?.value().to_string();
        let user: CurrentUser = sqlx::query_as(
            "SELECT u.id, u.username, u.display_name, u.role, u.locale, u.theme, u.avatar_color,
                    u.quota_bytes, s.id AS session_id
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.expires_at > now() AND u.disabled_at IS NULL",
        )
        .bind(token_hash(&token))
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;
        // Touch the session at most every few minutes to keep writes low.
        sqlx::query(
            "UPDATE sessions SET last_seen_at = now()
             WHERE id = $1 AND last_seen_at < now() - interval '5 minutes'",
        )
        .bind(user.session_id)
        .execute(&state.db)
        .await?;
        Ok(user)
    }
}

/// Loads an active user outside a request (background jobs acting for them).
pub async fn user_by_id(state: &AppState, id: Uuid) -> AppResult<CurrentUser> {
    sqlx::query_as(
        "SELECT id, username, display_name, role, locale, theme, avatar_color, quota_bytes,
                '00000000-0000-0000-0000-000000000000'::uuid AS session_id
         FROM users WHERE id = $1 AND disabled_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)
}

/// A signed-in admin.
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        if user.is_admin() { Ok(AdminUser(user)) } else { Err(AppError::Forbidden) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert_eq!(normalize_username(" Noor ").unwrap(), "noor");
        assert!(normalize_username("aai.baba-1").is_ok());
        assert!(normalize_username("a").is_err());
        assert!(normalize_username("-noor").is_err());
        assert!(normalize_username("noor/../x").is_err());
        assert!(normalize_username("नूर").is_err());
    }

    #[tokio::test]
    async fn password_round_trip() {
        let hash = hash_password("correct horse".into()).await.unwrap();
        assert!(verify_password("correct horse".into(), hash.clone()).await.unwrap());
        assert!(!verify_password("wrong".into(), hash).await.unwrap());
    }
}
