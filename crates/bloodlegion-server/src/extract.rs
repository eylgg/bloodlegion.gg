use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::Deref;

use anyhow::Context as _;
use axum::extract::{ConnectInfo, FromRequestParts, OptionalFromRequestParts};
use axum::http::header;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;

use crate::auth::session::authenticate;
use crate::{Error, Problem, State, users::User};

/// Best-effort client IP and User-Agent, captured as session metadata at login.
///
/// Never rejects: a missing header falls back to `None` and an absent peer
/// address to `0.0.0.0`, so the session row can always be written. These values
/// are **informational only**, never gate a request on them (IP/UA pinning logs
/// legitimate users out and adds nothing over the session token).
///
/// # Security: `X-Forwarded-For` trust boundary
///
/// `ip_address` is read from the first `X-Forwarded-For` entry, which is only
/// trustworthy when the backend is reachable **exclusively** through the trusted
/// reverse proxy that sets/overwrites that header, Caddy in dev/e2e, Traefik in
/// production. The proxy must strip any client-supplied `X-Forwarded-For` and
/// append the real peer. The backend's `:8080` listener must therefore not be
/// exposed directly: a client that can reach it directly could spoof this IP and
/// evade (or poison) the per-IP login throttle in [`crate::auth::local`]. The IP
/// is never used as a security gate on its own, only as a soft brute-force
/// signal alongside the username-scoped lockout, which bounds the impact.
pub struct ClientInfo {
    pub ip_address: IpAddr,
    pub user_agent: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for ClientInfo {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Absent (or empty) User-Agent is stored as NULL, not "".
        let user_agent = parts
            .headers
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.chars().take(1024).collect::<String>())
            .filter(|value| !value.is_empty());

        // Behind the reverse proxy the socket peer is the proxy itself; the real
        // client is the first entry of X-Forwarded-For. Fall back to the peer
        // address, then to 0.0.0.0, so a valid address is always recorded.
        let forwarded = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|value| value.trim().parse::<IpAddr>().ok());
        let ip_address = forwarded
            .or_else(|| {
                parts
                    .extensions
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|info| info.0.ip())
            })
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

        Ok(ClientInfo {
            ip_address,
            user_agent,
        })
    }
}

/// Why an auth extractor rejected a request, rendered as RFC 9457 problem JSON.
/// Carried as the `External` arm of [`crate::Error`]; database failures during
/// extraction become the `Internal` arm (a 500) instead.
#[derive(Debug, thiserror::Error, Problem)]
pub enum AuthRejection {
    #[error("authentication required")]
    #[problem(
        status = UNAUTHORIZED,
        title = "Unauthorized",
        detail = "Valid authentication credentials are required to access this resource."
    )]
    Unauthorized,
    #[error("forbidden")]
    #[problem(
        status = FORBIDDEN,
        title = "Forbidden",
        detail = "You do not have the necessary permissions to access this resource."
    )]
    Forbidden,
    #[error("cross-origin request")]
    #[problem(
        status = FORBIDDEN,
        title = "Forbidden",
        detail = "Cross-origin requests are not permitted for this endpoint."
    )]
    CrossOrigin,
}

impl FromRequestParts<State> for State {
    type Rejection = Infallible;

    async fn from_request_parts(
        _parts: &mut Parts,
        state: &State,
    ) -> Result<Self, Self::Rejection> {
        Ok(state.clone())
    }
}

/// Rejects cross-origin requests with `403 Forbidden`.
///
/// Compares the `Origin` header against `state.origin` and accepts requests
/// that omit the header (same-origin navigations, `curl`, server-to-server
/// calls). Add this extractor to state-changing handlers that browsers call
/// with cookie credentials, it stops cross-site request forgery, since a
/// hostile page cannot send a forged `Origin`.
pub struct SameOrigin;

impl FromRequestParts<State> for SameOrigin {
    type Rejection = Error<AuthRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        match parts.headers.get(header::ORIGIN) {
            Some(origin) if origin.as_bytes() != state.origin.as_bytes() => {
                Err(Error::External(AuthRejection::CrossOrigin))
            }
            _ => Ok(SameOrigin),
        }
    }
}

/// Extracts the signed-in user from the session cookie.
///
/// Looks up the cookie's hashed id in `auth_sessions` and rejects with
/// `401 Unauthorized` when the cookie is missing, unknown, or expired.
/// Take `User` in handlers that require a session; take `Option<User>` (the
/// implementation below) in handlers that serve both signed-in and anonymous
/// callers, the optional form never rejects.
impl FromRequestParts<State> for User {
    type Rejection = Error<AuthRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .expect("CookieJar extraction is infallible");
        let user = authenticate(&state.pool, jar)
            .await
            .context("loading the session user from the cookie")?
            .ok_or(Error::External(AuthRejection::Unauthorized))?;
        Ok(user)
    }
}

impl OptionalFromRequestParts<State> for User {
    type Rejection = Error<AuthRejection>;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &State,
    ) -> Result<Option<Self>, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .expect("CookieJar extraction is infallible");
        Ok(authenticate(&state.pool, jar)
            .await
            .context("loading the session user from the cookie")?)
    }
}

/// Extracts the signed-in user and requires the admin flag.
///
/// Rejects with `401 Unauthorized` when no valid session exists and
/// `403 Forbidden` when the user is not an admin. Derefs to [`User`], so
/// handlers that only gate on admin access can ignore the binding
/// (`_admin: Admin`) and handlers that need the caller can read through it.
pub struct Admin(pub User);

impl Deref for Admin {
    type Target = User;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<State> for Admin {
    type Rejection = Error<AuthRejection>;

    async fn from_request_parts(parts: &mut Parts, state: &State) -> Result<Self, Self::Rejection> {
        let user = <User as FromRequestParts<State>>::from_request_parts(parts, state).await?;
        user.is_superuser
            .then_some(Admin(user))
            .ok_or(Error::External(AuthRejection::Forbidden))
    }
}
