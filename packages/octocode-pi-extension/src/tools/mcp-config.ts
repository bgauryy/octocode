import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { getDefaultEnvironment } from '@modelcontextprotocol/client/stdio';
import { ensurePrivateDirectory, hardenPrivateFile, getMcpEnablement, PRIVATE_FILE_MODE } from '@octocodeai/octocode-awareness/mcp-state';
import type { PiContext } from '../types.js';
import { getOctocodeHome } from '@octocodeai/config';
import { extensionWorkspaceRoot, extensionCacheRoot, extensionHome } from '../extension-paths.js';
import { discoverMcpSystem } from './mcp-discovery.js';

import { openOctocodeDb } from './storage-policy.js';

export interface McpServerConfig {
  transport?: 'stdio' | 'http';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  /** Destination environment key -> source process environment key. */
  envRefs?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
  /** HTTP header name -> source process environment key. */
  headerRefs?: Record<string, string>;
  /** Environment variable containing an HTTP bearer token (used by discovered Codex configs). */
  bearerTokenEnvVar?: string;
  auth?: 'none' | 'oauth';
  disabled?: boolean;
  description?: string;
  timeoutMs?: number;
  /** Present only for definitions imported read-only from another MCP host. */
  discovered?: {
    host: string;
    scope: 'project' | 'user';
    path: string;
    originalName: string;
  };
}

export interface McpConfigSource {
  scope: 'built-in' | 'project' | 'global' | 'discovered-project' | 'discovered-user';
  path: string;
  trusted: boolean;
  host?: string;
  /** Foreign files are definitions only; edit them in their owning host or copy to Octocode. */
  readOnly?: boolean;
}

export interface McpLoadedConfig {
  /** All parsed definitions before file/DB enablement is applied. */
  configuredServers: Map<string, McpServerConfig>;
  servers: Map<string, McpServerConfig>;
  /** Effective winning definition source for each configured server. */
  serverSources: Map<string, McpConfigSource>;
  sources: McpConfigSource[];
  warnings: string[];
}

export type McpScope = 'project' | 'global';

export interface McpConfigPathOptions {
  /** OS user home override, primarily for tests. */
  homeDir?: string;
  /** Octocode home override; defaults to getOctocodeHome(). */
  octocodeHome?: string;
}

export const DEFAULT_TIMEOUT_MS = 30_000;
export const DEFAULT_OCTOCODE_MCP_SERVER_NAME = 'octocode';
const DEFAULT_OCTOCODE_MCP_NPX_CACHE = path.join(extensionCacheRoot(), 'mcp-npx');

/**
 * Env defaults every octocode MCP server spawn must carry:
 * - OCTOCODE_MCP_FULL_TEXT: octocode-mcp compacts text content to a
 *   "structuredContent available …" stub for structured-content-aware clients;
 *   Pi's MCP surfaces only read text blocks, so full text must stay on or the
 *   model sees counts instead of data.
 * - ENABLE_LOCAL: turns on the local tool family (localSearch, localGetFileContent, etc.). Force
 *   it rather than trusting octocode-mcp's own internal default — if that
 *   upstream default ever flips, local tools must not silently disappear here.
 * - ENABLE_CLONE: enables ghCloneRepo so the agent can clone a repo once and
 *   use local tools for deep research instead of many ghGetFileContent calls.
 * - npm_config_*: ensure npx resolves the local cache with the native addon.
 * User-supplied env values always take precedence over these defaults.
 */
export const OCTOCODE_MCP_ENV_DEFAULTS: Record<string, string> = {
  OCTOCODE_MCP_FULL_TEXT: 'true',
  ENABLE_LOCAL: 'true',
  ENABLE_CLONE: 'true',
  npm_config_include: 'optional',
  npm_config_cache: DEFAULT_OCTOCODE_MCP_NPX_CACHE,
};

const AMBIENT_ENV_PASSTHROUGH = ['HTTP_PROXY', 'HTTPS_PROXY', 'NO_PROXY', 'NODE_EXTRA_CA_CERTS'];

