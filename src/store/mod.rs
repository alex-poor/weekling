//! Storage layer: SQLite (WAL) with embedded, forward-only migrations.
//!
//! M0 keeps a single connection behind a mutex — fine for the health check and
//! startup migrations. Heavier query paths will move to `spawn_blocking` / a
//! small pool as the DAV engine lands (see DESIGN.md §4).

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Ordered list of migrations. Append new ones; never edit an applied script.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("migrations/0001_init.sql"))];

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open (or create) the database, set pragmas, and run pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("opening sqlite db at {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .context("applying pragmas")?;

        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate().context("running migrations")?;
        Ok(store)
    }

    /// Cheap liveness probe used by `/healthz`.
    pub fn ping(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.query_row("SELECT 1", [], |_| Ok(()))
            .context("ping query")?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 version    INTEGER PRIMARY KEY,
                 applied_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )?;

        let current: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;

        for (version, sql) in MIGRATIONS {
            if *version > current {
                let tx = conn.transaction()?;
                tx.execute_batch(sql)
                    .with_context(|| format!("applying migration {version}"))?;
                tx.execute(
                    "INSERT INTO schema_migrations (version) VALUES (?1)",
                    [version],
                )?;
                tx.commit()?;
                tracing::info!(version, "applied migration");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_runs_migrations_and_pings() {
        let dir = std::env::temp_dir().join(format!("weekling-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.db");
        let store = Store::open(&db).unwrap();
        store.ping().unwrap();

        // Re-opening is idempotent (migrations already applied).
        let store2 = Store::open(&db).unwrap();
        store2.ping().unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }
}
