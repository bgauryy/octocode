import fs from 'node:fs';
import path from 'node:path';
import { parse as parseToml } from 'smol-toml';
import { workspaceAgentRoot } from './paths.js';
import { capabilityDefinitionRevision, capabilitySourcePaths, stableCapabilitySourceId, type CapabilityPathOptions } from './capability-sources.js';

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
  instructions?: string;
  timeoutMs?: number;
  startupTimeoutMs?: number;
  enabledTools?: string[];
  disabledTools?: string[];
  sourceDisabled?: boolean;
  discovered: {
    host: string;
    scope: 'project' | 'user';
    path: string;
    originalName: string;
    sourceId?: string;
    revision?: string;
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
  diagnostics?: McpDiscoveryDiagnostic[];
}

export interface McpDiscoveryDiagnostic {
  code: 'unsupported-field' | 'invalid-field' | 'unsupported-transport' | 'unsupported-interpolation';
  field: string;
  message: string;
  server?: string;
}

export interface DiscoveredMcpDefinition {
  name: string;
  config: DiscoveredMcpServerConfig;
  sourceId: string;
  revision: string;
  diagnostics: McpDiscoveryDiagnostic[];
  /** Original vendor fields are kept for review, never executed directly. */
  rawDefinition: Record<string, unknown>;
  supported: boolean;
}

export interface McpDiscoveryResult {
  configs: DiscoveredMcpConfig[];
  definitions: DiscoveredMcpDefinition[];
}

