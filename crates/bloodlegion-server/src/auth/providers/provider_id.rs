/// An `auth_providers.id`: the shared envelope row behind both a SAML and an
/// OAuth2 provider. A newtype over the raw `i64` so a provider id cannot be
/// silently passed where some other table's `bigint` id is expected.
/// `#[sqlx(transparent)]` makes it bind and decode exactly like an `i64` (queries
/// selecting it into a `ProviderId` field still need an `AS "col: ProviderId"`
/// cast); `#[serde(transparent)]` keeps it a bare number on the wire.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, serde::Serialize, serde::Deserialize,
)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct ProviderId(pub i64);

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
