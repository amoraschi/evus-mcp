# Automated SSO Login — Design Considerations

## Current State

Auth flow opens a Tauri webview to `https://ev.us.es`, user logs in manually through
the university SSO, and cookies are harvested once the webview lands on `/ultra`.
Session cookies are persisted as plaintext JSON in `evus-session.json`.

---

## SSO Chain (confirmed from HAR capture)

```
1. GET  https://ev.us.es/auth-saml/saml/login?apId=_902_1&redirectUrl=https%3A%2F%2Fev.us.es%2Fultra
   → 302 to https://ev.us.es/auth-saml/saml2/authenticate/_902_1?redirectUrl=...

2. GET  https://ev.us.es/auth-saml/saml2/authenticate/_902_1?redirectUrl=...
   → 302 to https://sso.us.es/SAML2/SSOService.php?SAMLRequest=...&RelayState=...&Signature=...

3. GET  https://sso.us.es/SAML2/SSOService.php?SAMLRequest=...&RelayState=...&Signature=...
   → 200 OK — HTML login form

4. POST https://sso.us.es/SAML2/SSOService.php?SAMLRequest=...&RelayState=...&Signature=...
   → 200 OK — HTML TOTP form (credentials accepted, now wants 2FA)
   Content-Type: application/x-www-form-urlencoded
   Body fields:
     adAS_mode=authn
     adAS_request_id=<from form>        ← hidden field, changes per session
     adAS_username=<UVUS username>
     adAS_password=<URL-encoded password>

5. Extract TOTP submit URL from <a id="notification_2factor_button_ok" href="...">
   (adAS builds TOTP form via JS — no <form> tag exists in the HTML)

6. POST https://sso.us.es/SAML2/SSOService.php?adAS_request_id=<new_request_id>
   → 200 OK — HTML auto-submit form with SAMLResponse
   Content-Type: application/x-www-form-urlencoded
   Body fields:
     adAS_authn_module=otp_module
     adAS_mode=authn
     adAS_otp_code=<6-digit TOTP code>

7. POST https://ev.us.es/auth-saml/saml/SSO/alias/_902_1      ← ACS endpoint
   → Blackboard sets session cookies, redirects to /ultra
   Content-Type: application/x-www-form-urlencoded
   Body fields:
     SAMLResponse=<base64-encoded SAML XML>
     RelayState=<UUID from step 2>
   Origin: https://sso.us.es
```

### Key Observations

- **No CSRF tokens** — adAS uses `adAS_request_id` (a UUID) as the session correlation field
- **No redirects between auth steps** — both credential and TOTP POSTs return 200 with HTML
- **The `adAS_request_id` changes** — step 4 POST URL uses a different request ID than step 4's body.
  The body `adAS_request_id` comes from the login form HTML. The TOTP step posts to a URL with
  a *new* `adAS_request_id` (extracted from the TOTP page)
- **TOTP page has no `<form>` tag** — adAS builds the TOTP form entirely via JavaScript. The submit
  URL is found in `<a id="notification_2factor_button_ok" href="...">`, not a form action
- **SAMLResponse is auto-submitted by JS** — the response to the TOTP POST contains an HTML form
  with SAMLResponse + RelayState that JS auto-submits to the ACS. We parse it with scraper instead
- **No intermediate cookies needed** — the flow works without explicit cookie forwarding between
  sso.us.es requests (reqwest's cookie jar handles what's needed)
- **Both auth POSTs go to `sso.us.es/SAML2/SSOService.php`** — same endpoint, different query params
- **TOTP secret needs sanitization** — user-provided base32 secrets may contain spaces, dashes, or
  lowercase. Strip non-alphanumeric chars and uppercase before decoding

### IdP Details

| Field | Value |
|-------|-------|
| **IdP domain** | `sso.us.es` |
| **Software** | adAS SSO (Advanced Authentication Server) — developed by PRiSE / Universidad de Salamanca |
| **Protocols** | SAML 2.0, SAML 1.1/Shibboleth 1.3, PAPI v1, CAS, OpenSSO |
| **Federation** | RedIRIS SIR (Spanish academic identity federation) |
| **2FA type** | TOTP (RFC 6238) — Microsoft Authenticator, Google Authenticator, Authy |
| **TOTP params** | 6 digits, standard 30s period |
| **ACS endpoint** | `https://ev.us.es/auth-saml/saml/SSO/alias/_902_1` |