/**
 * Resolve the pinned, locally-installed `octocode-mcp` entry (its bin === main
 * === dist/index.js) relative to this extension's own module. Returns null when
 * the dependency is not resolvable, so the caller can fall back to npx.
 *
 * octocode-mcp's package.json `exports` only defines the "import" condition, so
 * require.resolve fails — use import.meta.resolve (ESM), which honours it.
 */
function resolveLocalOctocodeMcpBin(): string | null {
  try {
    const url = import.meta.resolve('octocode-mcp');
    const binPath = fileURLToPath(url);
    return fs.existsSync(binPath) ? binPath : null;
  } catch {
    return null;
  }
}

// Version kept in sync with package.json#dependencies.octocode-mcp.
const OCTOCODE_MCP_FALLBACK_VERSION = '^18.3.0';

/**
 * Build the built-in Octocode MCP server spawn config. Prefer the pinned local
 * binary (fast, offline, reproducible against the version we ship); fall back to
 * `npx -y octocode-mcp@OCTOCODE_MCP_FALLBACK_VERSION` (cache-first) when the dependency is unresolvable.
 */
export function buildDefaultOctocodeMcpServer(): McpServerConfig {
  const localBin = resolveLocalOctocodeMcpBin();
  const spawn = localBin
    ? { command: process.execPath, args: [localBin] }
    : // No --prefer-online: use the extension-owned npm cache.
      // for sub-100ms cold start when the pinned dep is unavailable.
      { command: 'npx', args: ['-y', `octocode-mcp@${OCTOCODE_MCP_FALLBACK_VERSION}`] };
  return {
    transport: 'stdio',
    ...spawn,
    env: { ...OCTOCODE_MCP_ENV_DEFAULTS },
    description: 'Built-in Octocode MCP server (lazy stdio bridge, pinned-local first).',
    timeoutMs: DEFAULT_TIMEOUT_MS,
  };
}

/**
 * Ambient env forwarded to every MCP server: the SDK's minimal safe default
 * (PATH, HOME, …) plus proxy/CA settings. Process secrets are NOT inherited —
 * a server that needs a token must receive it explicitly via its mcp.json
 * `env`. The built-in octocode server additionally gets its own OCTOCODE_* and
 * GitHub auth vars, since its research tools authenticate from the ambient env.
 */
export function buildServerEnv(name: string, config: McpServerConfig): Record<string, string> {
  const base = getDefaultEnvironment();
  for (const key of AMBIENT_ENV_PASSTHROUGH) {
    const value = process.env[key];
    if (value) base[key] = value;
  }
  if (name === DEFAULT_OCTOCODE_MCP_SERVER_NAME) {
    for (const [key, value] of Object.entries(process.env)) {
      if (!value) continue;
      if (key.startsWith('OCTOCODE_') || key === 'GITHUB_TOKEN' || key === 'GH_TOKEN') base[key] = value;
    }
  }
  const referenced: Record<string, string> = {};
  for (const [destination, source] of Object.entries(config.envRefs ?? {})) {
    const value = process.env[source];
    if (value === undefined) throw new Error(`MCP environment reference ${source} for ${destination} is not set`);
    referenced[destination] = value;
  }
  return { ...base, ...(config.env ?? {}), ...referenced };
}

export function buildServerHeaders(config: McpServerConfig): Record<string, string> {
  const referenced: Record<string, string> = {};
  for (const [header, source] of Object.entries(config.headerRefs ?? {})) {
    const value = process.env[source];
    if (value === undefined) throw new Error(`MCP header reference ${source} for ${header} is not set`);
    referenced[header] = value;
  }
  if (config.bearerTokenEnvVar) {
    const value = process.env[config.bearerTokenEnvVar];
    if (value === undefined) throw new Error(`MCP bearer token reference ${config.bearerTokenEnvVar} is not set`);
    referenced['Authorization'] = `Bearer ${value}`;
  }
  return { ...(config.headers ?? {}), ...referenced };
}

