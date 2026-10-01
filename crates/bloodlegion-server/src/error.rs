use crate::{IntoProblemResponse, Problem};

/// The one error type every handler returns, as `crate::Result<T, E>`.
///
/// `External(E)` is a client-facing [`Problem`] rendered as
/// `application/problem+json`; `Internal` is any other failure, logged in full
/// and shown to the client only as a generic 500. The blanket
/// `From<anyhow::Error>` lets `.context(...)?` funnel database, upstream, and
/// I/O failures into `Internal` without a per-call `.map_err`.
pub enum Error<E> {
    External(E),
    Internal(anyhow::Error),
}

impl<E: std::fmt::Debug> std::fmt::Debug for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::External(e) => std::fmt::Debug::fmt(e, f),
            Self::Internal(err) => std::fmt::Debug::fmt(err, f),
        }
    }
}

impl<E> std::fmt::Display for Error<E>
where
    E: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::External(e) => std::fmt::Display::fmt(e, f),
            Self::Internal(err) => std::fmt::Display::fmt(err, f),
        }
    }
}

impl<E> std::error::Error for Error<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::External(e) => e.source(),
            Self::Internal(err) => err.source(),
        }
    }
}

impl<E> From<anyhow::Error> for Error<E> {
    fn from(err: anyhow::Error) -> Self {
        Error::Internal(err)
    }
}

/// Routes each foreign error type into the `Internal` arm, so `db_call().await?`
/// works directly and lands as a logged 500. `.context("...")?` is still preferred
/// where a breadcrumb helps; this is the no-context shortcut.
macro_rules! from {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl<E> From<$ty> for Error<E> {
                fn from(error: $ty) -> Self {
                    Error::Internal(error.into())
                }
            }
        )+
    };
}

// Infrastructure failures are never a client-facing contract. Deliberately omit
// `reqwest::Error`: an upstream failure is often a client-facing 502, so
// modules wrap it explicitly.
from!(sqlx::Error, std::io::Error);

/// The one sanctioned 500 renderer, what [`crate::Error::Internal`] turns into
/// for the client. The `internal` marker opts it out of the derive's ban on a
/// 500 status; every other `Problem` must be a client-facing 4xx (or a
/// 502/503/504 gateway failure).
#[derive(Debug, Problem)]
#[problem(
    internal,
    status = INTERNAL_SERVER_ERROR,
    title = "Internal Server Error",
    detail = "An unexpected internal error occurred."
)]
struct InternalServerError;

impl<E: Problem + std::error::Error> axum::response::IntoResponse for Error<E> {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::External(e) => {
                // A 5xx external problem renders only its static detail to the
                // client (e.g. a 502 when an upstream IdP/Canvas/Forgejo fails);
                // log the real cause so the failure isn't invisible. 4xx are
                // expected client errors and stay quiet.
                if e.status().is_server_error() {
                    tracing::error!(error = %e, "external server error");
                }
                e.into_problem_response()
            }
            Self::Internal(err) => {
                tracing::error!(error = ?err, "internal server error occurred");
                InternalServerError.into_problem_response()
            }
        }
    }
}

/// Maps a database constraint violation onto a client-facing problem, falling
/// back to `Internal` for any other database error or unmapped constraint.
///
/// Centralizes the `sqlx::Error::Database` / `constraint()` inspection and the
/// `Internal` fallthrough that every module's `classify_insert` otherwise
/// repeats; each caller supplies only the constraint-name -> variant table.
pub fn classify_db_error<E>(
    error: sqlx::Error,
    classify: impl FnOnce(&str) -> Option<E>,
) -> Error<E> {
    if let sqlx::Error::Database(db) = &error
        && let Some(external) = db.constraint().and_then(classify)
    {
        return Error::External(external);
    }
    Error::Internal(error.into())
}

pub trait ErrorExt<T, E1> {
    fn map_external<E2>(self, f: impl FnOnce(E1) -> E2) -> Result<T, Error<E2>>;
}

impl<T, E1> ErrorExt<T, E1> for Result<T, Error<E1>> {
    fn map_external<E2>(self, f: impl FnOnce(E1) -> E2) -> Result<T, Error<E2>> {
        self.map_err(|err| match err {
            Error::External(e) => Error::External(f(e)),
            Error::Internal(e) => Error::Internal(e),
        })
    }
}
