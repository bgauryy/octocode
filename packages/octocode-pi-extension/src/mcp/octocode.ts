import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CONFIG_DIR_NAME, getAgentDir, type ExtensionAPI, type McpServerConfig, type ToolResultEvent, type ToolResultEventResult } from '@earendil-works/pi-coding-agent';
import { MCP_DIRECT_ENV, MCP_ENV, envFlag } from '../shared/env.js';
import { octocodeHome } from '../shared/home.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';

/**
 * Octocode MCP (local code, LSP, GitHub and package registry research) runs on Pi's built-in MCP support: one
 * `pi.registerMcpServer` call, so connection, reconnects, OAuth, `/mcp` and exposure are Pi's. A server named
 * `octocode` in Pi's `mcp.json` (`~/.pi/agent/mcp.json`, `.pi/mcp.json`) takes precedence, which is how users override
 * or disable it.
 */
export const OCTOCODE_SERVER = 'octocode';

/** Pi names MCP tools `mcp__<server>__<tool>`. */
export const OCTOCODE_TOOL_PREFIX = `mcp__${OCTOCODE_SERVER}__`;

/** The octocode-mcp release this extension depends on (package.json); the npx fallback pins it too. */
export const OCTOCODE_MCP_VERSION = '19.1.0';

/** Whether `name` is an Octocode MCP tool. */
export const isOctocodeTool = (name: string): boolean => name.startsWith(OCTOCODE_TOOL_PREFIX);

/**
 * Tools declared only once `tool_search` loads them: the GitHub tools (`gh*`) and the package tool (`artifactSearch`),
 * which carry most of the schema text and which a local coding task rarely needs. Local search, reads, structure, AST
 * and LSP stay declared. `OCTOCODE_MCP_DIRECT=1` declares everything.
 */
export const DEFERRED_TOOL_EXPOSURE: Readonly<Record<string, 'deferred'>> = { 'gh*': 'deferred', artifactSearch: 'deferred' };

/** Whether the GitHub and package registry tools are deferred (the default) rather than declared directly. */
export const octocodeToolsDeferred = (env: NodeJS.ProcessEnv = process.env): boolean => !envFlag(env, MCP_DIRECT_ENV);

/**
 * Pi expands `$NAME`, `${NAME}` and a leading `!command` in env values. Literal paths must stay literal: `$$` is a
 * literal `$`, and a leading `!` gets a `$` in front (`$!` is a literal `!`).
 */
export function literalConfigValue(value: string): string {
  const escaped = value.replace(/\$/g, '$$$$');
  return escaped.startsWith('!') ? `$${escaped}` : escaped;
}

/** The server config: the bundled octocode-mcp bin under this Node, else the pinned npm release. */
export function octocodeServerConfig(cwd: string, env: NodeJS.ProcessEnv = process.env, entry = octocodeMcpEntry()): McpServerConfig {
  // Octocode reads only under the home directory unless told otherwise: allow the workspace and the Octocode home.
  // Tokens (GITHUB_TOKEN, …) need no forwarding: stdio servers inherit Pi's environment.
  const allowed = [...new Set([cwd, octocodeHome(env)])].join(',');
  const launch = entry ? { command: process.execPath, args: [entry] } : { command: 'npx', args: ['-y', `octocode-mcp@${OCTOCODE_MCP_VERSION}`] };
  return {
    ...launch,
    cwd,
    env: { WORKSPACE_ROOT: literalConfigValue(cwd), ALLOWED_PATHS: literalConfigValue(allowed) },
    exposure: 'direct',
    ...(octocodeToolsDeferred(env) ? { toolExposure: { ...DEFERRED_TOOL_EXPOSURE } } : {}),
    description: 'Repository and package evidence: local code search and reads, LSP, GitHub code/PRs/history and package registry metadata (npm, PyPI, crates, Maven, NuGet, Go, Packagist, RubyGems). Use the connected tools\' schemas and result pagination hints; availability follows Pi MCP configuration.',
  };
}

/**
 * Registers the Octocode server for each session's workspace unless `OCTOCODE_MCP=0` (profiles with `mcp: false`).
 * Registered on `session_start`, which runs before Pi's MCP connects its servers, so the first prompt still waits for
 * it; registering the same name again (a session in another folder) replaces the earlier registration.
 */
