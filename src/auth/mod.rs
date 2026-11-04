//! SAML+TOTP login chain against ev.us.es (Blackboard Learn / Universidad de
//! Sevilla). Returns an authenticated reqwest::Client (cookie jar is internal
//! to the client, built fresh per login so no cookies leak between attempts).

pub mod saml;
pub mod totp;

use crate::credentials::Credentials;
use anyhow::{bail, Context, Result};
use reqwest::Client;
use std::collections::HashMap;

pub const ACS_URL: &str = "https://ev.us.es/auth-saml/saml/SSO/alias/_902_1";
const LOGIN_URL: &str =
    "https://ev.us.es/auth-saml/saml/login?apId=_902_1&redirectUrl=https%3A%2F%2Fev.us.es%2Fultra";

/// Runs the full SAML+TOTP chain against a fresh cookie jar and returns an
/// authenticated reqwest Client for Blackboard.
pub async fn login(creds: &Credentials) -> Result<Client> {
    let client = Client::builder()
        .cookie_store(true)
        .build()
        .context("building HTTP client")?;

    // Steps 1-3: GET login page, extract adAS_request_id + form action.
    let (login_html, login_url) = saml::get_text(&client, LOGIN_URL).await?;
    let form = saml::parse_login_form(&login_html, &login_url)?;
    ensure_same_origin(&form.form_action, "step 1-3 (form action)")?;

    // Step 4: POST credentials.
    let mut cred_body = HashMap::new();
    cred_body.insert("adAS_mode", "authn");
    cred_body.insert("adAS_request_id", form.request_id.as_str());
    cred_body.insert("adAS_username", creds.username.as_str());
    cred_body.insert("adAS_password", creds.password.as_str());
    let cred_res = client.post(&form.form_action).form(&cred_body).send().await?;
    let totp_html = cred_res.text().await?;
    saml::check_for_error(&totp_html, "step 4 (credentials)")?;

    // Step 5: find TOTP submit URL.
    let totp_url = saml::parse_totp_url(&totp_html)?;
    ensure_same_origin(&totp_url, "step 5 (TOTP URL)")?;

    // Step 6: generate + POST TOTP code.
    let code = totp::generate_code(&creds.totp_secret)?;
    let mut otp_body = HashMap::new();
    otp_body.insert("adAS_authn_module", "otp_module");
    otp_body.insert("adAS_mode", "authn");
    otp_body.insert("adAS_otp_code", code.as_str());
    let saml_res = client.post(&totp_url).form(&otp_body).send().await?;
    let saml_html = saml_res.text().await?;
    saml::check_for_error(&saml_html, "step 6 (TOTP)")?;

    // Step 7: extract SAMLResponse, POST to ACS.
    let assertion = saml::parse_saml_assertion(&saml_html)?;
    let mut acs_body = HashMap::new();
    acs_body.insert("SAMLResponse", assertion.saml_response.as_str());
    acs_body.insert("RelayState", assertion.relay_state.as_str());
    let acs_res = client.post(ACS_URL).form(&acs_body).send().await?;
    if !acs_res.status().is_success() {
        bail!("step 8 (ACS): POST returned {}", acs_res.status());
    }

    Ok(client)
}

/// Credentials/OTP get POSTed to URLs scraped out of the adAS page's own
/// HTML (form action, 2FA button href). The chain legitimately bounces
/// between ev.us.es (Blackboard) and sso.us.es (the university's SimpleSAMLphp
/// IdP), so the allow-list is the institution's us.es domain, not a single
/// host; this still refuses to post to anything outside it, so a tampered/
/// injected response can't redirect where the username/password/OTP end up.
fn ensure_same_origin(url: &str, step: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("{step}: parsing URL"))?;
    let is_us_es_host = parsed
        .host_str()
        .map(|h| h == "us.es" || h.ends_with(".us.es"))
        .unwrap_or(false);
    if parsed.scheme() != "https" || !is_us_es_host {
        bail!("{step}: refusing to post to off-origin URL: {url}");
    }
    Ok(())
}
