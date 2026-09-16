import { chmod, lstat, mkdir, readFile, realpath } from 'node:fs/promises';
import path from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';
import { extensionHome } from '../../extension-paths.js';
import { atomicWriteUtf8 } from '../file-state.js';
import { renderMcpRoutingIndex as renderMcpCatalogIndex } from './catalog-pages.js';
import {
  MCP_CATALOG_SNAPSHOT_VERSION,
  isRecord,
  sha256,
  stableJson,
  stableSchemaDigest,
  type BuildMcpCatalogSnapshotOptions,
  type McpCatalogServerSnapshot,
  type McpCatalogSnapshotV1,
  type McpCatalogSourceIdentity,
  type McpCatalogToolSnapshot,
} from './catalog-model.js';
export { renderMcpCatalogIndex };
export { MCP_CATALOG_SNAPSHOT_VERSION, stableJson, stableSchemaDigest } from './catalog-model.js';
export type {
  BuildMcpCatalogSnapshotOptions,
  McpCatalogServerInput,
  McpCatalogServerSnapshot,
  McpCatalogSnapshotV1,
  McpCatalogSourceIdentity,
  McpCatalogToolInput,
  McpCatalogToolSnapshot,
} from './catalog-model.js';

const DEFAULT_SERVER_NAME = 'octocode';
const MAX_SNAPSHOT_CHARS = 16 * 1024 * 1024;
const MAX_SCHEMA_CHARS = 512 * 1024;
const MAX_SERVERS = 128;
const MAX_TOOLS_PER_SERVER = 5_000;
const MAX_NAME_CHARS = 256;
const MAX_INSTRUCTIONS_CHARS = 64_000;
const MAX_DESCRIPTION_CHARS = 32_000;
const PRIVATE_DIR_MODE = 0o700;
const PRIVATE_FILE_MODE = 0o600;
const KEY_PATTERN = /^[a-f0-9]{32}$/;

export interface McpCatalogMeasurement {
  eagerChars: number;
  indexChars: number;
  instructionDescriptionChars: number;
  schemaChars: number;
  reductionRatio: number;
}

/**
 * Strip JSON-Schema draft metadata ($schema) that MCP SDK Zod adapters inject
 * and hoist 'type' to the first key for readability and stable catalog ordering.
 * Applied only at render time — the stored snapshot retains the original schema.
 */
export function normalizeSchemaForCatalog(schema: unknown): unknown {
  if (!isRecord(schema)) return schema;
  const { $schema: _dropped, type, ...rest } = schema as Record<string, unknown>;
  return type !== undefined ? { type, ...rest } : rest;
}

function sortServers<T extends { name: string }>(servers: T[]): T[] {
  return [...servers].sort((left, right) => {
    if (left.name === DEFAULT_SERVER_NAME && right.name !== DEFAULT_SERVER_NAME) return -1;
    if (right.name === DEFAULT_SERVER_NAME && left.name !== DEFAULT_SERVER_NAME) return 1;
    return left.name.localeCompare(right.name);
  });
}

function configDigest(signatures: Record<string, string>): string {
  return sha256(stableJson(Object.entries(signatures).sort(([left], [right]) => left.localeCompare(right))));
}

export function workspaceKeyForCatalog(cwd: string, sources: McpCatalogSourceIdentity[]): string {
  const identity = {
    cwd: path.resolve(cwd),
    sources: [...sources]
      .map((source) => ({ scope: source.scope, path: path.resolve(source.path) }))
      .sort((left, right) => left.scope.localeCompare(right.scope) || left.path.localeCompare(right.path)),
  };
  return sha256(stableJson(identity)).slice(0, 32);
}

export function buildMcpCatalogSnapshot(options: BuildMcpCatalogSnapshotOptions): McpCatalogSnapshotV1 {
  const servers = sortServers(options.servers).map((server) => ({
    name: server.name,
    configSignature: options.configSignatures[server.name] ?? '',
    ...(server.instructions ? { instructions: server.instructions } : {}),
    tools: [...server.tools]
      .sort((left, right) => left.name.localeCompare(right.name))
      .map((tool) => ({
        name: tool.name,
        ...(tool.description ? { description: tool.description } : {}),
        inputSchema: tool.inputSchema,
        schemaDigest: stableSchemaDigest(tool.inputSchema),
      })),
  }));
  return {
    version: MCP_CATALOG_SNAPSHOT_VERSION,
    workspaceKey: workspaceKeyForCatalog(options.cwd, options.sources),
    capturedAt: options.capturedAt ?? new Date().toISOString(),
    configDigest: configDigest(options.configSignatures),
    servers,
  };
}

