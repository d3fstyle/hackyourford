# FordDiag MCP setup (Cursor Agent)

The Cursor agent can drive the same OBD session as the FordDiag UI via a localhost bridge.

## Architecture

1. **FordDiag app** listens on `http://127.0.0.1:8787` and writes a Bearer token to:
   - `$XDG_RUNTIME_DIR/forddiag/agent.token`, or
   - `/tmp/forddiag-agent.token`
2. **MCP server** (`mcp/src/index.ts`) speaks stdio MCP to Cursor and HTTP to the bridge.
3. Both UI and agent share the session worker (one serial port).

## Setup

### 1. Install MCP deps (once)

```bash
cd mcp && npm install && cd ..
```

### 2. Start FordDiag

```bash
npm run tauri dev
```

Confirm the footer shows `MCP :8787`, or:

```bash
curl -s http://127.0.0.1:8787/health
```

### 3. Enable MCP in Cursor

This repo ships [`.cursor/mcp.json`](../.cursor/mcp.json). Reload MCP servers in Cursor (Settings → MCP), or restart the agent chat.

Optional env overrides:

| Variable | Default | Meaning |
|----------|---------|---------|
| `FORDDIAG_URL` | `http://127.0.0.1:8787` | Bridge base URL |
| `FORDDIAG_TOKEN` | (read from token file) | Bearer token |

### 4. Use tools from the agent

Examples:

- `forddiag_health` — is the app up?
- `forddiag_connect` with `port: /dev/rfcomm0`
- `forddiag_connect_demo` with `fixture_id: focus_2017_eu_demo`
- `forddiag_select_profile` → `forddiag_scan_modules` → `forddiag_read_all_dtcs`
- `forddiag_start_live` / `forddiag_get_live_samples` / `forddiag_stop`
- `forddiag_clear_dtcs` requires `confirm: true`
- `forddiag_write_as_built` requires `confirm: true` (writes ECU As-Built config)
- `forddiag_decode_as_built_params` / `forddiag_write_as_built_params` (named bit-field params; write needs `confirm: true`)

If tools fail with connection refused, open FordDiag first, then retry.

## Security

- Bridge binds **only** to `127.0.0.1`.
- Token file is mode `0600` when possible.
- Do not expose port 8787 on a network interface.
