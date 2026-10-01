use serde::Serialize;

use crate::Problem;
use crate::newtype::string_newtype;

/// Unlike `slug::Error` / `next::Error`, this is a [`Problem`]: `create_user`
/// validates an incoming email by constructing an `Email`, so a failure renders
/// straight to the client as a 422 "Invalid Email".
#[derive(Debug, thiserror::Error, Problem)]
#[error("{0:?} is not a valid email address")]
#[problem(
    status = UNPROCESSABLE_ENTITY,
    title = "Invalid Email",
    detail = "The email address is invalid."
)]
pub struct Error(String);

/// A syntactically-acceptable email address. Mirrors the `user_emails_email_check` database
/// constraint (3-256 chars, every byte printable ASCII with no space: `^[\x21-\x7E]+$`) and
/// *nothing more*. This is deliberately the same loose shape the schema enforces, not RFC 5322.
/// Real validity is proven by sending a verification mail, not by parsing. The printable-ASCII rule
/// is also what lets the database fold `lower(email)` collation-independently for the uniqueness
/// index, so keeping the Rust check identical preserves that invariant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Email(String);

fn is_valid(email: &str) -> bool {
    (3..=256).contains(&email.len()) && email.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

impl Email {
    fn new(value: &str) -> Result<Self, Error> {
        if is_valid(value) {
            Ok(Email(value.to_string()))
        } else {
            Err(Error(value.to_string()))
        }
    }
}

string_newtype!(Email, Error);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_database_email_check() {
        for ok in ["a@b.co", "User.Name+tag@example.com", "x@y"] {
            assert!(Email::try_from(ok).is_ok(), "{ok:?} should be valid");
        }
        for (bad, why) in [
            ("ab", "too short"),
            ("a b@c.com", "contains a space"),
            ("a\t@c.com", "contains a control char"),
            ("café@x.com", "non-ASCII"),
            ("a@b\u{7f}", "DEL is not printable"),
        ] {
            assert!(
                Email::try_from(bad).is_err(),
                "{bad:?} should be rejected: {why}"
            );
        }
    }

    #[test]
    fn serializes_as_the_bare_string() {
        let email = Email::try_from("user@example.com").unwrap();
        assert_eq!(
            serde_json::to_string(&email).unwrap(),
            "\"user@example.com\""
        );
        assert_eq!(&*email, "user@example.com");
    }
}