function validBoundedString(value: unknown, max: number, allowEmpty = false): value is string {
  return typeof value === 'string' && value.length <= max && (allowEmpty || value.length > 0);
}

function parseTool(value: unknown): McpCatalogToolSnapshot | undefined {
  if (!isRecord(value)) return undefined;
  if (!validBoundedString(value['name'], MAX_NAME_CHARS)) return undefined;
  if (value['description'] !== undefined && !validBoundedString(value['description'], MAX_DESCRIPTION_CHARS, true)) return undefined;
  if (!validBoundedString(value['schemaDigest'], 64) || !/^[a-f0-9]{64}$/.test(value['schemaDigest'])) return undefined;
  if (!Object.hasOwn(value, 'inputSchema')) return undefined;
  let schemaText: string;
  try {
    schemaText = stableJson(value['inputSchema']);
  } catch {
    return undefined;
  }
  if (schemaText.length > MAX_SCHEMA_CHARS || sha256(schemaText) !== value['schemaDigest']) return undefined;
  return {
    name: value['name'],
    ...(typeof value['description'] === 'string' ? { description: value['description'] } : {}),
    inputSchema: value['inputSchema'],
    schemaDigest: value['schemaDigest'],
  };
}

function parseServer(value: unknown): McpCatalogServerSnapshot | undefined {
  if (!isRecord(value)) return undefined;
  if (!validBoundedString(value['name'], MAX_NAME_CHARS)) return undefined;
  if (!validBoundedString(value['configSignature'], 4_096, true)) return undefined;
  if (value['instructions'] !== undefined && !validBoundedString(value['instructions'], MAX_INSTRUCTIONS_CHARS, true)) return undefined;
  if (!Array.isArray(value['tools']) || value['tools'].length > MAX_TOOLS_PER_SERVER) return undefined;
  const tools = value['tools'].map(parseTool);
  if (tools.some((tool) => tool === undefined)) return undefined;
  const parsedTools = tools as McpCatalogToolSnapshot[];
  if (new Set(parsedTools.map((tool) => tool.name)).size !== parsedTools.length) return undefined;
  return {
    name: value['name'],
    configSignature: value['configSignature'],
    ...(typeof value['instructions'] === 'string' ? { instructions: value['instructions'] } : {}),
    tools: [...parsedTools].sort((left, right) => left.name.localeCompare(right.name)),
  };
}

export function parseMcpCatalogSnapshot(
  text: string,
  expected?: { workspaceKey: string; configDigest: string },
): McpCatalogSnapshotV1 | undefined {
  if (text.length === 0 || text.length > MAX_SNAPSHOT_CHARS) return undefined;
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return undefined;
  }
  if (!isRecord(value) || value['version'] !== MCP_CATALOG_SNAPSHOT_VERSION) return undefined;
  if (!validBoundedString(value['workspaceKey'], 32) || !KEY_PATTERN.test(value['workspaceKey'])) return undefined;
  if (!validBoundedString(value['configDigest'], 64) || !/^[a-f0-9]{64}$/.test(value['configDigest'])) return undefined;
  if (!validBoundedString(value['capturedAt'], 64) || !Number.isFinite(Date.parse(value['capturedAt']))) return undefined;
  if (!Array.isArray(value['servers']) || value['servers'].length > MAX_SERVERS) return undefined;
  if (expected && (value['workspaceKey'] !== expected.workspaceKey || value['configDigest'] !== expected.configDigest)) return undefined;
  const servers = value['servers'].map(parseServer);
  if (servers.some((server) => server === undefined)) return undefined;
  const parsedServers = servers as McpCatalogServerSnapshot[];
  if (new Set(parsedServers.map((server) => server.name)).size !== parsedServers.length) return undefined;
  return {
    version: MCP_CATALOG_SNAPSHOT_VERSION,
    workspaceKey: value['workspaceKey'],
    capturedAt: value['capturedAt'],
    configDigest: value['configDigest'],
    servers: sortServers(parsedServers),
  };
}

export function sameMcpCatalogContent(left: McpCatalogSnapshotV1, right: McpCatalogSnapshotV1): boolean {
  return left.workspaceKey === right.workspaceKey
    && left.configDigest === right.configDigest
    && stableJson(left.servers) === stableJson(right.servers);
}

export function findMcpCatalogTool(
  snapshot: McpCatalogSnapshotV1,
  serverName: string,
  toolName: string,
): McpCatalogToolSnapshot | undefined {
  return snapshot.servers.find((server) => server.name === serverName)?.tools.find((tool) => tool.name === toolName);
}

