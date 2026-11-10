//! Storage layer: SQLite (WAL) with embedded migrations and the M1 repositories
//! (principals, credentials, calendars, calendar objects).
//!
//! A single connection behind a mutex is enough for M1's workload. Hot query
//! paths move to `spawn_blocking` / a small pool as load grows (DESIGN.md §4).

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("migrations/0001_init.sql")),
    (2, include_str!("migrations/0002_sharing.sql")),
    (3, include_str!("migrations/0003_push.sql")),
];

// ---- domain types -------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Principal {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    #[allow(dead_code)] // role checks land with the admin API (M2)
    pub is_admin: bool,
}

#[derive(Debug, Clone)]
pub struct Calendar {
    pub id: String,
    #[allow(dead_code)] // used once sharing/ACLs land (M3)
    pub owner_id: String,
    pub display_name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub components: String,
    pub sync_token: i64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // has_rrule/change_seq feed indexing & sync, not yet read off the struct
pub struct CalObject {
    pub uid: String,
    pub etag: String,
    pub ical_raw: String,
    pub dtstart: Option<String>,
    pub dtend: Option<String>,
    pub component: Option<String>,
    pub has_rrule: bool,
    pub change_seq: i64,
}

/// A principal's effective access to a calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Owner,
    ReadWrite,
    Read,
    FreeBusy,
}

impl Access {
    pub fn can_read(self) -> bool {
        matches!(self, Access::Owner | Access::ReadWrite | Access::Read)
    }
    pub fn can_write(self) -> bool {
        matches!(self, Access::Owner | Access::ReadWrite)
    }
    fn from_share(s: &str) -> Option<Access> {
        match s {
            "read" => Some(Access::Read),
            "read-write" => Some(Access::ReadWrite),
            "freebusy" => Some(Access::FreeBusy),
            _ => None,
        }
    }
}

/// Input for an upsert of a calendar object.
pub struct NewObject<'a> {
    pub collection_id: &'a str,
    pub uid: &'a str,
    pub etag: &'a str,
    pub ical_raw: &'a str,
    pub dtstart: Option<&'a str>,
    pub dtend: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub status: Option<&'a str>,
    pub component: Option<&'a str>,
    pub has_rrule: bool,
}

/// Metadata about an app password (never the secret itself).
#[derive(Debug, Clone)]
pub struct CredentialInfo {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

/// A stored WebDAV-Push subscription.
#[derive(Debug, Clone)]
#[allow(dead_code)] // collection_id/expires_at are read by the send path (M4 part 2)
pub struct PushSub {
    pub id: String,
    pub collection_id: String,
    pub push_resource: String,
    pub p256dh: String,
    pub auth_secret: String,
    pub expires_at: String,
}

/// One change for a sync-collection REPORT.
pub struct SyncChange {
    pub uid: String,
    pub deleted: bool,
    pub etag: Option<String>,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
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

    pub fn ping(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.query_row("SELECT 1", [], |_| Ok(())).context("ping")?;
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

    // ---- principals ------------------------------------------------------

    pub fn create_user(&self, id: &str, display_name: &str, is_admin: bool) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT INTO principals (id, kind, display_name, is_admin)
             VALUES (?1, 'user', ?2, ?3)",
            params![id, display_name, is_admin as i64],
        )?;
        Ok(())
    }

    pub fn get_principal(&self, id: &str) -> Result<Option<Principal>> {
        let conn = self.conn.lock().expect("poisoned");
        let p = conn
            .query_row(
                "SELECT id, display_name, email, is_admin FROM principals WHERE id = ?1",
                [id],
                |r| {
                    Ok(Principal {
                        id: r.get(0)?,
                        display_name: r.get(1)?,
                        email: r.get(2)?,
                        is_admin: r.get::<_, i64>(3)? != 0,
                    })
                },
            )
            .optional()?;
        Ok(p)
    }

    // ---- credentials (app passwords) -------------------------------------

