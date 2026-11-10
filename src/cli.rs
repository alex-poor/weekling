//! Admin CLI: `weekling <command> …`. Hand-rolled parsing to avoid a CLI
//! dependency (DESIGN.md minimal-deps principle). Runs against the open store.

use anyhow::{Result, bail};

use crate::store::Store;
use crate::{auth, util};

const USAGE: &str = "\
weekling — self-hosted calendar server

USAGE:
    weekling [serve]                         Run the server (default)
    weekling user add <name> [--display D] [--admin]
    weekling user passwd <name>              Set the account password (reads stdin)
    weekling token add <user> [--label L]    Create an app password (prints it once)
    weekling calendar add <user> <title> [--id ID] [--color #HEX]
                                             [--components VEVENT|VTODO|VEVENT,VTODO]
    weekling calendar ls <user>
    weekling share add <owner> <cal> <grantee> [--access read|read-write|freebusy]
    weekling share ls <owner> <cal>
    weekling share rm <owner> <cal> <grantee>
    weekling help
";

/// Handle a subcommand. Returns Ok(true) if a command was handled (caller should
/// exit), Ok(false) if the caller should start the server.
pub fn dispatch(store: &Store, args: &[String]) -> Result<bool> {
    let cmd = match args.first().map(String::as_str) {
        None | Some("serve") => return Ok(false),
        Some("help" | "-h" | "--help") => {
            print!("{USAGE}");
            return Ok(true);
        }
        Some(c) => c,
    };

    match cmd {
        "user" => user_cmd(store, &args[1..])?,
        "token" => token_cmd(store, &args[1..])?,
        "calendar" | "cal" => calendar_cmd(store, &args[1..])?,
        "share" => share_cmd(store, &args[1..])?,
        other => bail!("unknown command '{other}'\n\n{USAGE}"),
    }
    Ok(true)
}

fn user_cmd(store: &Store, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("add") => {
            let rest = &args[1..];
            let name = positional(rest, 0).ok_or_else(|| err("user add <name>"))?;
            let display = flag(rest, "--display").unwrap_or_else(|| name.clone());
            let admin = has_flag(rest, "--admin");
            store.create_user(&name, &display, admin)?;
            println!(
                "created user '{name}'{}",
                if admin { " (admin)" } else { "" }
            );
            Ok(())
        }
        Some("passwd") => {
            let user = positional(&args[1..], 0).ok_or_else(|| err("user passwd <name>"))?;
            if store.get_principal(&user)?.is_none() {
                bail!("no such user '{user}'");
            }
            eprint!("New password for {user}: ");
            let mut pw = String::new();
            std::io::stdin().read_line(&mut pw)?;
            let pw = pw.trim_end_matches(['\n', '\r']);
            if pw.chars().count() < 8 {
                bail!("password too short (minimum 8 characters)");
            }
            let hash = auth::hash_secret(pw)?;
            store.set_password(&util::new_id(), &user, &hash)?;
            println!("password set for '{user}'");
            Ok(())
        }
        _ => bail!("usage: weekling user add|passwd …"),
    }
}

fn token_cmd(store: &Store, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("add") => {
            let rest = &args[1..];
            let user = positional(rest, 0).ok_or_else(|| err("token add <user>"))?;
            if store.get_principal(&user)?.is_none() {
                bail!("no such user '{user}'");
            }
            let label = flag(rest, "--label").unwrap_or_else(|| "app password".into());
            let token = util::new_token();
            let hash = auth::hash_secret(&token)?;
            store.add_app_password(&util::new_id(), &user, &label, &hash)?;
            println!("app password for '{user}' ({label}):\n\n    {token}\n");
            println!("Use it as the password in your CalDAV client. It won't be shown again.");
            Ok(())
        }
        _ => bail!("usage: weekling token add <user> [--label L]"),
    }
}

