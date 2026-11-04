# EVUS MCP Server — Design Doc

Standalone Node/TS project, independent of this repo. Goal: expose ev.us.es
(Blackboard Learn, Universidad de Sevilla) to Claude via MCP tools.

## Stack

Node/TS. `@modelcontextprotocol/sdk` (stdio transport), `cheerio` (HTML
scrape), `otplib` (TOTP), `keytar` or `@napi-rs/keyring` (OS keychain),
native `fetch` + `tough-cookie`/`fetch-cookie` for the cookie-jar HTTP
client.

## Login (confirmed SSO chain, via HAR capture)

1. GET `https://ev.us.es/auth-saml/saml/login?apId=_902_1&redirectUrl=...` →
   redirect chain lands on `sso.us.es/SAML2/SSOService.php` login form.
2. Scrape hidden `adAS_request_id` + form action from login HTML.
3. POST `adAS_mode=authn`, `adAS_request_id`, `adAS_username`, `adAS_password`
   → 200 HTML TOTP challenge (no `<form>` tag — JS-built).
4. Scrape TOTP submit URL from `<a id="notification_2factor_button_ok" href="...">`.
5. Generate 6-digit TOTP (SHA1, 30s step, ±1 skew) from stored base32 secret
   (sanitize: strip non-alnum, uppercase).
6. POST `adAS_authn_module=otp_module`, `adAS_mode=authn`, `adAS_otp_code`
   → 200 HTML auto-submit form containing `SAMLResponse` + `RelayState`.
7. POST those two fields to ACS `https://ev.us.es/auth-saml/saml/SSO/alias/_902_1`
   → Blackboard sets session cookies.

No CSRF tokens, no manual cookie forwarding needed — a standard cookie jar
across the whole chain is sufficient. Error detection: adAS returns 200 with
error text in `.error`/`.alert-danger`/`.errorMessage`/`#errorMessage`.

IdP: `sso.us.es`, adAS SSO (SAML 2.0), RedIRIS SIR federation, TOTP 2FA
(6 digits, 30s step). ACS endpoint: `https://ev.us.es/auth-saml/saml/SSO/alias/_902_1`.

## Session lifecycle in the MCP server process

- Server holds one shared authenticated HTTP client (cookie jar) in memory,
  behind a mutex (avoid concurrent re-login races).
- Lazy login: first tool call that needs the platform triggers the chain
  above if no session exists yet.
- Re-auth: any API response that is 401, or a redirect back to the login
  page, is treated as session-expired → re-run login once → retry the
  request once. No retry loops.
- Credentials (UVUS username, password, TOTP secret) loaded once at startup
  from the OS keychain, never requested interactively (MCP tool calls have
  no human-in-the-loop prompt, so login must be fully headless).

## Credential storage

OS keychain, not plaintext/config file. Node: `keytar` or
`@napi-rs/keyring` → Windows Credential Manager / macOS Keychain /
Secret Service.

## Tools (Blackboard Learn REST API, `/learn/api/public/v{1,3}` +
`/learn/api/v1`, base `https://ev.us.es`)

- `list_courses` — `GET /users/{userId}/courses`
- `get_course` — `GET /v3/courses/{courseId}`
- `get_announcements` — `GET /v1/courses/{courseId}/announcements`
- `get_grades` — `GET /v1/courses/{courseId}/gradebook/grades?userId=...`
- `list_contents` — `GET /courses/{courseId}/contents`
- `list_content_children` — `GET /courses/{courseId}/contents/{contentId}/children`
- `list_attachments` — `GET /courses/{courseId}/contents/{contentId}/attachments`
- `download_attachment` — `GET /courses/{courseId}/contents/{contentId}/attachments/{attachmentId}/download`
- `get_todo_items` — `GET /v1/calendars/todo/studentItems?since=...&until=...`

User id needed for several calls: `GET /users/me` once at startup, cache it.

Skip for v1: avatar fetch, Ultra-document inline-body parsing, WebDAV
download — low value for an LLM client, add only if needed.

## Running locally against Claude

- `claude mcp add evus -- node server.js` (Claude Code), or add to
  `mcpServers` in `claude_desktop_config.json` (Claude Desktop).
- Debug standalone first with `npx @modelcontextprotocol/inspector node server.js`
  before wiring into Claude — catches schema/login bugs without Claude in
  the loop.
- stdout is the protocol channel — never `console.log` there; log to stderr.

## Risks

- adAS changes form fields → login breaks silently, no fallback (headless
  server, no webview to fall back to)
- Don't retry login in a loop — one attempt per expired session