export interface DiscoverMcpConfigOptions extends CapabilityPathOptions {
  /** Host-owned workspace storage; defaults to the Agent workspace root. */
  workspaceRoot?: (cwd: string, octocodeHome: string) => string;
  additionalNativeFiles?: Array<{ path: string; scope: 'project' | 'user' }>;
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

export function normalizeDiscoveredMcpServer(raw: JsonRecord): { config: Omit<DiscoveredMcpServerConfig, 'disabled' | 'discovered'>; diagnostics: McpDiscoveryDiagnostic[] } {
  const diagnostics: McpDiscoveryDiagnostic[] = [];
  const known = new Set(['transport', 'type', 'command', 'args', 'env', 'envRefs', 'env_vars', 'cwd', 'url', 'httpUrl', 'serverUrl', 'headers', 'http_headers', 'headerRefs', 'env_http_headers', 'bearer_token_env_var', 'bearerTokenEnvVar', 'auth', 'oauth', 'description', 'instructions', 'timeoutMs', 'timeout', 'tool_timeout_sec', 'startup_timeout_sec', 'startup_timeout_ms', 'startupTimeoutMs', 'enabled_tools', 'disabled_tools', 'enabledTools', 'disabledTools', 'disabled', 'enabled']);
  for (const field of Object.keys(raw)) {
    if (!known.has(field)) diagnostics.push({ code: 'unsupported-field', field, message: `Vendor field ${field} is retained for review but is not supported by this host` });
  }
  const transport = raw['transport'] ?? raw['type'];
  if (transport !== undefined && !['stdio', 'http', 'streamable-http'].includes(String(transport))) diagnostics.push({ code: 'unsupported-transport', field: 'transport', message: `Transport ${String(transport)} is not supported` });
  const rawUrl = typeof raw['url'] === 'string'
    ? raw['url']
    : typeof raw['httpUrl'] === 'string'
      ? raw['httpUrl']
      : typeof raw['serverUrl'] === 'string'
        ? raw['serverUrl']
        : undefined;
  const url = rawUrl?.trim();
  const command = typeof raw['command'] === 'string' ? raw['command'].trim() : undefined;
  if (!url && !command) diagnostics.push({ code: 'invalid-field', field: 'command', message: 'MCP server requires a command or HTTP URL' });
  if (url) {
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') diagnostics.push({ code: 'invalid-field', field: 'url', message: 'MCP URL requires HTTP or HTTPS' });
    } catch { diagnostics.push({ code: 'invalid-field', field: 'url', message: 'MCP URL is invalid' }); }
  }
  const timeout = raw['timeoutMs'] ?? raw['timeout'] ?? (typeof raw['tool_timeout_sec'] === 'number' ? raw['tool_timeout_sec'] * 1000 : undefined);
  const startupTimeout = raw['startupTimeoutMs'] ?? raw['startup_timeout_ms'] ?? (typeof raw['startup_timeout_sec'] === 'number' ? raw['startup_timeout_sec'] * 1000 : undefined);
  const env = splitReferences(raw['env']);
  const headers = splitReferences(raw['headers'] ?? raw['http_headers']);
  const explicitHeaderRefs = stringRecord(raw['headerRefs'] ?? raw['env_http_headers']);
  const envVars = stringArray(raw['env_vars']);
  const envRefs = {
    ...(env.refs ?? {}),
    ...(stringRecord(raw['envRefs']) ?? {}),
    ...Object.fromEntries((envVars ?? []).filter((key) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(key)).map((key) => [key, key])),
  };
  const headerRefs = { ...(headers.refs ?? {}), ...(explicitHeaderRefs ?? {}) };
  for (const [field, value] of Object.entries(raw)) {
    if (['command', 'cwd', 'url', 'httpUrl', 'serverUrl', 'description', 'instructions'].includes(field) && typeof value !== 'string') diagnostics.push({ code: 'invalid-field', field, message: `${field} must be a string` });
    if (['disabled', 'enabled'].includes(field) && typeof value !== 'boolean') diagnostics.push({ code: 'invalid-field', field, message: `${field} must be a boolean` });
    if (['tool_timeout_sec', 'startup_timeout_sec'].includes(field) && (typeof value !== 'number' || !Number.isFinite(value) || value <= 0)) diagnostics.push({ code: 'invalid-field', field, message: `${field} must be a positive number` });
    if (['args', 'env_vars', 'enabled_tools', 'disabled_tools', 'enabledTools', 'disabledTools'].includes(field) && !stringArray(value)) diagnostics.push({ code: 'invalid-field', field, message: `${field} must be an array of strings` });
    if (['env', 'headers', 'http_headers', 'headerRefs', 'envRefs', 'env_http_headers'].includes(field) && (!isRecord(value) || Object.values(value).some(item => typeof item !== 'string'))) diagnostics.push({ code: 'invalid-field', field, message: `${field} must map strings to strings` });
    if (['command', 'cwd', 'url', 'httpUrl', 'serverUrl', 'args'].includes(field) && JSON.stringify(value).includes('${')) diagnostics.push({ code: 'unsupported-interpolation', field, message: `Vendor interpolation in ${field} must be reviewed and resolved explicitly` });
    if (['env', 'headers', 'http_headers'].includes(field) && isRecord(value) && Object.values(value).some(item => typeof item === 'string' && item.includes('${') && !/^\$\{(?:env:)?[A-Za-z_][A-Za-z0-9_]*\}$/.test(item))) diagnostics.push({ code: 'unsupported-interpolation', field, message: `Embedded environment interpolation in ${field} is retained but requires explicit resolution` });
  }
  if (envVars?.some(value => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(value))) diagnostics.push({ code: 'invalid-field', field: 'env_vars', message: 'env_vars must contain valid environment names' });
  if (Object.values({ ...envRefs, ...headerRefs }).some(value => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(value))) diagnostics.push({ code: 'invalid-field', field: 'envRefs', message: 'Environment references must name environment variables' });
  if (isRecord(raw['oauth']) && Object.keys(raw['oauth']).length) diagnostics.push({ code: 'unsupported-field', field: 'oauth', message: 'Vendor-specific OAuth options require explicit configuration' });
  if (raw['auth'] !== undefined && raw['auth'] !== 'none' && raw['auth'] !== 'oauth') diagnostics.push({ code: 'invalid-field', field: 'auth', message: 'Unsupported authentication mode' });
  for (const [field, value] of [['timeoutMs', timeout], ['startupTimeoutMs', startupTimeout]] as const) {
    if (value !== undefined && (typeof value !== 'number' || !Number.isSafeInteger(value) || value <= 0)) diagnostics.push({ code: 'invalid-field', field, message: `${field} must be a positive integer` });
  }
  const bearer = raw['bearerTokenEnvVar'] ?? raw['bearer_token_env_var'];
  if (bearer !== undefined && (typeof bearer !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(bearer))) diagnostics.push({ code: 'invalid-field', field: 'bearerTokenEnvVar', message: 'Bearer token reference must name an environment variable' });
  return { config: {
    transport: url ? 'http' : 'stdio',
    ...(url ? { url } : { command }),
    ...(stringArray(raw['args']) ? { args: stringArray(raw['args']) } : {}),
    ...(env.values ? { env: env.values } : {}),
    ...(Object.keys(envRefs).length ? { envRefs } : {}),
    ...(typeof raw['cwd'] === 'string' ? { cwd: raw['cwd'] } : {}),
    ...(headers.values ? { headers: headers.values } : {}),
    ...(Object.keys(headerRefs).length ? { headerRefs } : {}),
    ...(typeof bearer === 'string' ? { bearerTokenEnvVar: bearer } : {}),
    ...((raw['auth'] === 'oauth' || isRecord(raw['oauth'])) ? { auth: 'oauth' as const } : {}),
    ...(raw['auth'] === 'none' ? { auth: 'none' as const } : {}),
    ...(typeof raw['description'] === 'string' ? { description: raw['description'] } : {}),
    ...(typeof raw['instructions'] === 'string' ? { instructions: raw['instructions'] } : {}),
    ...(typeof timeout === 'number' && Number.isFinite(timeout) && timeout > 0 ? { timeoutMs: timeout } : {}),
    ...(typeof startupTimeout === 'number' && Number.isFinite(startupTimeout) && startupTimeout > 0 ? { startupTimeoutMs: startupTimeout } : {}),
    ...(stringArray(raw['enabledTools'] ?? raw['enabled_tools']) ? { enabledTools: stringArray(raw['enabledTools'] ?? raw['enabled_tools']) } : {}),
    ...(stringArray(raw['disabledTools'] ?? raw['disabled_tools']) ? { disabledTools: stringArray(raw['disabledTools'] ?? raw['disabled_tools']) } : {}),
    ...(raw['disabled'] === true || raw['enabled'] === false ? { sourceDisabled: true } : {}),
  }, diagnostics };
}

