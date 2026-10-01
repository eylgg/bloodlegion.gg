pub mod api;
pub mod jwk;
pub mod jwt;
pub mod local;
pub mod logout;
mod next;
pub mod oauth2;
pub mod providers;
pub mod session;

use axum::Router;
use serde::{Deserialize, Serialize};

use crate::{Slug, State};

use next::Next;

/// Shared `?next=` query extractor: a single optional redirect target,
/// validated and defaulted through [`Next`]. A missing `next` key yields the
/// fallback, as does a present-but-unsafe value.
#[derive(Deserialize)]
pub struct Params {
    #[serde(default)]
    next: Next,
}

/// The URL the frontend should navigate to after a login step. Used by local
/// login and SAML login alike so the response shape is consistent.
#[derive(Debug, Serialize)]
pub struct RedirectTarget {
    pub url: String,
}

/// The internal user fields an external identity can map onto, shared by every
/// provider protocol. Serde reads it from the snake_case names the admin API
/// sends and each provider's mapping stores ("username", "email", ...), so an
/// unknown target fails at the request boundary. SAML maps attributes straight
/// onto these; OAuth2 wraps them in [`oauth2::providers::Oauth2Target`] to add
/// its protocol-only targets. Every target is single-valued except `groups`, which
/// is multi-valued and feeds `user_groups` (the scoped affiliation an IdP asserts)
/// rather than a profile field; only the SAML login path populates it so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Username,
    Email,
    FirstName,
    LastName,
    Groups,
}

/// Provider slugs an admin can't register a SAML/OAuth2 provider under. `local` is
/// owned by the built-in local-login provider (the sole `auth_providers` kind='local'
/// row), so no external provider may claim it. The `auth_providers_slug_check`
/// database CHECK enforces the same slug<->kind lock; this gives a clean 422 first.
pub const RESERVED_PROVIDER_SLUGS: &[&str] = &["local"];

/// Whether `slug` is reserved for a built-in login kind (see
/// [`RESERVED_PROVIDER_SLUGS`]), and so may not be used for an identity provider.
pub fn is_reserved_provider_slug(slug: &str) -> bool {
    RESERVED_PROVIDER_SLUGS.contains(&slug)
}

/// The four access-control flags every auth provider (SAML or OAuth2) carries on
/// the shared `auth_providers` row. Grouped into one value so `insert_provider`
/// names each flag at the call site instead of taking four positional `bool`s
/// that are trivial to transpose.
#[derive(Debug, Clone, Copy)]
pub struct ProviderFlags {
    /// New users may register through this provider.
    pub is_registration_allowed: bool,
    /// A login may auto-link to an existing account by verified email.
    pub is_auto_connection_allowed: bool,
    /// Emails this provider asserts are treated as already verified.
    pub is_email_verified: bool,
    /// Users may disconnect this provider from their account.
    pub is_disconnection_allowed: bool,
    /// A first login may claim an unclaimed account (no credentials) by matching this provider's
    /// asserted username to the account username. Enable only when that username is authoritative.
    pub is_unclaimed_username_connection_allowed: bool,
}

/// One external provider as offered on the login page, its stable `slug` (used
/// in the login URL) and display `name`. Each provider module's login listing
/// returns these; [`api`] renders them into the public discovery payload.
#[derive(Debug)]
pub struct ProviderListing {
    pub slug: Slug,
    pub name: String,
}

/// A provider listing plus whether the signed-in user is currently connected to
/// it. Returned by each provider module's connection listing.
#[derive(Debug)]
pub struct ProviderConnectionListing {
    pub slug: Slug,
    pub name: String,
    pub connected: bool,
}

/// IdP protocol endpoints external parties call at fixed top-level URLs
/// (mounted at `/auth`, not `/api/auth`): the SAML SP routes the IdP posts to
/// and the OAuth2 token/userinfo endpoints that openid-configuration
/// advertises. The reverse proxy forwards these paths straight to the backend.
/// The browser-facing `/api/auth` endpoints are the api router in [`api`].
pub fn router() -> Router<State> {
    Router::new().nest("/oauth2", oauth2::router())
}
