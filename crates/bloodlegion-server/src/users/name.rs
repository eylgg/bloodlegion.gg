use serde::Serialize;

use crate::Problem;
use crate::newtype::string_newtype;

/// Unlike `slug::Error` / `next::Error`, this is a [`Problem`]: `create_user`
/// validates an incoming name by constructing a `Name`, so a failure renders
/// straight to the client as a 422 "Invalid Name".
#[derive(Debug, thiserror::Error, Problem)]
#[error("{0:?} is not a valid name")]
#[problem(
    status = UNPROCESSABLE_ENTITY,
    title = "Invalid Name",
    detail = "A first or last name must be 1 to 64 characters."
)]
pub struct Error(String);

/// A person's first or last name. Mirrors the `users_first_name_check` /
/// `users_last_name_check` constraints: 1-64 characters, and *nothing else*,
/// names are free-form Unicode, so there is no charset rule. Length is counted in
/// characters (Unicode scalar values), matching Postgres `char_length`, so a name
/// of 64 non-ASCII characters is valid. Construction **rejects** an over-long (or
/// empty) name rather than truncating it, so an identity provider sending an
/// oversized display name fails the login loudly instead of silently losing data
/// or tripping the database CHECK as a 500.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Name(String);

fn is_valid(name: &str) -> bool {
    (1..=64).contains(&name.chars().count())
}

impl Name {
    fn new(value: &str) -> Result<Self, Error> {
        if is_valid(value) {
            Ok(Name(value.to_string()))
        } else {
            Err(Error(value.to_string()))
        }
    }
}

string_newtype!(Name, Error);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_database_name_check() {
        // 1-64 characters of any content, counted as Unicode scalar values.
        for ok in ["A", "José", "日本語の名前", &"x".repeat(64)] {
            assert!(Name::try_from(ok).is_ok(), "{ok:?} should be valid");
        }
        for (bad, why) in [
            ("", "empty"),
            (&"x".repeat(65), "over 64 characters"),
            (
                &"あ".repeat(65),
                "over 64 characters (non-ASCII counts by char)",
            ),
        ] {
            assert!(
                Name::try_from(bad).is_err(),
                "{bad:?} should be rejected: {why}"
            );
        }
        // 64 multibyte characters is 192 bytes but only 64 chars, still valid.
        assert!(Name::try_from("あ".repeat(64).as_str()).is_ok());
    }
}
