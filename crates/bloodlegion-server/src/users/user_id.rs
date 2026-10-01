/// A `users.id`. A newtype over the raw `i64` so a user id cannot be silently
/// passed where some other table's `bigint` id is expected. `#[sqlx(transparent)]`
/// makes it bind and decode exactly like an `i64` (queries selecting it into a
/// `UserId` field still need an `AS "col: UserId"` cast); `#[serde(transparent)]`
/// keeps it a bare number on the wire.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type, serde::Serialize, serde::Deserialize,
)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct UserId(pub i64);

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
