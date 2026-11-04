//! Standalone SSO login test against ev.us.es, run before trusting the MCP
//! server's login chain. Equivalent to login.mjs.

use crate::{auth, credentials};
use anyhow::{bail, Result};

pub async fn run() -> Result<()> {
    match try_login().await {
        Ok(summary) => {
            println!("LOGIN OK: {summary}");
            Ok(())
        }
        Err(err) => {
            eprintln!("LOGIN FAILED: {err}");
            std::process::exit(1);
        }
    }
}

async fn try_login() -> Result<String> {
    let creds = credentials::load_credentials()?;
    eprintln!("[login] credentials loaded, starting SAML chain");
    let client = auth::login(&creds).await?;
    eprintln!("[login] SAMLResponse posted to ACS, verifying session");

    let res = client
        .get("https://ev.us.es/learn/api/public/v1/users/me")
        .send()
        .await?;
    if !res.status().is_success() {
        bail!("verify: /users/me returned {}", res.status());
    }
    let me: serde_json::Value = res.json().await?;
    Ok(serde_json::json!({ "id": me.get("id"), "userName": me.get("userName") }).to_string())
}
