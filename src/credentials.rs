//! OS keychain storage for UVUS credentials. Mirrors auth.mjs's loadCredentials()
//! and set-creds.mjs, using the same keytar service name/keys so existing stored
//! credentials (if any) keep working: service "evus-mcp", keys uvus-username,
//! uvus-password, totp-secret.

use anyhow::{bail, Context, Result};
use keyring::Entry;
use std::io::{self, Write};

const SERVICE: &str = "evus-mcp";

pub struct Credentials {
    pub username: String,
    pub password: String,
    pub totp_secret: String,
}

fn entry(key: &str) -> Result<Entry> {
    Entry::new(SERVICE, key).context("opening keychain entry")
}

fn get(key: &str) -> Option<String> {
    entry(key).ok()?.get_password().ok()
}

fn set(key: &str, value: &str) -> Result<()> {
    entry(key)?.set_password(value).context("writing keychain entry")
}

/// Equivalent to auth.mjs's loadCredentials().
pub fn load_credentials() -> Result<Credentials> {
    let username = get("uvus-username");
    let password = get("uvus-password");
    let totp_secret = get("totp-secret");
    match (username, password, totp_secret) {
        (Some(username), Some(password), Some(totp_secret)) => {
            Ok(Credentials { username, password, totp_secret })
        }
        _ => bail!("Missing credentials in keychain, run `evus-mcp set-creds` first."),
    }
}

/// Equivalent to set-creds.mjs: prompt on stdin (not hidden, the JS version
/// doesn't hide input either, since it relies on nothing landing in shell
/// history or a file rather than on terminal echo suppression) and store in
/// the OS keychain.
pub fn prompt_and_store() -> Result<()> {
    let username = prompt("UVUS username: ")?;
    let password = prompt("UVUS password: ")?;
    let totp_secret = prompt("TOTP base32 secret: ")?;

    set("uvus-username", &username)?;
    set("uvus-password", &password)?;
    set("totp-secret", &totp_secret)?;

    println!("Stored in OS keychain under service 'evus-mcp'.");
    Ok(())
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}
