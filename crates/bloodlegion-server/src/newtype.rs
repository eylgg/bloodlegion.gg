//! Shared boilerplate for validated `String` newtypes (`Slug`, `HttpsUrl`,
//! `Email`, `Name`, `Username`, ...).

/// Generates the impls every validated `String` newtype shares: `TryFrom<&str>`
/// (delegating to an inherent `fn new(&str) -> Result<Self, $error>`), `Display`,
/// `AsRef<str>`, `Deref<Target = str>`, a `Deserialize` that runs the same
/// validation, and the sqlx `Type`/`Decode` glue that stores it as `text`. Each
/// type still defines its own struct, `new`, validation rules, and `Serialize`
/// derive; this covers only the mechanical parts. Paths are fully qualified so a
/// call site needs no imports beyond the newtype and its error.
macro_rules! string_newtype {
    ($name:ident, $error:ident) => {
        impl ::core::convert::TryFrom<&str> for $name {
            type Error = $error;
            fn try_from(value: &str) -> ::core::result::Result<Self, $error> {
                $name::new(value)
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl ::core::convert::AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl ::core::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                let value =
                    <::std::string::String as ::serde::Deserialize>::deserialize(deserializer)?;
                $name::try_from(value.as_str()).map_err(::serde::de::Error::custom)
            }
        }

        impl ::sqlx::Type<::sqlx::Postgres> for $name {
            fn type_info() -> ::sqlx::postgres::PgTypeInfo {
                <::std::string::String as ::sqlx::Type<::sqlx::Postgres>>::type_info()
            }
            fn compatible(ty: &::sqlx::postgres::PgTypeInfo) -> bool {
                <::std::string::String as ::sqlx::Type<::sqlx::Postgres>>::compatible(ty)
            }
        }

        impl<'r> ::sqlx::Decode<'r, ::sqlx::Postgres> for $name {
            fn decode(
                value: ::sqlx::postgres::PgValueRef<'r>,
            ) -> ::core::result::Result<Self, ::sqlx::error::BoxDynError> {
                let raw = <&str as ::sqlx::Decode<::sqlx::Postgres>>::decode(value)?;
                ::core::result::Result::Ok($name::try_from(raw)?)
            }
        }
    };
}

pub(crate) use string_newtype;