export function projectMcpPath(cwd: string, octocodeHome = getOctocodeHome()): string {
  return path.join(extensionWorkspaceRoot(cwd, octocodeHome), 'mcp', 'servers.json');
}

export function globalMcpPath(_homeDir = os.homedir(), octocodeHome = getOctocodeHome()): string {
  return path.join(extensionHome(octocodeHome), 'mcp', 'servers.json');
}

/** The single canonical global MCP server-definition file. */
export function globalMcpConfigPaths(options: McpConfigPathOptions = {}): string[] {
  const octocodeHome = options.octocodeHome ?? getOctocodeHome();
  return [globalMcpPath(options.homeDir, octocodeHome)];
}

/** The single canonical project MCP server-definition file. */
export function projectMcpConfigPaths(cwd: string, octocodeHome = getOctocodeHome()): string[] {
  return [projectMcpPath(cwd, octocodeHome)];
}

export function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseStringArray(value: unknown): string[] | undefined {
  if (value === undefined) return undefined;
  if (!Array.isArray(value)) throw new Error('args must be an array of strings');
  return value.map((item) => {
    if (typeof item !== 'string') throw new Error('args must be an array of strings');
    return item;
  });
}

function parseStringRecord(value: unknown, label: string): Record<string, string> | undefined {
  if (value === undefined) return undefined;
  if (!isPlainRecord(value)) throw new Error(`${label} must be an object`);
  const out: Record<string, string> = {};
  for (const [key, raw] of Object.entries(value)) {
    if (typeof raw !== 'string' || raw.length === 0) throw new Error(`${label}.${key} must be a non-empty string`);
    out[key] = raw;
  }
  return out;
}

function parseServerConfig(name: string, value: unknown): McpServerConfig {
  if (!/^[A-Za-z0-9_.-]{1,64}$/.test(name)) {
    throw new Error(`invalid server name ${JSON.stringify(name)}; use letters, numbers, _, -, or .`);
  }
  if (!isPlainRecord(value)) throw new Error(`server ${name} must be an object`);
  const rawUrl = value['url'];
  const rawCommand = value['command'];
  const isHttp = typeof rawUrl === 'string' && rawUrl.trim().length > 0;
  if (!isHttp && (typeof rawCommand !== 'string' || rawCommand.trim().length === 0))
    throw new Error(`server ${name} requires either a non-empty command or an http(s) url`);
  if (isHttp) {
    const parsedUrl = new URL(rawUrl as string);
    if (parsedUrl.protocol !== 'http:' && parsedUrl.protocol !== 'https:') throw new Error(`server ${name}.url must use http or https`);
    if (rawCommand !== undefined) throw new Error(`server ${name} cannot define both command and url`);
  }
  const timeoutMs = value['timeoutMs'];
  return {
    transport: isHttp ? 'http' : 'stdio',
    command: isHttp ? undefined : String(rawCommand),
    args: parseStringArray(value['args']),
    env: parseStringRecord(value['env'], 'env'),
    envRefs: parseStringRecord(value['envRefs'], 'envRefs'),
    cwd: value['cwd'] === undefined ? undefined : String(value['cwd']),
    url: isHttp ? String(rawUrl) : undefined,
    headers: parseStringRecord(value['headers'], 'headers'),
    headerRefs: parseStringRecord(value['headerRefs'], 'headerRefs'),
    auth: value['auth'] === 'oauth' ? 'oauth' : 'none',
    disabled: value['disabled'] === true,
    description: value['description'] === undefined ? undefined : String(value['description']),
    timeoutMs: timeoutMs === undefined ? undefined : Math.max(1_000, Math.min(120_000, Number(timeoutMs))),
  };
}

function parseConfigText(text: string): Map<string, McpServerConfig> {
  const json = JSON.parse(text) as unknown;
  if (!isPlainRecord(json)) throw new Error('mcp.json must contain an object');
  const rawServers = isPlainRecord(json['mcpServers'])
    ? json['mcpServers']
    : isPlainRecord(json['servers'])
      ? json['servers']
      : json;
  const servers = new Map<string, McpServerConfig>();
  for (const [name, raw] of Object.entries(rawServers)) {
    const server = parseServerConfig(name, raw);
    servers.set(name, server);
  }
  return servers;
}

