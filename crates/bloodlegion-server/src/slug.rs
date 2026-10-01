use serde::Serialize;

use crate::newtype::string_newtype;

#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a valid slug")]
pub struct Error(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Slug(String);

fn is_valid(slug: &str) -> bool {
    slug.len() >= 3
        && slug.len() <= 64
        && slug.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl Slug {
    fn new(value: &str) -> Result<Self, Error> {
        if is_valid(value) {
            Ok(Slug(value.to_string()))
        } else {
            Err(Error(value.to_string()))
        }
    }
}

string_newtype!(Slug, Error);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_its_boundaries() {
        for ok in ["abc", "a-b0", "2026-fall", &"a".repeat(64)] {
            assert!(Slug::try_from(ok).is_ok(), "{ok:?} should be valid");
        }
        for (bad, why) in [
            ("ab", "too short"),
            (&"a".repeat(65), "too long"),
            ("Abc", "lowercase only"),
            ("a_b", "no underscore"),
            ("a.b", "no dot"),
        ] {
            assert!(
                Slug::try_from(bad).is_err(),
                "{bad:?} should be rejected: {why}"
            );
        }
    }

    #[test]
    fn serializes_as_the_bare_string() {
        let slug = Slug::try_from("example").unwrap();
        assert_eq!(serde_json::to_string(&slug).unwrap(), "\"example\"");
        assert_eq!(&*slug, "example");
        assert_eq!(slug.to_string(), "example");
    }
}
