import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { CONFIG_DIR_NAME, getAgentDir, type ExtensionAPI, type ExtensionContext, type ToolDefinition } from '@earendil-works/pi-coding-agent';
import { MCP_ENV, envFlag } from '../shared/env.js';
import { octocodeHome } from '../shared/home.js';
import { findManifest, packageRoot } from '../shared/package.js';
import { OCTOCODE_SERVER, OCTOCODE_TOOL_PREFIX } from '../shared/octocode-tools.js';
import { visibleControls } from '../shared/sanitize.js';
import type { Tool } from '@earendil-works/pi-mcp';
import { OctocodeServer, type Launch, type ServerStatus } from './client.js';
import { octocodeToolDefinition, TurnBudget } from './tools.js';
import { notify } from '../shared/commands.js';

/**
 * Octocode MCP (local code, LSP, GitHub and package registry research) runs on this extension's own MCP client
 * (`OctocodeServer`): it starts the `octocode-mcp` server at `session_start`, registers every tool the server lists as
 * `mcp__octocode__<name>` and keeps the server's instructions for the prompt. A server named `octocode` in Pi's
 * `mcp.json` (`~/.pi/agent/mcp.json`, `.pi/mcp.json`) takes precedence, which is how users override or disable it: then
 * Pi's MCP owns the server and this extension starts nothing.
 */
/** The npm package of the server this extension runs. */
const MCP_PACKAGE = 'octocode-mcp';

/** The `octocode-mcp` range this package's `package.json` depends on (the one source of the version this extension runs). */
function octocodeMcpRange(): string {
  const manifest = JSON.parse(fs.readFileSync(path.join(packageRoot(), 'package.json'), 'utf8')) as { dependencies?: Record<string, string> };
  const range = manifest.dependencies?.[MCP_PACKAGE];
  if (!range) throw new Error('package.json has no octocode-mcp dependency');
  return range;
}

/**
 * The octocode-mcp releases this extension runs, as an npx spec: the `package.json` dependency range, so installing or
 * updating the extension and the npx fallback fetch the same newest compatible server; in this monorepo `dev setup`
 * resolves the dependency to the workspace build (`packages/octocode-mcp`).
 */
export const OCTOCODE_MCP_SPEC = `${MCP_PACKAGE}@${octocodeMcpRange()}`;

/** The server process: the installed octocode-mcp bin under this Node, else the newest npm release through npx. */
export function octocodeLaunch(cwd: string, env: NodeJS.ProcessEnv = process.env, entry = octocodeMcpEntry()): Launch {
  // Octocode reads only under the home directory unless told otherwise: allow the workspace and the Octocode home.
  // Tokens (GITHUB_TOKEN, …) need no forwarding: the server inherits this process's environment.
  const allowed = [...new Set([cwd, octocodeHome(env)])].join(',');
  const command = entry ? { command: process.execPath, args: [entry] } : { command: 'npx', args: ['-y', OCTOCODE_MCP_SPEC] };
  return { ...command, cwd, env: { WORKSPACE_ROOT: cwd, ALLOWED_PATHS: allowed } };
}

/**
 * The longest the first prompt waits for a connect in progress, so the server's instructions are in that request. A
 * failed connect or the user's Esc ends the wait at once; a slower start (a first `npx` download) joins a later prompt.
 */
const CONNECT_WAIT_MS = 10_000;

/** Whether the Octocode MCP server is switched on for this process (`OCTOCODE_MCP=0` turns it off). */
const octocodeMcpEnabled = (env: NodeJS.ProcessEnv = process.env): boolean => envFlag(env, MCP_ENV, true);

/**
 * The `octocode` entry of Pi's `mcp.json` (global, then the trusted project's replacement or settings override), which
 * takes precedence over the registration; undefined when neither file defines one. Unreadable files count as absent.
 */
