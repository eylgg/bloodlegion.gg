mod db;

use std::time::Duration;

use sqlx::postgres::PgPool;

/// How often the cleaner sweeps expired/stale rows.
const CLEANER_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Periodically deletes expired ephemeral rows (sessions, in-flight auth
/// requests, authorization codes, refresh tokens), stale login-throttle counters,
/// and retired signing keys. One instance runs per tick, guarded by a Postgres
/// advisory lock (see [`db::sweep`]).
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(CLEANER_INTERVAL);
        loop {
            ticker.tick().await;
            match db::sweep(&pool).await {
                Ok(rows) if rows > 0 => tracing::info!(rows, "cleaner: swept expired rows"),
                Ok(_) => {}
                Err(err) => tracing::warn!(error = %err, "cleaner: sweep failed"),
            }
        }
    });
}