    pub fn add_app_password(
        &self,
        id: &str,
        principal_id: &str,
        label: &str,
        hash: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT INTO credentials (id, principal_id, kind, label, secret_hash)
             VALUES (?1, ?2, 'app_password', ?3, ?4)",
            params![id, principal_id, label, hash],
        )?;
        Ok(())
    }

    /// All app-password hashes for a principal, as (credential_id, hash).
    pub fn app_password_hashes(&self, principal_id: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, secret_hash FROM credentials
             WHERE principal_id = ?1 AND kind = 'app_password'",
        )?;
        let rows = stmt
            .query_map([principal_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn touch_credential(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "UPDATE credentials SET last_used_at = datetime('now') WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn list_app_passwords(&self, principal_id: &str) -> Result<Vec<CredentialInfo>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, label, created_at, last_used_at FROM credentials
             WHERE principal_id = ?1 AND kind = 'app_password' ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([principal_id], |r| {
                Ok(CredentialInfo {
                    id: r.get(0)?,
                    label: r.get(1)?,
                    created_at: r.get(2)?,
                    last_used_at: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Revoke an app password owned by `principal_id`. Returns true if removed.
    pub fn delete_app_password(&self, principal_id: &str, id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("poisoned");
        let n = conn.execute(
            "DELETE FROM credentials
             WHERE id = ?1 AND principal_id = ?2 AND kind = 'app_password'",
            params![id, principal_id],
        )?;
        Ok(n > 0)
    }

    // ---- account password (management UI) --------------------------------

    /// Set (or replace) the single account password for a principal.
    pub fn set_password(&self, id: &str, principal_id: &str, hash: &str) -> Result<()> {
        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM credentials WHERE principal_id = ?1 AND kind = 'password'",
            [principal_id],
        )?;
        tx.execute(
            "INSERT INTO credentials (id, principal_id, kind, label, secret_hash)
             VALUES (?1, ?2, 'password', 'account password', ?3)",
            params![id, principal_id, hash],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn password_hash(&self, principal_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("poisoned");
        let h = conn
            .query_row(
                "SELECT secret_hash FROM credentials
                 WHERE principal_id = ?1 AND kind = 'password'",
                [principal_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(h)
    }

    // ---- sessions (management UI) ----------------------------------------

    pub fn create_session(
        &self,
        id: &str,
        principal_id: &str,
        csrf: &str,
        ttl_hours: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT INTO sessions (id, principal_id, csrf, expires_at)
             VALUES (?1, ?2, ?3, datetime('now', ?4))",
            params![id, principal_id, csrf, format!("+{ttl_hours} hours")],
        )?;
        Ok(())
    }

    /// Resolve a live (unexpired) session to (principal_id, csrf).
    pub fn get_session(&self, id: &str) -> Result<Option<(String, String)>> {
        let conn = self.conn.lock().expect("poisoned");
        let row = conn
            .query_row(
                "SELECT principal_id, csrf FROM sessions
                 WHERE id = ?1 AND expires_at > datetime('now')",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute("DELETE FROM sessions WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- calendars -------------------------------------------------------

    pub fn create_calendar(
        &self,
        id: &str,
        owner_id: &str,
        display_name: &str,
        components: &str,
        color: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned");
        conn.execute(
            "INSERT INTO collections (id, owner_id, display_name, components, color, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, owner_id, display_name, components, color, description],
        )?;
        Ok(())
    }

    pub fn get_calendar(&self, owner_id: &str, id: &str) -> Result<Option<Calendar>> {
        let conn = self.conn.lock().expect("poisoned");
        let c = conn
            .query_row(
                "SELECT id, owner_id, display_name, description, color, components, sync_token
                 FROM collections WHERE owner_id = ?1 AND id = ?2",
                params![owner_id, id],
                map_calendar,
            )
            .optional()?;
        Ok(c)
    }

    pub fn list_calendars(&self, owner_id: &str) -> Result<Vec<Calendar>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, owner_id, display_name, description, color, components, sync_token
             FROM collections WHERE owner_id = ?1 ORDER BY display_name",
        )?;
        let rows = stmt
            .query_map([owner_id], map_calendar)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Update mutable calendar properties (only the Some fields). Returns true
    /// if the calendar exists and is owned by `owner_id`.
    pub fn update_calendar(
        &self,
        owner_id: &str,
        id: &str,
        display_name: Option<&str>,
        color: Option<&str>,
        description: Option<&str>,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("poisoned");
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM collections WHERE owner_id = ?1 AND id = ?2",
                params![owner_id, id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(false);
        }
        if let Some(v) = display_name {
            conn.execute(
                "UPDATE collections SET display_name = ?1 WHERE owner_id = ?2 AND id = ?3",
                params![v, owner_id, id],
            )?;
        }
        if let Some(v) = color {
            conn.execute(
                "UPDATE collections SET color = ?1 WHERE owner_id = ?2 AND id = ?3",
                params![v, owner_id, id],
            )?;
        }
        if let Some(v) = description {
            conn.execute(
                "UPDATE collections SET description = ?1 WHERE owner_id = ?2 AND id = ?3",
                params![v, owner_id, id],
            )?;
        }
        Ok(true)
    }

    /// Delete a calendar (and, via cascade, its objects and tombstones).
    pub fn delete_calendar(&self, owner_id: &str, id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("poisoned");
        let n = conn.execute(
            "DELETE FROM collections WHERE owner_id = ?1 AND id = ?2",
            params![owner_id, id],
        )?;
        Ok(n > 0)
    }

    // ---- sharing ---------------------------------------------------------

    /// Share a calendar the owner owns with another principal. Returns false if
    /// the owner doesn't own it, the grantee doesn't exist, or they're the same.
    pub fn share_calendar(
        &self,
        owner_id: &str,
        cal_id: &str,
        grantee_id: &str,
        access: &str,
    ) -> Result<bool> {
        if Access::from_share(access).is_none() || owner_id == grantee_id {
            return Ok(false);
        }
        let conn = self.conn.lock().expect("poisoned");
        let owns = conn
            .query_row(
                "SELECT 1 FROM collections WHERE id = ?1 AND owner_id = ?2",
                params![cal_id, owner_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let grantee_ok = conn
            .query_row(
                "SELECT 1 FROM principals WHERE id = ?1",
                [grantee_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !owns || !grantee_ok {
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO shares (collection_id, grantee_id, access) VALUES (?1, ?2, ?3)
             ON CONFLICT (collection_id, grantee_id) DO UPDATE SET access = excluded.access",
            params![cal_id, grantee_id, access],
        )?;
        Ok(true)
    }

    pub fn unshare(&self, owner_id: &str, cal_id: &str, grantee_id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("poisoned");
        let n = conn.execute(
            "DELETE FROM shares WHERE grantee_id = ?1 AND collection_id IN
                 (SELECT id FROM collections WHERE id = ?2 AND owner_id = ?3)",
            params![grantee_id, cal_id, owner_id],
        )?;
        Ok(n > 0)
    }

    /// (grantee_id, access) rows for a calendar the owner owns.
    pub fn list_shares(&self, owner_id: &str, cal_id: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT grantee_id, access FROM shares WHERE collection_id IN
                 (SELECT id FROM collections WHERE id = ?1 AND owner_id = ?2)
             ORDER BY grantee_id",
        )?;
        let rows = stmt
            .query_map(params![cal_id, owner_id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Calendars shared *to* this principal, each with the granted access.
    pub fn shared_with(&self, grantee_id: &str) -> Result<Vec<(Calendar, Access)>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT c.id, c.owner_id, c.display_name, c.description, c.color, c.components,
                    c.sync_token, s.access
             FROM shares s JOIN collections c ON c.id = s.collection_id
             WHERE s.grantee_id = ?1 ORDER BY c.display_name",
        )?;
        let rows = stmt
            .query_map([grantee_id], |r| {
                let cal = Calendar {
                    id: r.get(0)?,
                    owner_id: r.get(1)?,
                    display_name: r.get(2)?,
                    description: r.get(3)?,
                    color: r.get(4)?,
                    components: r.get(5)?,
                    sync_token: r.get(6)?,
                };
                let access = Access::from_share(&r.get::<_, String>(7)?).unwrap_or(Access::Read);
                Ok((cal, access))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// A principal's effective access to `owner_id`'s calendar `cal_id`, or None
    /// if the calendar doesn't exist or isn't shared with them.
    pub fn access_for(
        &self,
        principal_id: &str,
        owner_id: &str,
        cal_id: &str,
    ) -> Result<Option<Access>> {
        let conn = self.conn.lock().expect("poisoned");
        let exists = conn
            .query_row(
                "SELECT 1 FROM collections WHERE id = ?1 AND owner_id = ?2",
                params![cal_id, owner_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(None);
        }
        if principal_id == owner_id {
            return Ok(Some(Access::Owner));
        }
        let acc: Option<String> = conn
            .query_row(
                "SELECT access FROM shares WHERE collection_id = ?1 AND grantee_id = ?2",
                params![cal_id, principal_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(acc.and_then(|a| Access::from_share(&a)))
    }

    // ---- push subscriptions (WebDAV-Push) --------------------------------

    /// Register (or update, keyed by collection+endpoint) a push subscription.
    /// Returns the registration id.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_subscription(
        &self,
        id: &str,
        collection_id: &str,
        principal_id: &str,
        push_resource: &str,
        p256dh: &str,
        auth_secret: &str,
        expires_modifier: &str, // e.g. "+30 days", applied to datetime('now')
    ) -> Result<String> {
        let conn = self.conn.lock().expect("poisoned");
        let effective = conn.query_row(
            "INSERT INTO push_subscriptions
                 (id, collection_id, principal_id, push_resource, p256dh, auth_secret, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now', ?7))
             ON CONFLICT (collection_id, push_resource) DO UPDATE SET
                 p256dh = excluded.p256dh, auth_secret = excluded.auth_secret,
                 expires_at = datetime('now', ?7), principal_id = excluded.principal_id
             RETURNING id",
            params![
                id,
                collection_id,
                principal_id,
                push_resource,
                p256dh,
                auth_secret,
                expires_modifier
            ],
            |r| r.get(0),
        )?;
        Ok(effective)
    }

    /// The principal that owns a subscription, if it exists.
    pub fn subscription_owner(&self, id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("poisoned");
        let owner = conn
            .query_row(
                "SELECT principal_id FROM push_subscriptions WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(owner)
    }

    pub fn delete_subscription(&self, id: &str, principal_id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("poisoned");
        let n = conn.execute(
            "DELETE FROM push_subscriptions WHERE id = ?1 AND principal_id = ?2",
            params![id, principal_id],
        )?;
        Ok(n > 0)
    }

    /// Live (unexpired) subscriptions for a collection.
    pub fn active_subscriptions(&self, collection_id: &str) -> Result<Vec<PushSub>> {
        let conn = self.conn.lock().expect("poisoned");
        let mut stmt = conn.prepare(
            "SELECT id, collection_id, push_resource, p256dh, auth_secret, expires_at
             FROM push_subscriptions
             WHERE collection_id = ?1 AND expires_at > datetime('now')",
        )?;
        let rows = stmt
            .query_map([collection_id], |r| {
                Ok(PushSub {
                    id: r.get(0)?,
                    collection_id: r.get(1)?,
                    push_resource: r.get(2)?,
                    p256dh: r.get(3)?,
                    auth_secret: r.get(4)?,
                    expires_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ---- calendar objects ------------------------------------------------

    pub fn get_object(&self, cal_id: &str, uid: &str) -> Result<Option<CalObject>> {
        let conn = self.conn.lock().expect("poisoned");
        let sql =
            format!("SELECT {OBJECT_COLS} FROM resources WHERE collection_id = ?1 AND uid = ?2");
        let o = conn
            .query_row(&sql, params![cal_id, uid], map_object)
            .optional()?;
        Ok(o)
    }

    /// List objects, optionally restricted to one component type (VEVENT/VTODO).
    pub fn list_objects(&self, cal_id: &str, component: Option<&str>) -> Result<Vec<CalObject>> {
        let conn = self.conn.lock().expect("poisoned");
        let sql = format!(
            "SELECT {OBJECT_COLS} FROM resources
             WHERE collection_id = ?1 AND (?2 IS NULL OR component IS NULL OR component = ?2)
             ORDER BY uid"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![cal_id, component], map_object)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Objects overlapping [start, end) (half-open, inclusive-of-undated-end),
    /// optionally restricted to one component type. Recurring objects (RRULE)
    /// are always included — the client re-filters.
    pub fn list_objects_in_range(
        &self,
        cal_id: &str,
        start: &str,
        end: &str,
        component: Option<&str>,
    ) -> Result<Vec<CalObject>> {
        let conn = self.conn.lock().expect("poisoned");
        let sql = format!(
            "SELECT {OBJECT_COLS} FROM resources
             WHERE collection_id = ?1
               AND (?4 IS NULL OR component IS NULL OR component = ?4)
               AND (has_rrule = 1
                    OR (dtstart IS NOT NULL
                        AND dtstart < ?3
                        AND COALESCE(dtend, dtstart) >= ?2))
             ORDER BY uid"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![cal_id, start, end, component], map_object)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Upsert an object, bumping the collection's sync token. Returns the new
    /// sync token (change_seq) assigned to this write.
    pub fn put_object(&self, obj: &NewObject, row_id: &str) -> Result<i64> {
        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction()?;
        let seq = bump_seq(&tx, obj.collection_id)?;
        tx.execute(
            "INSERT INTO resources
                 (id, collection_id, uid, etag, ical_raw, dtstart, dtend, summary,
                  status, component, has_rrule, change_seq, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, datetime('now'))
             ON CONFLICT (collection_id, uid) DO UPDATE SET
                 etag = excluded.etag, ical_raw = excluded.ical_raw,
                 dtstart = excluded.dtstart, dtend = excluded.dtend,
                 summary = excluded.summary, status = excluded.status,
                 component = excluded.component,
                 has_rrule = excluded.has_rrule, change_seq = excluded.change_seq,
                 updated_at = datetime('now')",
            params![
                row_id,
                obj.collection_id,
                obj.uid,
                obj.etag,
                obj.ical_raw,
                obj.dtstart,
                obj.dtend,
                obj.summary,
                obj.status,
                obj.component,
                obj.has_rrule as i64,
                seq,
            ],
        )?;
        // Re-adding a previously deleted uid clears its tombstone.
        tx.execute(
            "DELETE FROM deletions WHERE collection_id = ?1 AND uid = ?2",
            params![obj.collection_id, obj.uid],
        )?;
        tx.commit()?;
        Ok(seq)
    }

    /// Current sync token (version) of a collection.
    pub fn collection_sync_token(&self, cal_id: &str) -> Result<i64> {
        let conn = self.conn.lock().expect("poisoned");
        let t = conn.query_row(
            "SELECT sync_token FROM collections WHERE id = ?1",
            [cal_id],
            |r| r.get(0),
        )?;
        Ok(t)
    }

    /// Delete an object, recording a tombstone. Returns true if it existed.
    pub fn delete_object(&self, cal_id: &str, uid: &str) -> Result<bool> {
        let mut conn = self.conn.lock().expect("poisoned");
        let tx = conn.transaction()?;
        let seq = bump_seq(&tx, cal_id)?;
        let affected = tx.execute(
            "DELETE FROM resources WHERE collection_id = ?1 AND uid = ?2",
            params![cal_id, uid],
        )?;
        if affected > 0 {
            tx.execute(
                "INSERT INTO deletions (collection_id, uid, change_seq) VALUES (?1, ?2, ?3)
                 ON CONFLICT (collection_id, uid) DO UPDATE SET change_seq = excluded.change_seq",
                params![cal_id, uid, seq],
            )?;
        }
        tx.commit()?;
        Ok(affected > 0)
    }

    /// Changes since `since` (exclusive) for sync-collection. Returns (changes,
    /// new_token). A `since` of 0 enumerates everything (initial sync).
    pub fn changes_since(&self, cal_id: &str, since: i64) -> Result<(Vec<SyncChange>, i64)> {
        let conn = self.conn.lock().expect("poisoned");
        let current: i64 = conn.query_row(
            "SELECT sync_token FROM collections WHERE id = ?1",
            [cal_id],
            |r| r.get(0),
        )?;
        let mut out = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT uid, etag FROM resources WHERE collection_id = ?1 AND change_seq > ?2",
        )?;
        let added = stmt.query_map(params![cal_id, since], |r| {
            Ok(SyncChange {
                uid: r.get(0)?,
                etag: Some(r.get(1)?),
                deleted: false,
            })
        })?;
        for c in added {
            out.push(c?);
        }
        let mut stmt =
            conn.prepare("SELECT uid FROM deletions WHERE collection_id = ?1 AND change_seq > ?2")?;
        let removed = stmt.query_map(params![cal_id, since], |r| {
            Ok(SyncChange {
                uid: r.get(0)?,
                etag: None,
                deleted: true,
            })
        })?;
        for c in removed {
            out.push(c?);
        }
        Ok((out, current))
    }
}

fn bump_seq(tx: &rusqlite::Transaction, cal_id: &str) -> rusqlite::Result<i64> {
    tx.query_row(
        "UPDATE collections SET sync_token = sync_token + 1 WHERE id = ?1 RETURNING sync_token",
        [cal_id],
        |r| r.get(0),
    )
}

fn map_calendar(r: &rusqlite::Row) -> rusqlite::Result<Calendar> {
    Ok(Calendar {
        id: r.get(0)?,
        owner_id: r.get(1)?,
        display_name: r.get(2)?,
        description: r.get(3)?,
        color: r.get(4)?,
        components: r.get(5)?,
        sync_token: r.get(6)?,
    })
}

const OBJECT_COLS: &str = "uid, etag, ical_raw, dtstart, dtend, component, has_rrule, change_seq";

fn map_object(r: &rusqlite::Row) -> rusqlite::Result<CalObject> {
    Ok(CalObject {
        uid: r.get(0)?,
        etag: r.get(1)?,
        ical_raw: r.get(2)?,
        dtstart: r.get(3)?,
        dtend: r.get(4)?,
        component: r.get(5)?,
        has_rrule: r.get::<_, i64>(6)? != 0,
        change_seq: r.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_store() -> Store {
        // A throwaway file-backed db (WAL needs a real file).
        let dir = std::env::temp_dir().join(format!(
            "wk-store-{}-{}",
            std::process::id(),
            crate::util::new_id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open(&dir.join("t.db")).unwrap()
    }

    #[test]
    fn calendar_object_lifecycle_and_sync() {
        let s = mem_store();
        s.create_user("alice", "Alice", true).unwrap();
        s.create_calendar("cal1", "alice", "Home", "VEVENT", Some("#5b8def"), None)
            .unwrap();

        let obj = NewObject {
            collection_id: "cal1",
            uid: "e1",
            etag: "\"v1\"",
            ical_raw: "BEGIN:VCALENDAR\nEND:VCALENDAR",
            dtstart: Some("20260115T090000"),
            dtend: Some("20260115T100000"),
            summary: Some("Sync"),
            status: Some("CONFIRMED"),
            component: Some("VEVENT"),
            has_rrule: false,
        };
        let seq1 = s.put_object(&obj, "row1").unwrap();
        assert_eq!(seq1, 1);
        assert!(s.get_object("cal1", "e1").unwrap().is_some());

        let (changes, token) = s.changes_since("cal1", 0).unwrap();
        assert_eq!(token, 1);
        assert_eq!(changes.len(), 1);
        assert!(!changes[0].deleted);

        assert!(s.delete_object("cal1", "e1").unwrap());
        let (changes, token) = s.changes_since("cal1", 1).unwrap();
        assert_eq!(token, 2);
        assert_eq!(changes.len(), 1);
        assert!(changes[0].deleted);
    }

    #[test]
    fn range_query_filters_by_time() {
        let s = mem_store();
        s.create_user("bob", "Bob", false).unwrap();
        s.create_calendar("c", "bob", "Cal", "VEVENT", None, None)
            .unwrap();
        let mk = |uid, start, end| NewObject {
            collection_id: "c",
            uid,
            etag: "\"e\"",
            ical_raw: "x",
            dtstart: Some(start),
            dtend: Some(end),
            summary: None,
            status: None,
            component: Some("VEVENT"),
            has_rrule: false,
        };
        s.put_object(&mk("jan", "20260115T090000", "20260115T100000"), "r1")
            .unwrap();
        s.put_object(&mk("mar", "20260315T090000", "20260315T100000"), "r2")
            .unwrap();

        let hits = s
            .list_objects_in_range("c", "20260101T000000", "20260201T000000", None)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].uid, "jan");
    }

    #[test]
    fn sharing_grants_access() {
        let s = mem_store();
        s.create_user("alice", "Alice", false).unwrap();
        s.create_user("bob", "Bob", false).unwrap();
        s.create_calendar("home", "alice", "Home", "VEVENT", None, None)
            .unwrap();

        // No access before sharing.
        assert_eq!(s.access_for("bob", "alice", "home").unwrap(), None);
        assert_eq!(
            s.access_for("alice", "alice", "home").unwrap(),
            Some(Access::Owner)
        );

        assert!(
            s.share_calendar("alice", "home", "bob", "read-write")
                .unwrap()
        );
        assert_eq!(
            s.access_for("bob", "alice", "home").unwrap(),
            Some(Access::ReadWrite)
        );
        assert_eq!(s.shared_with("bob").unwrap().len(), 1);

        // Bob cannot share Alice's calendar onward.
        assert!(!s.share_calendar("bob", "home", "alice", "read").unwrap());

        assert!(s.unshare("alice", "home", "bob").unwrap());
        assert_eq!(s.access_for("bob", "alice", "home").unwrap(), None);
    }
}