export function configuredOctocodeServer(cwd: string, projectTrusted: boolean, agentDir = getAgentDir()): { enabled?: unknown } | undefined {
  let entry: Record<string, unknown> | undefined;
  for (const file of [path.join(agentDir, 'mcp.json'), ...(projectTrusted ? [path.join(cwd, CONFIG_DIR_NAME, 'mcp.json')] : [])]) {
    try {
      const servers = (JSON.parse(fs.readFileSync(file, 'utf8')) as { mcpServers?: Record<string, unknown> }).mcpServers;
      const found = servers && typeof servers === 'object' ? servers[OCTOCODE_SERVER] : undefined;
      if (found && typeof found === 'object' && !Array.isArray(found)) {
        const config = found as Record<string, unknown>;
        // Pi project entries without a transport override only enabled/exposure/toolExposure of the global server.
        if (file !== path.join(agentDir, 'mcp.json') && config['command'] === undefined && config['url'] === undefined && config['type'] === undefined) {
          if (entry && Object.keys(config).every((key) => ['enabled', 'exposure', 'toolExposure'].includes(key))) entry = { ...entry, ...config };
        } else entry = config;
      }
    } catch {
      // Missing or malformed: Pi reports a malformed file itself.
    }
  }
  return entry;
}

/** Who runs the `octocode` server: the user (an `octocode` entry in `mcp.json`, through Pi's MCP), this extension, or nobody. */
export type OctocodeOwner = { by: 'user'; enabled: boolean } | { by: 'extension' } | { by: 'off' };

/**
 * The Octocode server for a session, on this extension's MCP client: started at `session_start`, its tools registered
 * with Pi, its instructions kept for the prompt, closed at `session_shutdown`.
 */
export class OctocodeMcp {
  private readonly server: OctocodeServer;
  /** What is registered with Pi under each tool name; Pi cannot unregister, so a withdrawn tool is registered hidden. */
  private readonly registered = new Map<string, { tool: Tool; definition: ToolDefinition }>();
  private ctx: ExtensionContext | undefined;
  /** The text this turn's results have put in the context; `registerOctocodeMcp` resets it at every `turn_start`. */
  readonly turn = new TurnBudget();
  /** Who ran the server when the session started. */
  private owned: OctocodeOwner = { by: 'off' };

  private readonly env: NodeJS.ProcessEnv;
  private readonly connectWaitMs: number;

  constructor(
    private readonly pi: Pick<ExtensionAPI, 'registerTool'>,
    options: { env?: NodeJS.ProcessEnv; version?: string; launch?: (cwd: string) => Launch; connectWaitMs?: number } = {},
  ) {
    this.env = options.env ?? process.env;
    this.connectWaitMs = options.connectWaitMs ?? CONNECT_WAIT_MS;
    this.server = new OctocodeServer({
      launch: options.launch ?? ((cwd) => octocodeLaunch(cwd, this.env)),
      clientInfo: { name: 'octocode-pi-extension', version: options.version ?? '0' },
      onTools: (tools, instructions) => this.register(tools, instructions),
      onFailure: (message) => this.notifyFailure(message),
      onRefreshFailure: (message) => this.notify(`Octocode MCP could not refresh its tool list: ${message}\nTools it added since may be missing this session. See /octocode mcp.`),
    });
  }

  /** The user's `mcp.json` entry wins; else this extension runs the server unless `OCTOCODE_MCP=0`. */
  owner(ctx: Pick<ExtensionContext, 'cwd' | 'isProjectTrusted'>): OctocodeOwner {
    const configured = configuredOctocodeServer(ctx.cwd, ctx.isProjectTrusted?.() ?? false);
    if (configured) return { by: 'user', enabled: configured.enabled !== false };
    return { by: octocodeMcpEnabled(this.env) ? 'extension' : 'off' };
  }

  /** Starts the server for this session's folder (without waiting), or stops it when the session has none to run. */
  async start(ctx: ExtensionContext): Promise<void> {
    this.ctx = ctx;
    this.owned = this.owner(ctx);
    if (this.owned.by === 'extension') this.server.start(ctx.cwd);
    else await this.server.close();
  }

  /** Waits for the server to connect (see CONNECT_WAIT_MS); false when it is not connected. */
  ready(ctx: ExtensionContext): Promise<boolean> {
    this.ctx = ctx;
    return this.server.ready(this.connectWaitMs, ctx.signal);
  }

  /** The server's `initialize` instructions (how its tools work together), sanitized: the server's text is not trusted. */
  get instructions(): string | undefined {
    const text = this.server.instructions;
    return text ? visibleControls(text).trim() || undefined : undefined;
  }

  /** Stops the server and starts it again for this session, whatever state it was in. */
  async restart(ctx: ExtensionContext): Promise<ServerStatus> {
    this.ctx = ctx;
    await this.server.restart(ctx.cwd);
    return this.server.status();
  }