function readConfigFile(filePath: string): Map<string, McpServerConfig> | null {
  if (!fs.existsSync(filePath)) return null;
  return parseConfigText(fs.readFileSync(filePath, 'utf8'));
}

export function scopeTargetPath(scope: McpScope, ctx?: PiContext): string {
  return scope === 'global' ? globalMcpPath() : projectMcpPath(ctx?.cwd ?? process.cwd());
}

/**
 * Return the container object inside a parsed mcp.json that holds the server map,
 * preserving the file's existing shape (`mcpServers` > `servers` > root object).
 */
function serverContainer(raw: Record<string, unknown>): Record<string, unknown> {
  if (isPlainRecord(raw['mcpServers'])) return raw['mcpServers'] as Record<string, unknown>;
  if (isPlainRecord(raw['servers'])) return raw['servers'] as Record<string, unknown>;
  // New/empty file: standardize on the canonical `mcpServers` wrapper.
  const container: Record<string, unknown> = {};
  raw['mcpServers'] = container;
  return container;
}

function writeMcpJsonAtomic(filePath: string, raw: unknown): void {
  ensurePrivateDirectory(path.dirname(filePath));
  hardenPrivateFile(filePath);
  const tmp = `${filePath}.${process.pid}.${Date.now()}.tmp`;
  fs.writeFileSync(tmp, JSON.stringify(raw, null, 2) + '\n', { encoding: 'utf8', mode: PRIVATE_FILE_MODE, flag: 'wx' });
  fs.renameSync(tmp, filePath);
  hardenPrivateFile(filePath);
}

/** Insert or update a server in an mcp.json file. Validates via parseServerConfig. */
export function upsertServerInFile(filePath: string, name: string, serverJson: Record<string, unknown>): McpServerConfig {
  const parsed = parseServerConfig(name, serverJson); // throws on invalid name/command
  let raw: Record<string, unknown> = {};
  if (fs.existsSync(filePath)) {
    const text = fs.readFileSync(filePath, 'utf8').trim();
    if (text) {
      const json = JSON.parse(text) as unknown;
      if (!isPlainRecord(json)) throw new Error('mcp.json must contain an object');
      raw = json;
    }
  }
  const container = serverContainer(raw);
  // Persist only defined fields, in a stable shape.
  const entry: Record<string, unknown> = parsed.transport === 'http'
    ? { url: parsed.url }
    : { command: parsed.command };
  if (parsed.args && parsed.args.length) entry['args'] = parsed.args;
  if (parsed.env && Object.keys(parsed.env).length) entry['env'] = parsed.env;
  if (parsed.envRefs && Object.keys(parsed.envRefs).length) entry['envRefs'] = parsed.envRefs;
  if (parsed.headers && Object.keys(parsed.headers).length) entry['headers'] = parsed.headers;
  if (parsed.headerRefs && Object.keys(parsed.headerRefs).length) entry['headerRefs'] = parsed.headerRefs;
  if (parsed.auth === 'oauth') entry['auth'] = 'oauth';
  if (parsed.cwd) entry['cwd'] = parsed.cwd;
  if (parsed.timeoutMs) entry['timeoutMs'] = parsed.timeoutMs;
  if (parsed.description) entry['description'] = parsed.description;
  if (parsed.disabled) entry['disabled'] = true;
  container[name] = entry;
  writeMcpJsonAtomic(filePath, raw);
  return parsed;
}

/** Remove a server from an mcp.json file. Returns false if it wasn't present. */
export function removeServerFromFile(filePath: string, name: string): boolean {
  if (!fs.existsSync(filePath)) return false;
  const text = fs.readFileSync(filePath, 'utf8').trim();
  if (!text) return false;
  const json = JSON.parse(text) as unknown;
  if (!isPlainRecord(json)) return false;
  const container = isPlainRecord(json['mcpServers'])
    ? (json['mcpServers'] as Record<string, unknown>)
    : isPlainRecord(json['servers'])
      ? (json['servers'] as Record<string, unknown>)
      : json;
  if (!(name in container)) return false;
  delete container[name];
  writeMcpJsonAtomic(filePath, json);
  return true;
}