function jsonContainers(json: JsonRecord, candidate: Candidate, cwd: string): Array<{ servers: JsonRecord; scope: Candidate['scope']; project?: string }> {
  const containers: Array<{ servers: JsonRecord; scope: Candidate['scope']; project?: string }> = [];
  if (isRecord(json['mcpServers'])) containers.push({ servers: json['mcpServers'], scope: candidate.scope });
  else if (isRecord(json['servers'])) containers.push({ servers: json['servers'], scope: candidate.scope });
  else if (candidate.allowRootServers) containers.push({ servers: json, scope: candidate.scope });
  if (candidate.claudeState && isRecord(json['projects'])) {
    const exact = json['projects'][path.resolve(cwd)];
    if (isRecord(exact) && isRecord(exact['mcpServers'])) containers.push({ servers: exact['mcpServers'], scope: 'project', project: path.resolve(cwd) });
  }
  return containers;
}

function candidates(cwd: string, options?: string | DiscoverMcpConfigOptions): Candidate[] {
  const paths = capabilitySourcePaths(cwd, typeof options === 'string' ? { homeDir: options } : options);
  const { homeDir } = paths;
  const octocodeHome = paths.native.globalRoot;
  const workspaceRoot = typeof options === 'object' ? options.workspaceRoot ?? workspaceAgentRoot : workspaceAgentRoot;
  const project = (relative: string, host: string, format: 'json' | 'toml' = 'json'): Candidate => ({ path: path.join(cwd, relative), host, scope: 'project', format, active: false });
  const user = (relative: string, host: string, format: 'json' | 'toml' = 'json'): Candidate => ({ path: path.join(homeDir, relative), host, scope: 'user', format, active: false });
  return [
    { path: paths.native.workspaceMcpFile, host: 'agents', scope: 'project', format: 'json', active: true, allowRootServers: true },
    { path: path.join(workspaceRoot(cwd, octocodeHome), 'mcp', 'servers.json'), host: 'octocode', scope: 'project', format: 'json', active: true, allowRootServers: true },
    project('.mcp.json', 'claude'), project('.claude/mcp.json', 'claude'),
    project('.cursor/mcp.json', 'cursor'), project('.codex/config.toml', 'codex', 'toml'),
    project('.gemini/settings.json', 'gemini'),
    project('.agents/mcp_config.json', 'antigravity'),
    project('.agent/mcp_config.json', 'agent'), project('.agent/mcp.json', 'agent'),
    project('.vscode/mcp.json', 'vscode'),
    { path: paths.native.mcpFile, host: 'octocode', scope: 'user', format: 'json', active: true, allowRootServers: true },
    { path: path.join(octocodeHome, 'agent', 'mcp', 'servers.json'), host: 'octocode', scope: 'user', format: 'json', active: true, allowRootServers: true },
    { ...user('.claude.json', 'claude'), claudeState: true }, user('.claude/mcp.json', 'claude'),
    user('.cursor/mcp.json', 'cursor'), { path: path.join(paths.codexHome, 'config.toml'), host: 'codex', scope: 'user', format: 'toml', active: false },
    user('.codex/config.toml', 'codex', 'toml'),
    user('.gemini/settings.json', 'gemini'), user('.copilot/mcp-config.json', 'copilot'),
    user('.agents/mcp_config.json', 'antigravity'), user('.agents/mcp.json', 'agents'),
    user('.agent/mcp_config.json', 'agent'), user('.agent/mcp.json', 'agent'),
    user('.gemini/config/mcp_config.json', 'antigravity'), user('.gemini/antigravity/mcp_config.json', 'antigravity'),
    user('.gemini/antigravity-cli/mcp_config.json', 'antigravity'),
    user('Library/Application Support/Claude/claude_desktop_config.json', 'claude-desktop'),
    user('.config/Claude/claude_desktop_config.json', 'claude-desktop'), user('.vscode/mcp.json', 'vscode'),
    ...(typeof options === 'object' ? options.additionalNativeFiles ?? [] : []).map(item => ({ ...item, host: 'octocode', format: 'json' as const, active: true, allowRootServers: true })),
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

/** Shared admission for both discovery and active host configuration reads. */
export function readMcpConfigText(filePath: string): string {
  const realPath = fs.realpathSync(filePath);
  const stat = fs.lstatSync(filePath);
  if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('MCP configuration must be a regular non-symbolic-link file');
  if (stat.size > MAX_CONFIG_BYTES) throw new Error(`MCP configuration exceeds ${MAX_CONFIG_BYTES} bytes`);
  const text = fs.readFileSync(filePath, 'utf8');
  const after = fs.lstatSync(filePath);
  if (fs.realpathSync(filePath) !== realPath || stat.dev !== after.dev || stat.ino !== after.ino || stat.size !== after.size || stat.mtimeMs !== after.mtimeMs) throw new Error('MCP configuration changed during discovery');
  return text;
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
      const text = readMcpConfigText(candidate.path);
      const toml = candidate.format === 'toml' ? parseToml(text) : undefined;
      const containers = candidate.format === 'toml'
        ? [{ servers: isRecord(toml?.['mcp_servers']) ? toml['mcp_servers'] : {}, scope: candidate.scope }]
        : jsonContainers(JSON.parse(text) as JsonRecord, candidate, cwd);
      const summaries: Array<{ name: string; command?: string }> = [];
      const diagnostics: McpDiscoveryDiagnostic[] = [];
      for (const container of containers) {
        for (const [originalName, raw] of Object.entries(container.servers)) {
          if (!/^[A-Za-z0-9_.-]{1,64}$/.test(originalName) || !isRecord(raw)) {
            diagnostics.push({ code: 'invalid-field', field: originalName, server: originalName, message: 'MCP server requires a supported name and object definition' });
            continue;
          }
          const normalized = normalizeDiscoveredMcpServer(raw);
          diagnostics.push(...normalized.diagnostics.map(item => ({ ...item, server: originalName })));
          summaries.push({ name: originalName, ...(normalized.config.command ? { command: normalized.config.command } : {}) });
          if (!candidate.active) {
            const name = importedName(candidate.host, container.scope, originalName, usedNames);
            usedNames.add(name);
            const sourceId = stableCapabilitySourceId({ kind: 'mcp', host: candidate.host, scope: container.scope, path: candidate.path, name: `${'project' in container ? container.project ?? '' : ''}:${originalName}` });
            const revision = capabilityDefinitionRevision({ definition: raw, realPath: fs.realpathSync(candidate.path) });
            definitions.push({
              name, sourceId, revision, diagnostics: normalized.diagnostics, rawDefinition: raw,
              supported: normalized.diagnostics.length === 0,
              config: {
                ...normalized.config,
                disabled: true,
                discovered: { host: candidate.host, scope: container.scope, path: candidate.path, originalName, sourceId, revision },
              },
            });
          }
        }
      }
      summaries.sort((a, b) => a.name.localeCompare(b.name));
      configs.push({ path: candidate.path, host: candidate.host, scope: candidate.scope, format: candidate.format, active: candidate.active, servers: summaries, ...(diagnostics.length ? { diagnostics } : {}) });
    } catch (error) {
      configs.push({ path: candidate.path, host: candidate.host, scope: candidate.scope, format: candidate.format, active: candidate.active, servers: [], error: (error as Error).message });
    }
  }
  return { configs, definitions };
}

export function discoverMcpConfigs(cwd: string, options?: string | DiscoverMcpConfigOptions): DiscoveredMcpConfig[] {
  return discoverMcpSystem(cwd, options).configs;
}