---

## Programmatic Replay Strategy

### Exact step-by-step with `reqwest`

```rust
// Step 1-3: Follow redirects to get the login form
let login_page = client
    .get("https://ev.us.es/auth-saml/saml/login?apId=_902_1&redirectUrl=https%3A%2F%2Fev.us.es%2Fultra")
    .send().await?;
// After redirect chain, we're at sso.us.es with the login form HTML
// Parse HTML to extract: adAS_request_id (hidden input), form action URL

// Step 4: POST credentials
let totp_page = client
    .post(&login_form_action_url)  // same URL we landed on
    .form(&[
        ("adAS_mode", "authn"),
        ("adAS_request_id", &request_id),  // from hidden field
        ("adAS_username", &username),
        ("adAS_password", &password),
    ])
    .send().await?;
// Returns 200 with TOTP challenge HTML
// Parse to extract TOTP submit URL from: <a id="notification_2factor_button_ok" href="...">
// (adAS builds TOTP form via JS, no <form> tag exists)

// Step 5: Generate TOTP code (sanitize secret: strip non-alnum, uppercase)
// Step 6: POST TOTP code
let saml_page = client
    .post(&totp_submit_url)  // sso.us.es/SAML2/SSOService.php?adAS_request_id=<new>
    .form(&[
        ("adAS_authn_module", "otp_module"),
        ("adAS_mode", "authn"),
        ("adAS_otp_code", &totp_code),
    ])
    .send().await?;
// Returns 200 with auto-submit form containing SAMLResponse + RelayState
// Parse HTML to extract both values

// Step 6: POST SAMLResponse to ACS
let result = client
    .post("https://ev.us.es/auth-saml/saml/SSO/alias/_902_1")
    .form(&[
        ("SAMLResponse", &saml_response),
        ("RelayState", &relay_state),
    ])
    .send().await?;
// Client now has Blackboard session cookies
```

### HTML Parsing with `scraper`

At each step, parse the response HTML to extract:

```rust
use scraper::{Html, Selector};

// Extract hidden input value
fn extract_hidden_input(html: &str, name: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(&format!("input[name='{name}']")).ok()?;
    doc.select(&sel).next()?.value().attr("value").map(String::from)
}

// Extract form action URL
fn extract_form_action(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("form").ok()?;
    doc.select(&sel).next()?.value().attr("action").map(String::from)
}

// Extract TOTP submit URL (adAS builds form via JS, URL is in <a> href)
fn extract_totp_url(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("#notification_2factor_button_ok").ok()?;
    doc.select(&sel).next()?.value().attr("href").map(String::from)
}
```

### Key reqwest Configuration

```rust
let jar = Arc::new(Jar::default());
let client = Client::builder()
    .cookie_provider(jar)
    .redirect(reqwest::redirect::Policy::limited(20))
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36")
    .timeout(Duration::from_secs(30))
    .build()?;
```

---

## Credential Storage: `keyring` Crate

### Why keyring

| Approach | Security | UX |
|----------|----------|----|
| Plaintext JSON (current session approach) | Bad — readable by any process | Transparent |
| Encrypted file with passphrase | OK — but user must enter passphrase each launch | Extra step |
| **`keyring` → Windows Credential Manager** | **Good — OS-level protection, tied to user account** | **Transparent** |

### Integration

```toml
[dependencies]
keyring = { version = "3", features = ["windows-native"] }
```

