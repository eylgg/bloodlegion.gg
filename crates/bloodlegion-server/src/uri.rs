//! The two URL value types. Both mirror their columns' length CHECK (<=512
//! characters) and add the URL validation the database can't express:
//!
//! - [`HttpsUrl`], an absolute `https://` URL. Used for every endpoint the
//!   server *fetches*: SAML `metadata_url`/`sso_url`, OIDC `issuer`/`*_endpoint`/
//!   `jwks_uri`, and the Canvas/Forgejo base URLs. https is non-negotiable there.
//! - [`RedirectUri`], an absolute URI of *any* scheme, stored verbatim. Used
//!   only for the OAuth2 client redirect URI, which native/mobile clients give as
//!   a custom scheme (`com.example.app://cb`) or loopback `http://127.0.0.1`, and
//!   which the token endpoint compares by exact string, so it must not require
//!   https and must not be normalized.
//!
//! The module is named `uri` (not `url`) so it doesn't shadow the `url` crate.

use serde::Serialize;

use crate::newtype::string_newtype;

/// Shared maximum length, mirroring the URL columns' `char_length(...) <= 512`.
const MAX_LEN: usize = 512;

#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a valid https URL")]
pub struct HttpsUrlError(String);

#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a valid redirect URI")]
pub struct RedirectUriError(String);

/// An absolute `https://` URL of at most 512 characters. Validates the value as
/// given, callers trim first if they want to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HttpsUrl(String);

/// An absolute URI of any scheme, at most 512 characters, stored verbatim (no normalization, since
/// the OAuth2 token endpoint matches it exactly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedirectUri(String);

fn bounded(value: &str) -> bool {
    (1..=MAX_LEN).contains(&value.chars().count())
}

impl HttpsUrl {
    fn new(value: &str) -> Result<Self, HttpsUrlError> {
        if bounded(value) && url::Url::parse(value).is_ok_and(|url| url.scheme() == "https") {
            Ok(HttpsUrl(value.to_string()))
        } else {
            Err(HttpsUrlError(value.to_string()))
        }
    }
}

impl RedirectUri {
    fn new(value: &str) -> Result<Self, RedirectUriError> {
        // `Url::parse` succeeds only for an absolute URI (it requires a scheme),
        // so this rejects relative paths while allowing any scheme.
        if bounded(value) && url::Url::parse(value).is_ok() {
            Ok(RedirectUri(value.to_string()))
        } else {
            Err(RedirectUriError(value.to_string()))
        }
    }
}

string_newtype!(HttpsUrl, HttpsUrlError);
string_newtype!(RedirectUri, RedirectUriError);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_url_requires_absolute_https() {
        for ok in ["https://idp.example.com/metadata", "https://x.test"] {
            assert!(HttpsUrl::try_from(ok).is_ok(), "{ok:?} should be valid");
        }
        for (bad, why) in [
            ("http://x.test", "not https"),
            ("ftp://x.test", "not https"),
            ("/relative", "not absolute"),
            ("not a url", "unparseable"),
            ("", "empty"),
        ] {
            assert!(
                HttpsUrl::try_from(bad).is_err(),
                "{bad:?} should be rejected: {why}"
            );
        }
        assert!(
            HttpsUrl::try_from(format!("https://x.test/{}", "a".repeat(512)).as_str()).is_err()
        );
    }

    #[test]
    fn redirect_uri_allows_any_absolute_scheme() {
        for ok in [
            "https://app.test/cb",
            "http://127.0.0.1:8080/cb", // loopback for native apps
            "com.example.app://callback",
        ] {
            assert!(RedirectUri::try_from(ok).is_ok(), "{ok:?} should be valid");
        }
        for (bad, why) in [
            ("/cb", "not absolute"),
            ("", "empty"),
            ("nonsense", "no scheme"),
        ] {
            assert!(
                RedirectUri::try_from(bad).is_err(),
                "{bad:?} should be rejected: {why}"
            );
        }
    }
}
