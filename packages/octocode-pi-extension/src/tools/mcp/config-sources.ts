import os from 'node:os';
import path from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';
import { repositoryDirectories } from '../../contracts/agent-skills.js';
import { capabilitySourcePaths } from '../../contracts/capability-sources.js';
import { getCapabilitySourceStatus, listCapabilitySourceReviews, reviewCapabilitySource, type CapabilitySourceStatus } from '../../contracts/capability-state.js';
import type { ReadableSqlite } from '@octocodeai/octocode-awareness/host';
import { extensionWorkspaceRoot, extensionHome } from '../../extension-paths.js';
import { openOctocodeDb } from '../storage-policy.js';
import { discoverMcpSystem } from './discovery.js';
import type { McpConfigPathOptions, McpConfigSource, McpLoadedConfig, McpScope, McpServerConfig } from './config.js';

export function projectMcpPath(cwd: string, octocodeHome = getOctocodeHome()): string {
  return capabilitySourcePaths(cwd, { octocodeHome }).native.workspaceMcpFile;
}
export function globalMcpPath(homeDir = os.homedir(), octocodeHome?: string): string {
  return capabilitySourcePaths(process.cwd(), { homeDir, octocodeHome }).native.mcpFile;
}

/** Low-to-high precedence; public native files win over retained private locations. */
export function globalMcpConfigPaths(options: McpConfigPathOptions = {}): string[] {
  const paths = capabilitySourcePaths(process.cwd(), options);
  return [...new Set([
    path.join(paths.native.globalRoot, 'agent', 'mcp', 'servers.json'),
    path.join(extensionHome(paths.native.globalRoot), 'mcp', 'servers.json'),
    paths.native.mcpFile,
  ])];
}

/** Repository ancestors apply before the nearest workspace definition. */
export function projectMcpConfigPaths(cwd: string, octocodeHome = getOctocodeHome()): string[] {
  return repositoryDirectories(cwd).flatMap(directory => [
    path.join(extensionWorkspaceRoot(directory, octocodeHome), 'mcp', 'servers.json'),
    projectMcpPath(directory, octocodeHome),
  ]);
}

/** Read-only source inventory; the caller applies runtime enablement after native precedence. */
export function discoverMcpConfigSources(cwd: string, trusted: boolean, options: McpConfigPathOptions, db?: ReadableSqlite): Omit<McpLoadedConfig, 'configuredServers'> {
  const discovered = discoverMcpSystem(cwd, options);
  const scopeKey = path.resolve(cwd);
  const servers = new Map<string, McpServerConfig>();
  const serverSources = new Map<string, McpConfigSource>();
  const sources: McpConfigSource[] = [];
  const warnings: string[] = [];
  const sourceByPath = new Map<string, McpConfigSource>();
  for (const config of discovered.configs.filter(item => !item.active)) {
    const allowed = config.scope === 'user' || trusted;
    const source: McpConfigSource = { scope: config.scope === 'project' ? 'discovered-project' : 'discovered-user', path: config.path, trusted: allowed, host: config.host, readOnly: true };
    sources.push(source);
    sourceByPath.set(config.path, source);
    if (config.error) warnings.push(`${config.path}: ${config.error}`);
    if (!allowed) warnings.push(`${config.path}: discovered but disabled because the project is not trusted`);
  }
  for (const definition of discovered.definitions) {
    const metadata = definition.config.discovered;
    const source = sourceByPath.get(metadata.path);
    if (!source) continue;
    const reviewStatus: CapabilitySourceStatus = metadata.scope === 'project' && !trusted ? 'untrusted'
      : db ? getCapabilitySourceStatus(db, scopeKey, { kind: 'mcp', sourceId: definition.sourceId, revision: definition.revision, name: definition.name, path: metadata.path }) : 'disabled';
    servers.set(definition.name, { ...definition.config, discovered: { ...metadata, sourceId: definition.sourceId, revision: definition.revision, reviewStatus, supported: definition.supported, diagnostics: definition.diagnostics } });
    serverSources.set(definition.name, { ...source, scope: metadata.scope === 'project' ? 'discovered-project' : 'discovered-user', trusted: metadata.scope === 'user' || trusted });
    for (const diagnostic of definition.diagnostics) warnings.push(`${metadata.path} (${metadata.originalName}): ${diagnostic.message}`);
  }
  if (db) {
    const knownSources = new Set(discovered.definitions.map(definition => definition.sourceId));
    for (const review of listCapabilitySourceReviews(db, scopeKey)) {
      if (review.kind !== 'mcp' || knownSources.has(review.sourceId)) continue;
      const name = servers.has(review.name) ? `${review.name}.unavailable.${review.sourceId.slice(-8)}` : review.name;
      servers.set(name, { disabled: true, discovered: { host: 'linked', scope: review.scopeKey === '*' ? 'user' : 'project', path: review.path, originalName: review.name, sourceId: review.sourceId, revision: review.revision, reviewStatus: 'unavailable', supported: false } });
      serverSources.set(name, { scope: review.scopeKey === '*' ? 'discovered-user' : 'discovered-project', path: review.path, trusted, readOnly: true });
    }
  }
  return { servers, serverSources, sources, warnings };
}

/** Approve the current linked source without copying or rewriting the vendor file. */
export function reviewMcpSource(cwd: string, sourceId: string, revision: string, scope: McpScope, options: McpConfigPathOptions & { trusted?: boolean } = {}): void {
  const definition = discoverMcpSystem(cwd, options).definitions.find(item => item.sourceId === sourceId);
  if (!definition) throw new Error('MCP source is unavailable');
  if (definition.revision !== revision) throw new Error('MCP source changed; review its current definition');
  if (!definition.supported) throw new Error(`MCP source has unsupported configuration: ${definition.diagnostics.map(item => item.message).join('; ')}`);
  if (definition.config.discovered.scope === 'project' && options.trusted === false) throw new Error('Trust the workspace before reviewing its MCP sources');
  reviewCapabilitySource(options.db ?? openOctocodeDb(), scope === 'global' ? '*' : path.resolve(cwd), { kind: 'mcp', sourceId, revision, name: definition.name, path: definition.config.discovered.path });
}