export async function loadMcpConfig(
  ctx?: PiContext,
  pathOptions: McpConfigPathOptions = {},
): Promise<McpLoadedConfig> {
  const cwd = ctx?.cwd ?? process.cwd();
  const trusted = ctx?.isProjectTrusted ? Boolean(await ctx.isProjectTrusted()) : true;
  const defaultServer = buildDefaultOctocodeMcpServer();
  const servers = new Map<string, McpServerConfig>([[DEFAULT_OCTOCODE_MCP_SERVER_NAME, defaultServer]]);
  const sourcePath = defaultServer.command === 'npx' ? `npx -y octocode-mcp@${OCTOCODE_MCP_FALLBACK_VERSION}` : `node ${defaultServer.args?.[0] ?? 'octocode-mcp'}`;
  const builtInSource: McpConfigSource = { scope: 'built-in', path: sourcePath, trusted: true };
  const sources: McpConfigSource[] = [builtInSource];
  const serverSources = new Map<string, McpConfigSource>([[DEFAULT_OCTOCODE_MCP_SERVER_NAME, builtInSource]]);
  const warnings: string[] = [];

  // Foreign host configurations are discoverable definitions, never implicit authority.
  // They enter the effective catalog disabled and can only run after an explicit SQLite
  // enablement override. Project definitions additionally require project trust.
  const discovered = discoverMcpSystem(cwd, pathOptions);
  const sourceByPath = new Map<string, McpConfigSource>();
  for (const config of discovered.configs.filter((item) => !item.active)) {
    const allowed = config.scope === 'user' || trusted;
    const source: McpConfigSource = {
      scope: config.scope === 'project' ? 'discovered-project' : 'discovered-user',
      path: config.path,
      trusted: allowed,
      host: config.host,
      readOnly: true,
    };
    sources.push(source);
    sourceByPath.set(config.path, source);
    if (config.error) warnings.push(`${config.path}: ${config.error}`);
    if (!allowed) warnings.push(`${config.path}: discovered but disabled because the project is not trusted`);
  }
  for (const definition of discovered.definitions) {
    const metadata = definition.config.discovered;
    if (metadata.scope === 'project' && !trusted) continue;
    const source = sourceByPath.get(metadata.path);
    if (!source) continue;
    servers.set(definition.name, definition.config);
    serverSources.set(definition.name, source);
  }

  for (const candidate of globalMcpConfigPaths(pathOptions)) {
    try {
      const globalServers = readConfigFile(candidate);
      if (globalServers) {
        const source: McpConfigSource = { scope: 'global', path: candidate, trusted: true };
        sources.push(source);
        for (const [name, config] of globalServers) {
          servers.set(name, config);
          serverSources.set(name, source);
        }
      }
    } catch (error) {
      warnings.push(`${candidate}: ${(error as Error).message}`);
    }
  }

  for (const candidate of projectMcpConfigPaths(cwd, pathOptions.octocodeHome ?? getOctocodeHome())) {
    if (!fs.existsSync(candidate)) continue;
    if (!trusted) {
      sources.push({ scope: 'project', path: candidate, trusted: false });
      warnings.push(`${candidate}: skipped because the project is not trusted`);
      continue;
    }
    try {
      const projectServers = readConfigFile(candidate);
      if (projectServers) {
        const source: McpConfigSource = { scope: 'project', path: candidate, trusted: true };
        sources.push(source);
        for (const [name, config] of projectServers) {
          servers.set(name, config);
          serverSources.set(name, source);
        }
      }
    } catch (error) {
      warnings.push(`${candidate}: ${(error as Error).message}`);
    }
  }

  const configuredServers = new Map(servers);
  try {
    const db = openOctocodeDb();
    const scopeKey = path.resolve(cwd);
    for (const [name, config] of servers) {
      if (!getMcpEnablement(db, scopeKey, name, undefined, !config.disabled)) servers.delete(name);
    }
  } catch (error) {
    warnings.push(`MCP enablement database unavailable: ${(error as Error).message}`);
    for (const [name, config] of servers) if (config.disabled) servers.delete(name);
  }
  return { configuredServers, servers, serverSources, sources, warnings };
}

