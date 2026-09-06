import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { getOctocodeHome, workspaceAgentRoot } from './paths.js';

export interface DiscoveredMcpServerConfig {
  transport?: 'stdio' | 'http';
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  envRefs?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
  headerRefs?: Record<string, string>;
  bearerTokenEnvVar?: string;
  auth?: 'none' | 'oauth';
  disabled: true;
  description?: string;
  timeoutMs?: number;
  discovered: {
    host: string;
    scope: 'project' | 'user';
    path: string;
    originalName: string;
  };
}

export interface DiscoveredMcpConfig {
  path: string;
  host: string;
  scope: 'project' | 'user';
  format: 'json' | 'toml';
  /** Canonical Octocode definitions are active; foreign definitions require an explicit DB override. */
  active: boolean;
  servers: Array<{ name: string; command?: string }>;
  error?: string;
}

export interface DiscoveredMcpDefinition {
  name: string;
  config: DiscoveredMcpServerConfig;
}

export interface McpDiscoveryResult {
  configs: DiscoveredMcpConfig[];
  definitions: DiscoveredMcpDefinition[];
}

export interface DiscoverMcpConfigOptions {
  homeDir?: string;
  octocodeHome?: string;
}

interface Candidate {
  path: string;
  host: string;
  scope: 'project' | 'user';
  format: 'json' | 'toml';
  active: boolean;
  allowRootServers?: boolean;
  claudeState?: boolean;
}

type JsonRecord = Record<string, unknown>;
const MAX_CONFIG_BYTES = 1024 * 1024;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function stringRecord(value: unknown): Record<string, string> | undefined {
  if (!isRecord(value)) return undefined;
  const entries = Object.entries(value).filter((entry): entry is [string, string] => typeof entry[1] === 'string');
  return entries.length ? Object.fromEntries(entries) : undefined;
}

function stringArray(value: unknown): string[] | undefined {
  return Array.isArray(value) && value.every((item) => typeof item === 'string') ? value : undefined;
}

function splitReferences(value: unknown): { values?: Record<string, string>; refs?: Record<string, string> } {
  const record = stringRecord(value);
  if (!record) return {};
  const values: Record<string, string> = {};
  const refs: Record<string, string> = {};
  for (const [key, raw] of Object.entries(record)) {
    const match = raw.match(/^\$\{(?:env:)?([A-Za-z_][A-Za-z0-9_]*)\}$/);
    if (match) refs[key] = match[1]!;
    else values[key] = raw;
  }
  return {
    ...(Object.keys(values).length ? { values } : {}),
    ...(Object.keys(refs).length ? { refs } : {}),
  };
}

function normalizeServer(raw: JsonRecord): Omit<DiscoveredMcpServerConfig, 'disabled' | 'discovered'> | null {
  const rawUrl = typeof raw['url'] === 'string'
    ? raw['url']
    : typeof raw['httpUrl'] === 'string'
      ? raw['httpUrl']
      : typeof raw['serverUrl'] === 'string'
        ? raw['serverUrl']
        : undefined;
  const url = rawUrl?.trim();
  const command = typeof raw['command'] === 'string' ? raw['command'].trim() : undefined;
  if (!url && !command) return null;
  if (url) {
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return null;
    } catch { return null; }
  }
  const timeout = Number(raw['timeoutMs'] ?? raw['timeout']);
  const env = splitReferences(raw['env']);
  const headers = splitReferences(raw['headers'] ?? raw['http_headers']);
  const explicitHeaderRefs = stringRecord(raw['headerRefs'] ?? raw['env_http_headers']);
  const envVars = stringArray(raw['env_vars']);
  const envRefs = {
    ...(env.refs ?? {}),
    ...Object.fromEntries((envVars ?? []).filter((key) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(key)).map((key) => [key, key])),
  };
  const headerRefs = { ...(headers.refs ?? {}), ...(explicitHeaderRefs ?? {}) };
  return {
    transport: url ? 'http' : 'stdio',
    ...(url ? { url } : { command }),
    ...(stringArray(raw['args']) ? { args: stringArray(raw['args']) } : {}),
    ...(env.values ? { env: env.values } : {}),
    ...(Object.keys(envRefs).length ? { envRefs } : {}),
    ...(typeof raw['cwd'] === 'string' ? { cwd: raw['cwd'] } : {}),
    ...(headers.values ? { headers: headers.values } : {}),
    ...(Object.keys(headerRefs).length ? { headerRefs } : {}),
    ...(typeof raw['bearer_token_env_var'] === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(raw['bearer_token_env_var'])
      ? { bearerTokenEnvVar: raw['bearer_token_env_var'] } : {}),
    ...((raw['auth'] === 'oauth' || isRecord(raw['oauth'])) ? { auth: 'oauth' as const } : {}),
    ...(typeof raw['description'] === 'string' ? { description: raw['description'] } : {}),
    ...(Number.isFinite(timeout) && timeout > 0 ? { timeoutMs: Math.max(1_000, Math.min(120_000, timeout)) } : {}),
  };
}