export function measureMcpCatalog(snapshot: McpCatalogSnapshotV1): McpCatalogMeasurement {
  const eagerChars = stableJson(snapshot.servers).length;
  const indexChars = renderMcpCatalogIndex(snapshot).length;
  const instructionDescriptionChars = snapshot.servers.reduce((serverTotal, server) => (
    serverTotal + (server.instructions?.length ?? 0) + server.tools.reduce((toolTotal, tool) => toolTotal + (tool.description?.length ?? 0), 0)
  ), 0);
  const schemaChars = snapshot.servers.reduce((serverTotal, server) => (
    serverTotal + server.tools.reduce((toolTotal, tool) => toolTotal + JSON.stringify(tool.inputSchema).length, 0)
  ), 0);
  return {
    eagerChars,
    indexChars,
    instructionDescriptionChars,
    schemaChars,
    reductionRatio: eagerChars === 0 ? 0 : (eagerChars - indexChars) / eagerChars,
  };
}

export function snapshotPathForWorkspace(workspaceKey: string, home = getOctocodeHome()): string {
  if (!KEY_PATTERN.test(workspaceKey)) throw new Error('Invalid MCP catalog workspace key');
  return path.join(extensionHome(home), 'mcp', 'workspaces', workspaceKey, 'catalog.json');
}

function isPathInside(parent: string, candidate: string): boolean {
  const relative = path.relative(parent, candidate);
  return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
}

async function resolveSafeCatalogRoot(home: string, create: boolean): Promise<string | undefined> {
  const absoluteHome = extensionHome(home);
  if (create) await mkdir(absoluteHome, { recursive: true });
  let realHome: string;
  try {
    realHome = await realpath(absoluteHome);
  } catch {
    return undefined;
  }
  const root = path.join(absoluteHome, 'mcp', 'workspaces');
  try {
    const metadata = await lstat(root);
    if (metadata.isSymbolicLink() || !metadata.isDirectory()) return undefined;
  } catch {
    if (!create) return undefined;
    await mkdir(root, { recursive: true, mode: PRIVATE_DIR_MODE });
  }
  const realRoot = await realpath(root);
  if (!isPathInside(realHome, realRoot)) return undefined;
  if (create) await chmod(realRoot, PRIVATE_DIR_MODE);
  return realRoot;
}

export async function writeMcpCatalogSnapshot(
  snapshot: McpCatalogSnapshotV1,
  options: { home?: string } = {},
): Promise<string> {
  const parsed = parseMcpCatalogSnapshot(JSON.stringify(snapshot));
  if (!parsed) throw new Error('Refusing to write invalid MCP catalog snapshot');
  const home = options.home ?? getOctocodeHome();
  const root = await resolveSafeCatalogRoot(home, true);
  if (!root) throw new Error('MCP catalog root is a symlink or escapes Octocode home');
  const workspaceDir = path.join(root, snapshot.workspaceKey);
  await mkdir(workspaceDir, { recursive: false, mode: PRIVATE_DIR_MODE }).catch((error: NodeJS.ErrnoException) => {
    if (error.code !== 'EEXIST') throw error;
  });
  const filePath = path.join(workspaceDir, 'catalog.json');
  try {
    if ((await lstat(filePath)).isSymbolicLink()) throw new Error('MCP catalog snapshot path is a symlink');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
  await atomicWriteUtf8(filePath, `${JSON.stringify(snapshot)}\n`, PRIVATE_FILE_MODE);
  return filePath;
}

export async function readMcpCatalogSnapshot(options: {
  workspaceKey: string;
  configDigest: string;
  home?: string;
}): Promise<McpCatalogSnapshotV1 | undefined> {
  if (!KEY_PATTERN.test(options.workspaceKey)) return undefined;
  const home = options.home ?? getOctocodeHome();
  const root = await resolveSafeCatalogRoot(home, false);
  if (!root) return undefined;
  const filePath = path.join(root, options.workspaceKey, 'catalog.json');
  try {
    const metadata = await lstat(filePath);
    if (metadata.isSymbolicLink() || !metadata.isFile() || metadata.size > MAX_SNAPSHOT_CHARS) return undefined;
    const text = await readFile(filePath, 'utf8');
    return parseMcpCatalogSnapshot(text, {
      workspaceKey: options.workspaceKey,
      configDigest: options.configDigest,
    });
  } catch {
    return undefined;
  }
}
