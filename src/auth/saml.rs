//! SAML page-scraping steps of the adAS login chain against ev.us.es.
//! Split out from TOTP generation (auth.mjs did both in one file).

use anyhow::{anyhow, bail, Result};
use reqwest::Client;
use scraper::{Html, Selector};

const ERROR_SELECTORS: &str = ".error, .alert-danger, .errorMessage, #errorMessage";

fn select_first<'a>(html: &'a Html, selector: &str) -> Option<scraper::ElementRef<'a>> {
    let sel = Selector::parse(selector).ok()?;
    html.select(&sel).next()
}

fn attr(html: &Html, selector: &str, attr_name: &str) -> Option<String> {
    select_first(html, selector).and_then(|el| el.value().attr(attr_name)).map(str::to_string)
}

pub fn check_for_error(body: &str, step: &str) -> Result<()> {
    let html = Html::parse_document(body);
    if let Some(el) = select_first(&html, ERROR_SELECTORS) {
        let text = el.text().collect::<String>().trim().to_string();
        if !text.is_empty() {
            bail!("{step}: adAS returned an error: {text}");
        }
    }
    Ok(())
}

pub struct LoginFormStep {
    pub request_id: String,
    pub form_action: String,
}

/// Step 1-3: GET the login page, find adAS_request_id and the form action.
pub fn parse_login_form(html_body: &str, response_url: &str) -> Result<LoginFormStep> {
    let html = Html::parse_document(html_body);
    let request_id = attr(&html, "input[name='adAS_request_id']", "value")
        .ok_or_else(|| anyhow!("step 1-3: could not find adAS_request_id in login form"))?;
    let form_action = attr(&html, "form", "action").unwrap_or_else(|| response_url.to_string());
    Ok(LoginFormStep { request_id, form_action })
}

/// Step 5: find the TOTP submit URL after credentials are posted.
pub fn parse_totp_url(html_body: &str) -> Result<String> {
    let html = Html::parse_document(html_body);
    attr(&html, "#notification_2factor_button_ok", "href")
        .ok_or_else(|| anyhow!("step 5: could not find TOTP submit URL, adAS form fields may have changed"))
}

pub struct SamlAssertion {
    pub saml_response: String,
    pub relay_state: String,
}

/// Step 7: find the SAMLResponse/RelayState in the auto-submit form.
pub fn parse_saml_assertion(html_body: &str) -> Result<SamlAssertion> {
    let html = Html::parse_document(html_body);
    let saml_response = attr(&html, "input[name='SAMLResponse']", "value")
        .ok_or_else(|| anyhow!("step 7: SAMLResponse not found in auto-submit form"))?;
    let relay_state = attr(&html, "input[name='RelayState']", "value").unwrap_or_default();
    Ok(SamlAssertion { saml_response, relay_state })
}

/// Fetch helper shared by the login chain (plain GET with no special headers).
pub async fn get_text(client: &Client, url: &str) -> Result<(String, String)> {
    let res = client.get(url).send().await?;
    let final_url = res.url().to_string();
    let text = res.text().await?;
    Ok((text, final_url))
}