function jsonContainers(json: JsonRecord, candidate: Candidate, cwd: string): JsonRecord[] {
  const containers: JsonRecord[] = [];
  if (isRecord(json['mcpServers'])) containers.push(json['mcpServers']);
  else if (isRecord(json['servers'])) containers.push(json['servers']);
  else if (candidate.allowRootServers) containers.push(json);
  if (candidate.claudeState && isRecord(json['projects'])) {
    const exact = json['projects'][path.resolve(cwd)];
    if (isRecord(exact) && isRecord(exact['mcpServers'])) containers.push(exact['mcpServers']);
  }
  return containers;
}

function parseTomlValue(raw: string): unknown {
  const value = raw.trim();
  if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) return value.slice(1, -1);
  if (value.startsWith('{') && value.endsWith('}')) {
    const record: Record<string, string> = {};
    for (const match of value.matchAll(/([A-Za-z0-9_.-]+)\s*=\s*(?:"((?:\\.|[^"\\])*)"|'([^']*)')/g)) {
      record[match[1]!] = (match[2] ?? match[3] ?? '').replace(/\\"/g, '"');
    }
    return record;
  }
  if (value.startsWith('[') && value.endsWith(']')) {
    const matches = [...value.matchAll(/"((?:\\.|[^"\\])*)"|'([^']*)'/g)];
    return matches.map((match) => (match[1] ?? match[2] ?? '').replace(/\\"/g, '"'));
  }
  if (value === 'true') return true;
  if (value === 'false') return false;
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : undefined;
}

function parseToml(text: string): JsonRecord {
  const servers: JsonRecord = {};
  let currentName: string | undefined;
  let nested: 'env' | 'headers' | 'env_http_headers' | undefined;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) continue;
    const header = line.match(/^\[mcp_servers\.(?:"([^"]+)"|'([^']+)'|([A-Za-z0-9_-]+))(?:\.(env|http_headers|env_http_headers|headers))?\]$/);
    if (header) {
      currentName = header[1] ?? header[2] ?? header[3];
      nested = header[4] === 'env' ? 'env' : header[4] === 'env_http_headers' ? 'env_http_headers' : header[4] ? 'headers' : undefined;
      if (currentName && !isRecord(servers[currentName])) servers[currentName] = {};
      continue;
    }
    if (!currentName) continue;
    const assignment = line.match(/^([A-Za-z0-9_.-]+)\s*=\s*(.+)$/);
    if (!assignment) continue;
    const server = servers[currentName] as JsonRecord;
    const key = assignment[1]!;
    const value = parseTomlValue(assignment[2]!);
    if (value === undefined) continue;
    if (nested) {
      if (!isRecord(server[nested])) server[nested] = {};
      (server[nested] as JsonRecord)[key] = value;
    } else server[key] = value;
  }
  return servers;
}

function roots(options: string | DiscoverMcpConfigOptions | undefined): { homeDir: string; octocodeHome: string } {
  if (typeof options === 'string') return { homeDir: options, octocodeHome: path.join(options, '.octocode') };
  const homeDir = options?.homeDir ?? os.homedir();
  return {
    homeDir,
    octocodeHome: options?.octocodeHome ?? (options?.homeDir ? path.join(homeDir, '.octocode') : getOctocodeHome()),
  };
}