```rust
use keyring::Entry;

const SERVICE: &str = "evus-manager";

fn store_credentials(username: &str, password: &str) -> Result<(), keyring::Error> {
    Entry::new(SERVICE, "uvus-username")?.set_password(username)?;
    Entry::new(SERVICE, "uvus-password")?.set_password(password)?;
    Ok(())
}

fn load_credentials() -> Result<(String, String), keyring::Error> {
    let username = Entry::new(SERVICE, "uvus-username")?.get_password()?;
    let password = Entry::new(SERVICE, "uvus-password")?.get_password()?;
    Ok((username, password))
}

fn store_totp_secret(secret: &str) -> Result<(), keyring::Error> {
    Entry::new(SERVICE, "totp-secret")?.set_password(secret)?;
    Ok(())
}

fn clear_credentials() -> Result<(), keyring::Error> {
    let _ = Entry::new(SERVICE, "uvus-username")?.delete_credential();
    let _ = Entry::new(SERVICE, "uvus-password")?.delete_credential();
    let _ = Entry::new(SERVICE, "totp-secret")?.delete_credential();
    Ok(())
}
```

### Windows Credential Manager Behavior

- Credentials stored per-user, encrypted at rest by Windows
- Survives reboots, cleared when user explicitly deletes them
- Visible in Control Panel → Credential Manager (user can audit)
- No master password needed — tied to Windows login session
- Thread safety: Windows credential store doesn't serialize concurrent access — use a Mutex if needed

---

## TOTP Generation: `totp-rs` Crate

### Integration

```toml
[dependencies]
totp-rs = "6"
```

```rust
use totp_rs::{Algorithm, Builder, Secret};

fn generate_totp_code(secret_base32: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Sanitize: strip spaces/dashes, uppercase
    let clean: String = secret_base32.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '=')
        .collect::<String>()
        .to_uppercase();
    let secret = Secret::try_from_base32(&clean)?;
    let totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret)
        .build()?;
    Ok(format!("{}", totp.generate_current()))
}
```

### How the User Provides the TOTP Secret

Two options:
1. **Manual entry** — User copies the base32 secret from university 2FA setup page
   (the page shows a secret key with a "Copiar clave" button)
2. **QR scan** — User scans the QR with their phone, also pastes the text secret into the app

Store the secret in keyring alongside credentials.

---

## Risks & Mitigations

### Technical

| Risk | Impact | Mitigation |
|------|--------|------------|
| adAS changes form fields / adds CAPTCHA | Login breaks silently | Version-check the form; fall back to webview login |
| TOTP clock drift | Code rejected | Use system time; `totp-rs` skew parameter allows ±1 period |
| `adAS_request_id` extraction fails | Can't POST credentials | Parse error → fall back to webview |
| SAMLResponse form not found in HTML | Can't complete auth | Fall back to webview |
| Session cookies expire | 401 on API calls | Detect 401 → re-run auto-login → retry |

### Security

| Risk | Mitigation |
|------|------------|
| Credentials in memory | Zeroize password strings after use (`zeroize` crate, or `totp-rs` zeroize feature) |
| Another app reads Credential Manager | Windows isolates per-user; admin-level malware is game over anyway |
| TOTP secret compromise | Same risk as any authenticator app — keyring is equivalent security |
| Man-in-the-middle | All traffic is HTTPS; reqwest verifies TLS by default |

### Policy

| Concern | Notes |
|---------|-------|
| University acceptable use | Automated login may violate terms. Personal tool accessing your own data — low risk, but be aware |
| Rate limiting | Don't retry login in a loop. One attempt, fall back to webview on failure |

---

## Implementation Plan

### Phase 1: Credential UI + Storage
- Add settings fields for UVUS username + TOTP secret in the frontend
- Store via `keyring` on save, load on app start
- Password field in frontend, never displayed after entry

### Phase 2: Headless Login
- Implement `auto_login(username, password, totp_secret) -> Result<Client, Error>`
- Chain: GET SAML redirect → POST credentials → POST TOTP → parse SAMLResponse → POST ACS
- Parse HTML with `scraper` at each step
- On success: return authenticated `Client` with session cookies
- On failure: emit event to frontend, fall back to webview

### Phase 3: Integrate
- On app start: if credentials stored → try auto-login → if fail → open webview
- Settings page: "Save credentials" toggle, "Test login" button, "Clear credentials" button
- Status indicator: auto-login success/failure

---

## New Dependencies