  status(): ServerStatus {
    return this.server.status();
  }

  /** `/octocode mcp`: what runs the server and how it is doing; the first line is the summary. */
  describe(): string {
    const owner = this.owned;
    if (owner.by === 'user') return `Octocode MCP: your own \`octocode\` server from mcp.json (${owner.enabled ? "Pi's MCP, see /mcp" : 'disabled there'})`;
    if (owner.by === 'off') return `Octocode MCP: off (${MCP_ENV}=0)`;
    const status = this.server.status();
    const server = status.serverName ? `${status.serverName} ${status.serverVersion ?? ''}`.trim() : 'octocode-mcp';
    switch (status.state) {
      case 'connected':
        return `Octocode MCP: connected (${server}, ${status.tools} tools, pid ${status.pid ?? '?'})`;
      case 'connecting':
        return 'Octocode MCP: connecting…';
      case 'failed':
        return `Octocode MCP: failed to start: ${status.error ?? 'unknown error'}\nRetry with /octocode mcp restart.`;
      case 'disconnected':
        return `Octocode MCP: stopped: ${status.error ?? 'the server exited'}\nThe next tool call starts it again once; or run /octocode mcp restart.`;
      default:
        return 'Octocode MCP: not started';
    }
  }

  /** Stops the server and its process tree. */
  async close(): Promise<void> {
    this.ctx = undefined;
    await this.server.close();
  }

  /** Registers every listed tool with Pi, and withdraws the ones the server no longer lists. */
  private register(tools: readonly Tool[], instructions: string | undefined): void {
    const namespace = { name: OCTOCODE_TOOL_PREFIX.replace(/__$/, ''), ...(instructions ? { instructions: visibleControls(instructions).trim() } : {}) };
    const listed = new Set<string>();
    for (const tool of tools) {
      const name = `${OCTOCODE_TOOL_PREFIX}${tool.name}`;
      listed.add(name);
      const previous = this.registered.get(name);
      // Pi rebuilds the complete registry per registration. A reconnect or an unrelated tool change needs no rebuild.
      if (previous && isDeepStrictEqual(previous.tool, tool) && isDeepStrictEqual(previous.definition.namespace, namespace)) continue;
      const definition = octocodeToolDefinition(tool, { prefix: OCTOCODE_TOOL_PREFIX, namespace, call: (name, args, signal) => this.server.call(name, args, signal), budget: this.turn });
      this.registered.set(definition.name, { tool, definition });
      this.pi.registerTool(definition);
    }
    for (const [name, { definition }] of this.registered) {
      if (listed.has(name)) continue;
      this.registered.delete(name);
      this.pi.registerTool({ ...definition, exposure: 'hidden' });
    }
  }

  private notifyFailure(message: string): void {
    this.notify(`Octocode MCP failed to start: ${message}\nThe model has no Octocode tools this session. See /octocode mcp.`);
  }

  private notify(text: string): void {
    const ctx = this.ctx;
    if (!ctx?.hasUI) return;
    try {
      notify(ctx.ui, text, 'warning');
    } catch {
      // The session ended before the server gave up.
    }
  }
}

/**
 * Wires the Octocode server to the session: its client starts at `session_start` (`OctocodeMcp.start` decides whether
 * this extension or the user's `mcp.json` runs it) and closes at `session_shutdown`.
 */
export function registerOctocodeMcp(pi: ExtensionAPI, version?: string): OctocodeMcp {
  const mcp = new OctocodeMcp(pi, { ...(version ? { version } : {}) });
  pi.on('session_start', async (_event, ctx) => mcp.start(ctx));
  pi.on('session_shutdown', async () => mcp.close());
  pi.on('turn_start', async () => mcp.turn.reset());
  return mcp;
}

/** Absolute path of the bundled octocode-mcp bin, read from its manifest (the launcher package has no `main` or `exports`). */
function octocodeMcpEntry(): string | undefined {
  try {
    const found = findManifest(path.dirname(fileURLToPath(import.meta.resolve(`${MCP_PACKAGE}/package.json`))), MCP_PACKAGE, 1);
    const bin = typeof found?.manifest.bin === 'string' ? found.manifest.bin : found?.manifest.bin?.[MCP_PACKAGE];
    return found && bin ? path.join(found.dir, bin) : undefined;
  } catch {
    // Not installed next to the extension.
  }
  return undefined;
}