export function registerOctocodeMcp(pi: ExtensionAPI): boolean {
  // Also for a user's own `octocode` server: its text reaches the model and the terminal either way.
  pi.on('tool_result', async (event) => sanitizeOctocodeResult(event));
  if (!octocodeMcpEnabled()) return false;
  pi.on('session_start', async (_event, ctx) => {
    pi.registerMcpServer(OCTOCODE_SERVER, octocodeServerConfig(ctx.cwd));
  });
  return true;
}

/** Whether the Octocode MCP server is registered for this process (the same switch `registerOctocodeMcp` reads). */
export const octocodeMcpEnabled = (env: NodeJS.ProcessEnv = process.env): boolean => envFlag(env, MCP_ENV, true);

/**
 * The `octocode` entry of Pi's `mcp.json` (global, then the project's in a trusted project, which replaces it), which
 * takes precedence over the registration; undefined when neither file defines one. Unreadable files count as absent.
 */
export function configuredOctocodeServer(cwd: string, projectTrusted: boolean, agentDir = getAgentDir()): { enabled?: unknown } | undefined {
  let entry: { enabled?: unknown } | undefined;
  for (const file of [path.join(agentDir, 'mcp.json'), ...(projectTrusted ? [path.join(cwd, CONFIG_DIR_NAME, 'mcp.json')] : [])]) {
    try {
      const servers = (JSON.parse(fs.readFileSync(file, 'utf8')) as { mcpServers?: Record<string, unknown> }).mcpServers;
      const found = servers && typeof servers === 'object' ? servers[OCTOCODE_SERVER] : undefined;
      if (found && typeof found === 'object') entry = found as { enabled?: unknown };
    } catch {
      // Missing or malformed: Pi reports a malformed file itself.
    }
  }
  return entry;
}

/** Every string in a JSON value, sanitized; the same object when nothing changed. */
function sanitizeJson(value: unknown): unknown {
  if (typeof value === 'string') return sanitizeTerminalText(value);
  if (Array.isArray(value)) {
    const next = value.map(sanitizeJson);
    return next.some((item, i) => item !== value[i]) ? next : value;
  }
  if (value && typeof value === 'object') {
    let changed = false;
    const next: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value)) {
      next[key] = sanitizeJson(item);
      if (next[key] !== item) changed = true;
    }
    return changed ? next : value;
  }
  return value;
}

/**
 * Octocode returns repository, GitHub and package registry text verbatim, and any of it is untrusted: strip terminal control
 * sequences, bidi overrides and invisible characters (Unicode tags among them) from its text parts and structured
 * content before the model reads it or the terminal draws it. Undefined when the result is clean or not Octocode's.
 */
export function sanitizeOctocodeResult(event: Pick<ToolResultEvent, 'toolName' | 'content' | 'structuredContent'>): ToolResultEventResult | undefined {
  if (!isOctocodeTool(event.toolName)) return undefined;
  const content = sanitizeJson(event.content) as ToolResultEvent['content'];
  const structured = event.structuredContent === undefined ? undefined : sanitizeJson(event.structuredContent);
  if (content === event.content && structured === event.structuredContent) return undefined;
  // Replacing content alone would drop the structured content, so return both.
  return { content, ...(structured !== undefined ? { structuredContent: structured as ToolResultEventResult['structuredContent'] } : {}) };
}

/** Absolute path of the bundled octocode-mcp bin (its `exports` hide package.json, so walk up from the entry). */
function octocodeMcpEntry(): string | undefined {
  try {
    let dir = path.dirname(fileURLToPath(import.meta.resolve('octocode-mcp')));
    for (let depth = 0; depth < 4; depth++, dir = path.dirname(dir)) {
      const manifestPath = path.join(dir, 'package.json');
      if (!fs.existsSync(manifestPath)) continue;
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8')) as { name?: string; bin?: string | Record<string, string> };
      if (manifest.name !== 'octocode-mcp') continue;
      const bin = typeof manifest.bin === 'string' ? manifest.bin : manifest.bin?.['octocode-mcp'];
      return bin ? path.join(dir, bin) : undefined;
    }
  } catch {
    // Not installed next to the extension.
  }
  return undefined;
}
