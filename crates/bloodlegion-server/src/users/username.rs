use serde::Serialize;

use crate::Problem;
use crate::newtype::string_newtype;

/// Unlike `slug::Error` / `next::Error`, this is a [`Problem`]: `create_user`
/// validates an incoming username by constructing a `Username`, so a failure
/// renders straight to the client as a 422 "Invalid Username" (wired in
/// transparently via `users::Error::InvalidUsername`). The detail describes the
/// rule rather than echoing the rejected input.
#[derive(Debug, thiserror::Error, Problem)]
#[error("{0:?} is not a valid username")]
#[problem(
    status = UNPROCESSABLE_ENTITY,
    title = "Invalid Username",
    detail = "The username must be 3 to 32 characters: a letter, then letters and digits."
)]
pub struct Error(String);

/// A validated account username. Mirrors the `users_username_check` database
/// constraint (`^[A-Za-z][A-Za-z0-9]*$`, 3-32 chars), so constructing one is the
/// single Rust-side check that a string is a usable username, the same shape
/// the database enforces, and a URL-path-safe charset (no `/`, `.`, `%`, or
/// whitespace). Capitals are kept for display; [`Username::normalized`] is the
/// lowercase form that `users.username_normalized` carries for uniqueness and lookups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Username(String);

fn is_valid(username: &str) -> bool {
    username.len() >= 3
        && username.len() <= 32
        && username.starts_with(|c: char| c.is_ascii_alphabetic())
        && username.chars().all(|c| c.is_ascii_alphanumeric())
}

impl Username {
    fn new(value: &str) -> Result<Self, Error> {
        if is_valid(value) {
            Ok(Username(value.to_string()))
        } else {
            Err(Error(value.to_string()))
        }
    }

    /// The lowercase form, matching `users.username_normalized`: what sign-in, lookups, and
    /// uniqueness compare on, so `Thrall` and `thrall` are the same account.
    pub fn normalized(&self) -> String {
        self.0.to_ascii_lowercase()
    }
}

string_newtype!(Username, Error);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_the_database_username_shape() {
        for ok in ["abc", "Thrall", "student01", &"a".repeat(32)] {
            assert!(Username::try_from(ok).is_ok(), "{ok:?} should be valid");
        }
        assert_eq!(Username::try_from("Thrall").unwrap().normalized(), "thrall");
        for (bad, why) in [
            ("ab", "too short"),
            (&"a".repeat(33), "too long"),
            ("1abc", "must start with a letter"),
            ("a_b", "no underscore"),
            ("a-b", "no dash"),
            ("ab.c", "no dot"),
            ("ab/c", "no slash (path-traversal vector)"),
            ("ab c", "no whitespace"),
            ("ab%63", "no percent-encoding"),
            ("abc\u{0}", "no control characters"),
        ] {
            assert!(
                Username::try_from(bad).is_err(),
                "{bad:?} should be rejected: {why}"
            );
        }
    }

    #[test]
    fn serializes_as_the_bare_string() {
        let username = Username::try_from("student01").unwrap();
        assert_eq!(serde_json::to_string(&username).unwrap(), "\"student01\"");
        assert_eq!(&*username, "student01");
        assert_eq!(username.to_string(), "student01");
    }
}
