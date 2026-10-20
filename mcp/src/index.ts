#!/usr/bin/env node
/**
 * FordDiag MCP stdio server — proxies tools to the Tauri agent bridge (127.0.0.1:8787).
 */
import { readFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const BASE_URL = (process.env.FORDDIAG_URL ?? "http://127.0.0.1:8787").replace(
  /\/$/,
  "",
);

function resolveToken(): string {
  if (process.env.FORDDIAG_TOKEN?.trim()) {
    return process.env.FORDDIAG_TOKEN.trim();
  }
  const candidates = [
    process.env.XDG_RUNTIME_DIR
      ? join(process.env.XDG_RUNTIME_DIR, "forddiag", "agent.token")
      : null,
    "/tmp/forddiag-agent.token",
    join(homedir(), ".cache", "forddiag", "agent.token"),
  ].filter(Boolean) as string[];

  for (const path of candidates) {
    if (existsSync(path)) {
      return readFileSync(path, "utf8").trim();
    }
  }
  return "";
}

async function api(
  method: string,
  path: string,
  body?: unknown,
): Promise<unknown> {
  const token = resolveToken();
  const headers: Record<string, string> = {
    Accept: "application/json",
  };
  if (path !== "/health") {
    if (!token) {
      throw new Error(
        "No FordDiag agent token found. Open FordDiag (npm run tauri dev) so it writes the token, or set FORDDIAG_TOKEN.",
      );
    }
    headers.Authorization = `Bearer ${token}`;
  }
  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }

  let res: Response;
  try {
    res = await fetch(`${BASE_URL}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    throw new Error(
      `Cannot reach FordDiag bridge at ${BASE_URL} (${msg}). Open FordDiag (npm run tauri dev) and retry.`,
    );
  }

  const text = await res.text();
  let data: unknown = text;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    /* keep text */
  }
  if (!res.ok) {
    const err =
      typeof data === "object" &&
      data &&
      "error" in data &&
      typeof (data as { error: unknown }).error === "string"
        ? (data as { error: string }).error
        : text || res.statusText;
    throw new Error(`FordDiag API ${res.status}: ${err}`);
  }
  return data;
}

function ok(data: unknown) {
  return {
    content: [
      {
        type: "text" as const,
        text: JSON.stringify(data, null, 2),
      },
    ],
  };
}

function fail(e: unknown) {
  const msg = e instanceof Error ? e.message : String(e);
  return {
    content: [{ type: "text" as const, text: msg }],
    isError: true as const,
  };
}

const server = new McpServer({
  name: "forddiag",
  version: "0.1.0",
});

server.tool(
  "forddiag_health",
  "Check if the FordDiag app agent bridge is reachable (no auth).",
  {},
  async () => {
    try {
      return ok(await api("GET", "/health"));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_status",
  "Get OBD connection status (adapter, VIN, profile).",
  {},
  async () => {
    try {
      return ok(await api("GET", "/status"));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_list_ports",
  "List serial ports (e.g. /dev/rfcomm0).",
  {},
  async () => {
    try {
      return ok(await api("GET", "/ports"));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_list_profiles",
  "List built-in vehicle profiles.",
  {},
  async () => {
    try {
      return ok(await api("GET", "/profiles"));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_list_fixtures",
  "List demo fixtures for hardware-free testing.",
  {},
  async () => {
    try {
      return ok(await api("GET", "/fixtures"));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_connect",
  "Connect to a serial OBD adapter.",
  {
    port: z.string().describe("Serial device path, e.g. /dev/rfcomm0"),
    baud: z.number().int().optional().describe("Baud rate (default 115200)"),
  },
  async ({ port, baud }) => {
    try {
      return ok(await api("POST", "/connect", { port, baud }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_connect_demo",
  "Connect using a built-in demo fixture (no hardware).",
  {
    fixture_id: z.string().describe("Fixture id, e.g. focus_2017_eu_demo"),
  },
  async ({ fixture_id }) => {
    try {
      return ok(await api("POST", "/connect_demo", { fixture_id }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_disconnect",
  "Disconnect from the adapter.",
  {},
  async () => {
    try {
      return ok(await api("POST", "/disconnect", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_select_profile",
  "Select a vehicle profile by id.",
  {
    profile_id: z.string().describe("e.g. focus_2017_eu"),
  },
  async ({ profile_id }) => {
    try {
      return ok(await api("POST", "/profile", { profile_id }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_read_vin",
  "Read VIN from PCM.",
  {},
  async () => {
    try {
      return ok(await api("POST", "/read_vin", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_scan_modules",
  "Scan profile modules (online/offline + identity DIDs).",
  {},
  async () => {
    try {
      return ok(await api("POST", "/scan_modules", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_discover_ecus",
  "Brute-force discover ECUs on a CAN bus ID range.",
  {
    bus: z.enum(["hs", "ms"]).describe("CAN bus"),
    from_id: z.number().int().describe("Start CAN ID (e.g. 0x700 = 1792)"),
    to_id: z.number().int().describe("End CAN ID (e.g. 0x7EF = 2031)"),
  },
  async ({ bus, from_id, to_id }) => {
    try {
      return ok(await api("POST", "/discover_ecus", { bus, from_id, to_id }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_read_all_dtcs",
  "Read DTCs from all profile modules.",
  {},
  async () => {
    try {
      return ok(await api("POST", "/read_all_dtcs", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_read_dtcs",
  "Read DTCs from one module.",
  {
    module_id: z.string().describe("Module id, e.g. PCM"),
  },
  async ({ module_id }) => {
    try {
      return ok(await api("POST", "/read_dtcs", { module_id }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_clear_dtcs",
  "Clear DTCs on a module. DESTRUCTIVE — requires confirm=true.",
  {
    module_id: z.string(),
    confirm: z
      .literal(true)
      .describe("Must be true to proceed with clearing DTCs"),
  },
  async ({ module_id, confirm }) => {
    try {
      return ok(await api("POST", "/clear_dtcs", { module_id, confirm }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_start_live",
  "Start live polling of Mode 01 PIDs and UDS DIDs.",
  {
    pids: z.array(z.number().int()).describe("OBD Mode 01 PID numbers"),
    dids: z
      .array(
        z.tuple([
          z.string().describe("module_id"),
          z.number().int().describe("DID"),
          z.string().nullable().describe("label"),
        ]),
      )
      .describe("UDS DIDs to poll"),
    interval_ms: z.number().int().optional(),
  },
  async ({ pids, dids, interval_ms }) => {
    try {
      return ok(
        await api("POST", "/live/start", { pids, dids, interval_ms }),
      );
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_stop",
  "Stop current activity (live, sniff, discover, sweep).",
  {},
  async () => {
    try {
      return ok(await api("POST", "/stop", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_get_live_samples",
  "Get buffered live samples from the current/last live session.",
  {},
  async () => {
    try {
      return ok(await api("GET", "/live/samples"));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_save_live",
  "Save live session JSON to a filesystem path.",
  {
    path: z.string().describe("Absolute path for the JSON file"),
  },
  async ({ path }) => {
    try {
      return ok(await api("POST", "/live/save", { path }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_list_supported_pids",
  "Query supported OBD Mode 01 PIDs from the PCM.",
  {},
  async () => {
    try {
      return ok(await api("GET", "/live/supported_pids"));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_start_sniff",
  "Start CAN bus monitor (ATMA).",
  {
    bus: z.enum(["hs", "ms"]),
  },
  async ({ bus }) => {
    try {
      return ok(await api("POST", "/sniff/start", { bus }));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_mark_press",
  "Mark start/end of a button-press window while sniffing.",
  {},
  async () => {
    try {
      return ok(await api("POST", "/sniff/mark_press", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_analyze_pattern",
  "Correlate CAN frame changes with press windows.",
  {},
  async () => {
    try {
      return ok(await api("POST", "/sniff/analyze", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_did_sweep",
  "Sweep a DID range on a module via UDS 0x22.",
  {
    module_id: z.string(),
    from_did: z.number().int(),
    to_did: z.number().int(),
  },
  async ({ module_id, from_did, to_did }) => {
    try {
      return ok(
        await api("POST", "/did/sweep", { module_id, from_did, to_did }),
      );
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_did_snapshot",
  "Take a DID-range snapshot (before or after) for diffing.",
  {
    module_id: z.string(),
    from_did: z.number().int(),
    to_did: z.number().int(),
    which: z.enum(["before", "after"]),
  },
  async ({ module_id, from_did, to_did, which }) => {
    try {
      return ok(
        await api("POST", "/did/snapshot", {
          module_id,
          from_did,
          to_did,
          which,
        }),
      );
    } catch (e) {
      return fail(e);
    }
  },
);

server.tool(
  "forddiag_did_diff",
  "Diff DID snapshots A (before) vs B (after).",
  {},
  async () => {
    try {
      return ok(await api("POST", "/did/diff", {}));
    } catch (e) {
      return fail(e);
    }
  },
);

const transport = new StdioServerTransport();
await server.connect(transport);