fn calendar_cmd(store: &Store, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("add") => {
            let rest = &args[1..];
            let user = positional(rest, 0).ok_or_else(|| err("calendar add <user> <title>"))?;
            let title = positional(rest, 1).ok_or_else(|| err("calendar add <user> <title>"))?;
            if store.get_principal(&user)?.is_none() {
                bail!("no such user '{user}'");
            }
            let id = flag(rest, "--id").unwrap_or_else(util::new_id);
            let color = flag(rest, "--color");
            let components = flag(rest, "--components").unwrap_or_else(|| "VEVENT".into());
            validate_components(&components)?;
            store.create_calendar(&id, &user, &title, &components, color.as_deref(), None)?;
            println!("created calendar '{title}' at /dav/calendars/{user}/{id}/");
            Ok(())
        }
        Some("ls") => {
            let user = positional(&args[1..], 0).ok_or_else(|| err("calendar ls <user>"))?;
            for c in store.list_calendars(&user)? {
                println!(
                    "{:<34} {:<24} [{}] token={}",
                    c.id, c.display_name, c.components, c.sync_token
                );
            }
            Ok(())
        }
        _ => bail!("usage: weekling calendar add|ls …"),
    }
}

fn share_cmd(store: &Store, args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("add") => {
            let rest = &args[1..];
            let owner =
                positional(rest, 0).ok_or_else(|| err("share add <owner> <cal> <grantee>"))?;
            let cal =
                positional(rest, 1).ok_or_else(|| err("share add <owner> <cal> <grantee>"))?;
            let grantee =
                positional(rest, 2).ok_or_else(|| err("share add <owner> <cal> <grantee>"))?;
            let access = flag(rest, "--access").unwrap_or_else(|| "read".into());
            if !matches!(access.as_str(), "read" | "read-write" | "freebusy") {
                bail!("invalid --access '{access}' (read | read-write | freebusy)");
            }
            if store.share_calendar(&owner, &cal, &grantee, &access)? {
                println!("shared {owner}/{cal} with '{grantee}' ({access})");
            } else {
                bail!("could not share — check that '{owner}' owns '{cal}' and '{grantee}' exists");
            }
            Ok(())
        }
        Some("ls") => {
            let owner = positional(&args[1..], 0).ok_or_else(|| err("share ls <owner> <cal>"))?;
            let cal = positional(&args[1..], 1).ok_or_else(|| err("share ls <owner> <cal>"))?;
            for (grantee, access) in store.list_shares(&owner, &cal)? {
                println!("{grantee:<20} {access}");
            }
            Ok(())
        }
        Some("rm") => {
            let rest = &args[1..];
            let owner =
                positional(rest, 0).ok_or_else(|| err("share rm <owner> <cal> <grantee>"))?;
            let cal = positional(rest, 1).ok_or_else(|| err("share rm <owner> <cal> <grantee>"))?;
            let grantee =
                positional(rest, 2).ok_or_else(|| err("share rm <owner> <cal> <grantee>"))?;
            if store.unshare(&owner, &cal, &grantee)? {
                println!("unshared {owner}/{cal} from '{grantee}'");
            } else {
                bail!("no such share");
            }
            Ok(())
        }
        _ => bail!("usage: weekling share add|ls|rm …"),
    }
}

fn validate_components(c: &str) -> Result<()> {
    for part in c.split(',') {
        if !matches!(part, "VEVENT" | "VTODO") {
            bail!("invalid component '{part}' (allowed: VEVENT, VTODO)");
        }
    }
    Ok(())
}

// ---- tiny arg helpers ---------------------------------------------------

fn err(usage: &str) -> anyhow::Error {
    anyhow::anyhow!("usage: weekling {usage}")
}

/// The Nth argument that is not a flag or a flag's value.
fn positional(args: &[String], n: usize) -> Option<String> {
    let mut idx = 0;
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            // Skip `--flag value` (but not boolean flags we know).
            if !is_bool_flag(&args[i]) {
                i += 1;
            }
            i += 1;
            continue;
        }
        if idx == n {
            return Some(args[i].clone());
        }
        idx += 1;
        i += 1;
    }
    None
}

fn is_bool_flag(flag: &str) -> bool {
    matches!(flag, "--admin")
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}
