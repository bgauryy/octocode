import * as fs from 'fs/promises';
import * as path from 'path';
import { randomUUID } from 'crypto';
import { readJsonFile } from './jsonUtils';

export const MCP_COMMAND = 'npx';
export const MCP_ARGS = ['-y', 'octocode-mcp@latest'];
export type ConfigKey = 'mcpServers' | 'servers';
type JsonObject = Record<string, unknown>;

function object(value: unknown): value is JsonObject {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

async function readConfig(file: string, key: ConfigKey) {
  const config = await readJsonFile<unknown>(file);
  if (config === undefined) return {};
  if (!object(config) || (key in config && !object(config[key]))) {
    throw new Error(`Invalid MCP configuration at ${file}`);
  }
  const servers = config[key] as JsonObject | undefined;
  if (servers && 'octocode' in servers) {
    const server = servers.octocode;
    if (!object(server) || ('env' in server && !object(server.env))) {
      throw new Error(`Invalid Octocode server configuration at ${file}`);
    }
  }
  return config;
}

/**
 * OCTOCODE_TOKEN has token priority 0, so the signed-in token wins over an
 * inherited GH_TOKEN/GITHUB_TOKEN. GITHUB_TOKEN is the key earlier extension
 * versions wrote; drop it so a stale copy cannot linger.
 */
export const MCP_TOKEN_ENV = 'OCTOCODE_TOKEN';
const LEGACY_TOKEN_ENV = 'GITHUB_TOKEN';

function setToken(server: JsonObject, token: string | undefined) {
  const env = { ...(server.env as JsonObject | undefined) };
  delete env[LEGACY_TOKEN_ENV];
  if (token) env[MCP_TOKEN_ENV] = token;
  else delete env[MCP_TOKEN_ENV];
  if (Object.keys(env).length) server.env = env;
  else delete server.env;
}

async function writeConfig(file: string, config: JsonObject) {
  await fs.mkdir(path.dirname(file), { recursive: true });
  const temporary = `${file}.${randomUUID()}.tmp`;
  try {
    await fs.writeFile(temporary, JSON.stringify(config, null, 2), {
      encoding: 'utf8',
      mode: 0o600,
      flag: 'wx',
    });
    await fs.rename(temporary, file);
  } finally {
    await fs.rm(temporary, { force: true });
  }
}

const writes = new Map<string, Promise<unknown>>();

async function serialize<T>(
  file: string,
  action: () => Promise<T>
): Promise<T> {
  const key = path.resolve(file);
  const previous = writes.get(key) ?? Promise.resolve();
  const current = previous.catch(() => {}).then(action);
  writes.set(key, current);
  try {
    return await current;
  } finally {
    if (writes.get(key) === current) writes.delete(key);
  }
}

export async function configureMcpServer(
  file: string,
  token: string | undefined,
  key: ConfigKey = 'mcpServers'
): Promise<'installed' | 'unchanged'> {
  return serialize(file, async () => {
    const config = await readConfig(file, key);
    const servers = (config[key] ?? {}) as JsonObject;
    const existing = servers.octocode;
    const server: JsonObject = {
      ...(existing as JsonObject | undefined),
      command: MCP_COMMAND,
      type: 'stdio',
      args: [...MCP_ARGS],
    };
    setToken(server, token);
    if (JSON.stringify(existing) === JSON.stringify(server)) return 'unchanged';
    servers.octocode = server;
    config[key] = servers;
    await writeConfig(file, config);
    return 'installed';
  });
}

export async function updateMcpConfigToken(
  file: string,
  token: string | undefined,
  key: ConfigKey = 'mcpServers'
): Promise<void> {
  return serialize(file, async () => {
    const config = await readConfig(file, key);
    const servers = config[key] as JsonObject | undefined;
    if (!servers?.octocode) return;
    setToken(servers.octocode as JsonObject, token);
    await writeConfig(file, config);
  });
}