function candidates(cwd: string, options?: string | DiscoverMcpConfigOptions): Candidate[] {
  const { homeDir, octocodeHome } = roots(options);
  const project = (relative: string, host: string, format: 'json' | 'toml' = 'json'): Candidate => ({ path: path.join(cwd, relative), host, scope: 'project', format, active: false });
  const user = (relative: string, host: string, format: 'json' | 'toml' = 'json'): Candidate => ({ path: path.join(homeDir, relative), host, scope: 'user', format, active: false });
  return [
    { path: path.join(workspaceAgentRoot(cwd, octocodeHome), 'mcp', 'servers.json'), host: 'octocode', scope: 'project', format: 'json', active: true, allowRootServers: true },
    project('.pi/mcp.json', 'pi'), project('.pi/agent/mcp.json', 'pi'),
    project('.mcp.json', 'claude'), project('.claude/mcp.json', 'claude'),
    project('.cursor/mcp.json', 'cursor'), project('.codex/config.toml', 'codex', 'toml'),
    project('.gemini/settings.json', 'gemini'),
    project('.agents/mcp_config.json', 'antigravity'), project('.agents/mcp.json', 'agents'),
    project('.agent/mcp_config.json', 'agent'), project('.agent/mcp.json', 'agent'),
    project('.vscode/mcp.json', 'vscode'),
    { path: path.join(octocodeHome, 'agent', 'mcp', 'servers.json'), host: 'octocode', scope: 'user', format: 'json', active: true, allowRootServers: true },
    user('.pi/mcp.json', 'pi'), user('.pi/agent/mcp.json', 'pi'),
    { ...user('.claude.json', 'claude'), claudeState: true }, user('.claude/mcp.json', 'claude'),
    user('.cursor/mcp.json', 'cursor'), user('.codex/config.toml', 'codex', 'toml'),
    user('.gemini/settings.json', 'gemini'), user('.copilot/mcp-config.json', 'copilot'),
    user('.agents/mcp_config.json', 'antigravity'), user('.agents/mcp.json', 'agents'),
    user('.agent/mcp_config.json', 'agent'), user('.agent/mcp.json', 'agent'),
    user('.gemini/config/mcp_config.json', 'antigravity'), user('.gemini/antigravity/mcp_config.json', 'antigravity'),
    user('.gemini/antigravity-cli/mcp_config.json', 'antigravity'),
    user('Library/Application Support/Claude/claude_desktop_config.json', 'claude-desktop'),
    user('.config/Claude/claude_desktop_config.json', 'claude-desktop'), user('.vscode/mcp.json', 'vscode'),
  ];
}

function importedName(host: string, scope: 'project' | 'user', name: string, used: Set<string>): string {
  const base = `${host}.${name}`;
  if (!used.has(base)) return base;
  const scoped = `${host}.${scope}.${name}`;
  if (!used.has(scoped)) return scoped;
  let suffix = 2;
  while (used.has(`${scoped}.${suffix}`)) suffix += 1;
  return `${scoped}.${suffix}`;
}

/** Discover common MCP configuration files and normalize foreign definitions without activating them. */
export function discoverMcpSystem(cwd: string, options?: string | DiscoverMcpConfigOptions): McpDiscoveryResult {
  const configs: DiscoveredMcpConfig[] = [];
  const definitions: DiscoveredMcpDefinition[] = [];
  const seenPaths = new Set<string>();
  const usedNames = new Set<string>();
  for (const candidate of candidates(path.resolve(cwd), options)) {
    if (seenPaths.has(candidate.path) || !fs.existsSync(candidate.path)) continue;
    seenPaths.add(candidate.path);
    try {
      const stat = fs.lstatSync(candidate.path);
      if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('MCP configuration must be a regular non-symbolic-link file');
      if (stat.size > MAX_CONFIG_BYTES) throw new Error(`MCP configuration exceeds ${MAX_CONFIG_BYTES} bytes`);
      const text = fs.readFileSync(candidate.path, 'utf8');
      const containers = candidate.format === 'toml'
        ? [parseToml(text)]
        : jsonContainers(JSON.parse(text) as JsonRecord, candidate, cwd);
      const summaries: Array<{ name: string; command?: string }> = [];
      for (const container of containers) {
        for (const [originalName, raw] of Object.entries(container)) {
          if (!/^[A-Za-z0-9_.-]{1,64}$/.test(originalName) || !isRecord(raw)) continue;
          const normalized = normalizeServer(raw);
          if (!normalized) continue;
          summaries.push({ name: originalName, ...(normalized.command ? { command: normalized.command } : {}) });
          if (!candidate.active) {
            const name = importedName(candidate.host, candidate.scope, originalName, usedNames);
            usedNames.add(name);
            definitions.push({
              name,
              config: {
                ...normalized,
                disabled: true,
                discovered: { host: candidate.host, scope: candidate.scope, path: candidate.path, originalName },
              },
            });
          }
        }
      }
      summaries.sort((a, b) => a.name.localeCompare(b.name));
      configs.push({ path: candidate.path, host: candidate.host, scope: candidate.scope, format: candidate.format, active: candidate.active, servers: summaries });
    } catch (error) {
      configs.push({ path: candidate.path, host: candidate.host, scope: candidate.scope, format: candidate.format, active: candidate.active, servers: [], error: (error as Error).message });
    }
  }
  return { configs, definitions };
}

export function discoverMcpConfigs(cwd: string, options?: string | DiscoverMcpConfigOptions): DiscoveredMcpConfig[] {
  return discoverMcpSystem(cwd, options).configs;
}
