use serde::{Deserialize, Deserializer};
use sqlx::Postgres;
use sqlx::postgres::{PgTypeInfo, PgValueRef};

/// Fallback redirect target when no `?next=` is supplied or validation fails.
/// Applies to every auth transition, post-login, post-logout, SAML
/// idp-initiated returns, the SPA, `curl`. Exposed to the frontend via
/// `GET /api/auth/providers` so the login UI doesn't have to hard-code it.
pub const NEXT_FALLBACK: &str = "/";

/// A `next` target is only honored when it is a same-site relative path: the
/// root, or a single leading `/` that does not start a scheme-relative (`//`) or
/// backslash-smuggled (`/\`) URL the browser would treat as off-site. Anything
/// else (absolute URL, empty, protocol-relative) is an open-redirect vector and
/// is rejected.
///
/// As defense in depth the target must also be free of backslashes and ASCII
/// control characters *anywhere*, not just at the start: browsers normalize
/// `\` to `/` and a stray CR/LF could enable response-header smuggling if the
/// value ever reached a header unencoded.
fn is_valid_redirect(target: &str) -> bool {
    if target == NEXT_FALLBACK {
        return true;
    }
    target.starts_with('/')
        && !target.starts_with("//")
        && !target.starts_with("/\\")
        && !target
            .bytes()
            .any(|byte| byte == b'\\' || byte.is_ascii_control())
}

/// A `next` value that failed validation. Only surfaces from the
/// [`sqlx::Decode`] boundary, a stored redirect target that isn't a safe
/// same-site path is a broken invariant, so it becomes a plain internal error
/// (500). Deliberately a `thiserror` type, not a `Problem`. Mirrors `slug::Error`.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a valid redirect target")]
pub struct Error(String);

/// A validated post-auth redirect target, always safe to hand to `Redirect::to`.
/// The two trust boundaries treat an invalid value differently:
///
/// - an untrusted `?next=` query param is sanitized by [`Deserialize`], quietly
///   falling back to [`NEXT_FALLBACK`] when absent or unsafe, a bad redirect
///   param must not reject the request;
/// - a value reloaded from the (`NOT NULL`) storage column is validated by
///   [`sqlx::Decode`] via [`Next::new`], which *errors* (-> 500) on an invalid
///   value rather than falling back, since a bad stored value is a broken
///   invariant. Mirrors `Slug`.
///
/// Everything else just carries the `Next` around and dereferences it to `&str`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Next(String);

impl Next {
    /// Strict constructor: validates `value`, returning [`Error`] when it is not
    /// a safe same-site path. The constructor behind the [`sqlx::Decode`] boundary.
    fn new(value: &str) -> Result<Self, Error> {
        if is_valid_redirect(value) {
            Ok(Next(value.to_string()))
        } else {
            Err(Error(value.to_string()))
        }
    }

    /// Lenient constructor: wraps `value`, falling back to [`NEXT_FALLBACK`] when
    /// it is absent or unsafe. Used for untrusted `?next=` query params.
    fn sanitize(value: &str) -> Self {
        Next::new(value).unwrap_or_default()
    }

    /// A target the server chooses itself (a flow that always ends on one page), so there is
    /// nothing untrusted to validate. Still checked, so a typo panics in tests, not in a browser.
    pub fn fixed(path: &'static str) -> Self {
        assert!(is_valid_redirect(path), "{path:?} is not a safe redirect");
        Next(path.to_string())
    }
}

#[cfg(test)]
impl Next {
    /// Test-only: build a `Next` for a fixture without going through a request
    /// or a query. Still sanitized, so a test value is as valid as a real one.
    /// Not available outside tests, production code only gets a `Next` from the
    /// [`Deserialize`] / [`sqlx::Decode`] boundaries. Mirrors `Ciphertext::for_test`.
    pub fn for_test(value: &str) -> Self {
        Next::sanitize(value)
    }
}

impl Default for Next {
    fn default() -> Self {
        Next(NEXT_FALLBACK.to_string())
    }
}

impl std::ops::Deref for Next {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Next {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Next::sanitize(&value))
    }
}

/// The storage column is `text`, so `Next` rides on `String`'s wire format.
impl sqlx::Type<Postgres> for Next {
    fn type_info() -> PgTypeInfo {
        <String as sqlx::Type<Postgres>>::type_info()
    }
    fn compatible(ty: &PgTypeInfo) -> bool {
        <String as sqlx::Type<Postgres>>::compatible(ty)
    }
}

/// Strict on the way out of the database: the column is `NOT NULL` and only ever
/// holds values written through `Next`, so a value that fails validation here is
/// a broken stored invariant, it errors (becoming a 500) instead of silently
/// falling back to [`NEXT_FALLBACK`].
impl<'r> sqlx::Decode<'r, Postgres> for Next {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        Ok(Next::new(<&str as sqlx::Decode<Postgres>>::decode(value)?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_open_redirect_vectors() {
        for evil in [
            "",
            "//evil.example.com",
            "/\\evil.example.com",
            "https://evil.example.com",
            "javascript:alert(1)",
            // backslash or control characters anywhere in the path are rejected.
            "/path\\to/evil",
            "/login\r\nSet-Cookie: x=y",
            "/foo\tbar",
        ] {
            assert_eq!(
                Next::sanitize(evil),
                Next::default(),
                "should reject {evil:?}"
            );
        }
    }

    #[test]
    fn keeps_safe_relative_paths() {
        for ok in ["/", "/dashboard", "/a/b?c=d#e"] {
            assert_eq!(&*Next::sanitize(ok), ok);
        }
    }

    #[test]
    fn new_rejects_unsafe_stored_values() {
        // The strict constructor behind `sqlx::Decode`: an invalid stored value
        // errors (-> 500) rather than falling back, so a broken or hand-edited row
        // is surfaced instead of silently redirected to the fallback.
        assert!(Next::new("//evil").is_err());
        assert!(Next::new("/dashboard").is_ok());
    }
}