export function resolveServerCwd(config: McpServerConfig, ctx?: PiContext): string {
  const base = ctx?.cwd ?? process.cwd();
  if (!config.cwd) return base;
  return path.isAbsolute(config.cwd) ? config.cwd : path.resolve(base, config.cwd);
}

export function requestOptions(config: McpServerConfig, signal?: AbortSignal): { timeout: number; signal?: AbortSignal } {
  const timeout = config.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  return signal ? { timeout, signal } : { timeout };
}

export function normalizeServerConfig(name: string, config: McpServerConfig): McpServerConfig {
  if (name !== DEFAULT_OCTOCODE_MCP_SERVER_NAME) return config;
  // Always ensure full-text responses + the npm cache path; user env wins.
  return {
    ...config,
    env: { ...OCTOCODE_MCP_ENV_DEFAULTS, ...(config.env ?? {}) },
  };
}

/** Stable signature of the fields that determine the spawned process, for drift detection. */
export function configSignature(config: McpServerConfig): string {
  return JSON.stringify({
    command: config.command,
    args: config.args ?? [],
    env: config.env ?? {},
    envRefs: config.envRefs ?? {},
    cwd: config.cwd ?? null,
    timeoutMs: config.timeoutMs ?? null,
    transport: config.transport,
    url: config.url ?? null,
    headers: config.headers ?? {},
    headerRefs: config.headerRefs ?? {},
    bearerTokenEnvVar: config.bearerTokenEnvVar ?? null,
    auth: config.auth ?? 'none',
  });
}

/**
 * Concise stderr warning for best-effort MCP paths. Never throws and never
 * touches the TUI (stderr only) — it makes an otherwise-silent config failure
 * observable in logs/debug output without blocking session start.
 */
function warnMcp(message: string): void {
  try { process.stderr.write(`[octocode-mcp] ${message}\n`); } catch { /* stderr unavailable */ }
}

export function patchGlobalMcpOctocodeEnv(configPath = globalMcpPath()): void {
  try {
    if (!fs.existsSync(configPath)) return; // No global mcp.json — nothing to patch.

    // Re-parse as raw JSON so we can write it back with minimal diff.
    let raw: Record<string, unknown>;
    try { raw = JSON.parse(fs.readFileSync(configPath, 'utf8')); }
    catch { warnMcp(`global mcp.json is not valid JSON (${configPath}); skipping env patch`); return; }

    const servers = raw['mcpServers'];
    if (!isPlainRecord(servers)) return;
    const entry = servers[DEFAULT_OCTOCODE_MCP_SERVER_NAME];
    if (!isPlainRecord(entry)) return;

    // Check whether every required env var is already present.
    const env = isPlainRecord(entry['env']) ? entry['env'] : {};
    const missing = Object.keys(OCTOCODE_MCP_ENV_DEFAULTS).filter(
      (key) => !(typeof env[key] === 'string' && (env[key] as string).length > 0),
    );
    if (missing.length === 0) return; // Already patched.

    // Merge — user-supplied values take precedence.
    entry['env'] = { ...OCTOCODE_MCP_ENV_DEFAULTS, ...env };
    servers[DEFAULT_OCTOCODE_MCP_SERVER_NAME] = entry;
    raw['mcpServers'] = servers;
    writeMcpJsonAtomic(configPath, raw);
  } catch (err) {
    // Must not block session start, but make the failure observable.
    warnMcp(`failed to patch global mcp.json env: ${(err as Error)?.message ?? String(err)}`);
  }
}
