//! First-run bootstrap. On a fresh install (no users yet), create the admin
//! account from the environment so a plain `docker run` / compose is enough —
//! no `docker exec` or CLI step. Idempotent: does nothing once a user exists.

use anyhow::{Context, Result};

use crate::store::Store;
use crate::{auth, util};

pub fn run(store: &Store) -> Result<()> {
    if !store.list_users()?.is_empty() {
        return Ok(()); // already set up
    }

    let Some(username) = env_nonempty("WEEKLING_ADMIN_USERNAME") else {
        tracing::warn!(
            "no users yet — set WEEKLING_ADMIN_USERNAME and WEEKLING_ADMIN_PASSWORD (or \
             WEEKLING_ADMIN_PASSWORD_FILE) to create an admin on first start, or run \
             `weekling user add`"
        );
        return Ok(());
    };
    let Some(password) = admin_password() else {
        tracing::warn!(
            %username,
            "WEEKLING_ADMIN_USERNAME is set but no WEEKLING_ADMIN_PASSWORD[_FILE]; skipping bootstrap"
        );
        return Ok(());
    };
    if password.chars().count() < 8 {
        tracing::warn!("WEEKLING_ADMIN_PASSWORD is too short (minimum 8); skipping bootstrap");
        return Ok(());
    }

    let display = env_nonempty("WEEKLING_ADMIN_DISPLAY").unwrap_or_else(|| username.clone());
    let email = env_nonempty("WEEKLING_ADMIN_EMAIL");

    store
        .create_user(&username, &display, email.as_deref(), true)
        .context("bootstrap: creating admin user")?;
    let hash = auth::hash_secret(&password).context("bootstrap: hashing password")?;
    store
        .set_password(&util::new_id(), &username, &hash)
        .context("bootstrap: setting password")?;
    // A starter calendar so the UI isn't empty.
    store
        .create_calendar(
            &util::new_id(),
            &username,
            "Home",
            "VEVENT",
            Some("#3b5bff"),
            None,
        )
        .context("bootstrap: creating default calendar")?;

    tracing::info!(%username, "bootstrapped admin account from the environment");
    Ok(())
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The admin password from `WEEKLING_ADMIN_PASSWORD`, or read from the file at
/// `WEEKLING_ADMIN_PASSWORD_FILE` (for Docker/Compose secrets).
fn admin_password() -> Option<String> {
    if let Some(p) = env_nonempty("WEEKLING_ADMIN_PASSWORD") {
        return Some(p);
    }
    let path = env_nonempty("WEEKLING_ADMIN_PASSWORD_FILE")?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let trimmed = contents.trim_end_matches(['\n', '\r']).to_string();
    (!trimmed.is_empty()).then_some(trimmed)
}
