//! Shared identity for the `auth_providers` envelope both external-IdP kinds
//! (SAML, OAuth2) hang off of.

mod provider_id;

pub use provider_id::ProviderId;
