### EVUS MCP

MCP server exposing **ev.us.es** (Blackboard Learn, Universidad de Sevilla / UVUS) to Claude: courses, announcements, grades, content, attachments, and to-do items.

Rust, stdio transport ([rmcp](https://crates.io/crates/rmcp)). Handles the full SAML2 + TOTP SSO login chain headlessly and re-authenticates automatically on session expiry.

#### BUILD

```
cargo build --release
```

#### SETUP

Store UVUS credentials (username, password, TOTP secret) in the OS keychain:

```
evus-mcp set-creds
```

Verify the SAML+TOTP login chain works standalone, before wiring into Claude:

```
evus-mcp login
```

#### RUNNING AS AN MCP SERVER

Default (no subcommand) starts the stdio server:

```
evus-mcp
```

Add to Claude Code:

```
claude mcp add evus-mcp -- /path/to/evus-mcp.exe
```

or to `claude_desktop_config.json`'s `mcpServers`.

#### TOOLS

- `list_courses` / `get_course`: enrolled courses and course details
- `get_announcements`: course announcements
- `get_grades`: gradebook grades for the current user
- `list_contents` / `list_content_children`: course content tree
- `list_attachments` / `download_attachment` / `read_attachment_text`: content attachments (PDF, Office, legacy Office)
- `get_content` / `list_embedded_files` / `read_embedded_file_text`: Ultra document bodies and inline files
- `find_attachment` / `search_content`: search by file name / title across enrolled courses
- `get_todo_items`: to-do calendar items
