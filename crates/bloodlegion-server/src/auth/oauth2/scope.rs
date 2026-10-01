use serde::{Deserialize, Deserializer, Serialize};
use sqlx::Postgres;
use sqlx::postgres::{PgTypeInfo, PgValueRef};

#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a valid scope value")]
pub struct Error(String);

/// A validated OAuth2 `scope` value: a space-delimited set of scope tokens.
///
/// Mirrors the `scope`/`scopes` database CHECK
/// (`^([\x21\x23-\x5B\x5D-\x7E]+( [\x21\x23-\x5B\x5D-\x7E]+)*)?$`, <=512 chars),
/// the RFC 6749 section 3.3 grammar: each token is printable ASCII excluding space,
/// double-quote, and backslash, tokens are joined by single spaces with no
/// leading, trailing, or doubled spaces, and the empty set is allowed. Like
/// `slug::Error` this is a plain `thiserror`, not a `Problem`: scope values
/// entering at `/authorize` are validated token-by-token against the known-scope
/// registry (a stricter check), so this type's role is to enforce the stored
/// shape at the database boundary.
///
/// One `Scope` carries the whole `scope` value, which in OAuth2 is a single
/// space-delimited list of tokens (RFC 6749 section 3.3), so one value names many
/// scopes. The per-scope catalog entry (name + consent description) is the
/// separate [`super::ScopeDefinition`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scope(String);

fn is_scope_char(byte: u8) -> bool {
    byte == 0x21 || (0x23..=0x5b).contains(&byte) || (0x5d..=0x7e).contains(&byte)
}

fn is_valid(scopes: &str) -> bool {
    scopes.len() <= 512
        && (scopes.is_empty()
            || scopes
                .split(' ')
                .all(|token| !token.is_empty() && token.bytes().all(is_scope_char)))
}

impl Scope {
    fn new(value: &str) -> Result<Self, Error> {
        if is_valid(value) {
            Ok(Scope(value.to_string()))
        } else {
            Err(Error(value.to_string()))
        }
    }

    /// Whether the set contains `scope` as a whole token.
    pub fn contains(&self, scope: &str) -> bool {
        self.0.split(' ').any(|token| token == scope)
    }
}

impl TryFrom<&str> for Scope {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Error> {
        Scope::new(value)
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for Scope {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Scope::try_from(value.as_str()).map_err(serde::de::Error::custom)
    }
}

impl sqlx::Type<Postgres> for Scope {
    fn type_info() -> PgTypeInfo {
        <String as sqlx::Type<Postgres>>::type_info()
    }
    fn compatible(ty: &PgTypeInfo) -> bool {
        <String as sqlx::Type<Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for Scope {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        Ok(Scope::try_from(<&str as sqlx::Decode<Postgres>>::decode(
            value,
        )?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_database_scope_grammar() {
        for ok in ["", "openid", "openid profile email", "a!#$%"] {
            assert!(Scope::try_from(ok).is_ok(), "{ok:?} should be valid");
        }
        for (bad, why) in [
            (" openid", "leading space"),
            ("openid ", "trailing space"),
            ("openid  profile", "doubled space"),
            ("open\"id", "double-quote not allowed"),
            ("open\\id", "backslash not allowed"),
        ] {
            assert!(
                Scope::try_from(bad).is_err(),
                "{bad:?} should be rejected: {why}"
            );
        }
    }

    #[test]
    fn contains_matches_whole_tokens_only() {
        let scopes = Scope::try_from("openid profile").unwrap();
        assert!(scopes.contains("openid"));
        assert!(scopes.contains("profile"));
        assert!(!scopes.contains("email"));
        assert!(!scopes.contains("open")); // not a substring match
    }
}