```toml
keyring = { version = "3", features = ["windows-native"] }
totp-rs = "6"
```

Both are pure Rust on Windows (no native C deps beyond what Windows provides).

---

## adAS Form Field Reference (from HAR)

### Login form (step 4)

| Field | Value | Source |
|-------|-------|--------|
| `adAS_mode` | `"authn"` | Constant |
| `adAS_request_id` | UUID | Hidden input in login form HTML |
| `adAS_username` | UVUS ID | User input |
| `adAS_password` | URL-encoded password | User input |

### TOTP form (step 5)

| Field | Value | Source |
|-------|-------|--------|
| `adAS_authn_module` | `"otp_module"` | Constant |
| `adAS_mode` | `"authn"` | Constant |
| `adAS_otp_code` | 6-digit code | Generated from TOTP secret |

### ACS form (step 6)

| Field | Value | Source |
|-------|-------|--------|
| `SAMLResponse` | Base64 XML | Hidden input in auto-submit form |
| `RelayState` | UUID | Hidden input in auto-submit form |

---

## Session Expiry & Re-authentication

Don't track cookie lifetime. Instead, detect expiry at the API layer:

- On any API call, if the response is **401** or a **redirect to the login page**
  (check for `302` to `ev.us.es` login URL, or response body containing the login form),
  treat the session as expired.
- If credentials are stored in keyring → re-run `auto_login()` → retry the failed request.
- If re-login fails → fall back to webview, emit event to frontend.
- **One re-login attempt per failed request** — don't loop.

```rust
// In BbApi or a wrapper:
async fn get_with_reauth(&self, url: &str, state: &AppState) -> Result<Response, ApiError> {
    let response = self.client.get(url).send().await?;
    if is_session_expired(&response) {
        // Try re-login
        if let Ok((username, password, totp_secret)) = load_all_credentials() {
            let new_client = auto_login(&username, &password, &totp_secret).await?;
            state.replace_client(new_client);
            // Retry once
            return Ok(self.client.get(url).send().await?.error_for_status()?);
        }
        // No stored credentials → signal UI to open webview
        return Err(ApiError::SessionExpired);
    }
    Ok(response.error_for_status()?)
}

fn is_session_expired(response: &Response) -> bool {
    response.status() == 401
        || (response.status().is_redirection()
            && response.headers().get("location")
                .and_then(|v| v.to_str().ok())
                .map_or(false, |loc| loc.contains("/auth-saml/") || loc.contains("/login")))
}
```

---

## Resolved Questions

1. **Cookie jars on sso.us.es** — reqwest's cookie jar handles everything needed. No manual
   cookie forwarding required. Flow works end-to-end with default jar.
2. **Error responses** — adAS returns 200 with error messages in HTML (selectors:
   `.error`, `.alert-danger`, `.errorMessage`, `#errorMessage`). Detected and reported as
   `SsoError::Rejected`.
3. **TOTP form structure** — adAS TOTP page has NO `<form>` tag. The submit URL is in
   `<a id="notification_2factor_button_ok" href="...">`. Discovered by dumping response HTML.

---

## Sources

- [RedIRIS bulletin on adAS](https://www.rediris.es/rediris/boletin/90/ponencia5.C.pdf) — adAS architecture and protocol support
- [Blackboard SAML Auth Provider](https://help.blackboard.com/Learn/Administrator/Hosting/Authentication/Auth_Implementing/SAML_Authentication_Provider_Type) — Blackboard SAML setup
- [Blackboard REST 3LO + SSO](https://docs.blackboard.com/rest-apis/learn/admin/rest3LO-and-learnSSO) — REST API auth notes
- [Python SAML login gist](https://gist.github.com/3219601) — reference for SAML replay pattern
- [keyring crate](https://docs.rs/crate/keyring/3.6.3/source/README.md) — Windows Credential Manager integration
- [totp-rs crate](https://docs.rs/totp-rs/5.0.1/totp_rs) — TOTP code generation
- [SURF SAML product list](https://www.doc.surf/wiki/saml-based-products-and-services) — adAS listed as SAML IdP/SP
