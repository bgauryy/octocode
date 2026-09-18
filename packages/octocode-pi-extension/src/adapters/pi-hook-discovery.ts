import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { parse as parseToml } from 'smol-toml';
import { capabilitySourcePaths, capabilityDefinitionRevision, stableCapabilitySourceId } from '../contracts/capability-sources.js';
import {
  HookCatalog,
  parseCodexHooks,
  revision,
  type HookScope,
  type HookSourceDescriptor,
  type CodexHookConfiguration,
} from '@octocodeai/agent-core';

const MAX_HOOK_SOURCE_BYTES = 1024 * 1024;

export interface CodexHookDiscoveryOptions {
  readonly workspace: string;
  readonly userCodexDir?: string;
  readonly catalog?: HookCatalog;
}

export interface HookDiscoveryError {
  readonly path: string;
  readonly message: string;
}

export interface CodexHookDiscoveryResult {
  readonly catalog: HookCatalog;
  readonly sources: readonly HookSourceDescriptor[];
  readonly errors: readonly HookDiscoveryError[];
  readonly definitions: readonly PiHookDefinition[];
}

export interface PiHookDefinition {
  readonly name: string;
  readonly source: HookSourceDescriptor;
  readonly configuration: CodexHookConfiguration;
  readonly status: 'discovered' | 'shadowed';
}

export interface PiHookDiscoveryResult extends CodexHookDiscoveryResult {
  readonly definitions: readonly PiHookDefinition[];
}

export interface PiHookDiscoveryOptions extends Omit<CodexHookDiscoveryOptions, 'catalog'> {
  readonly octocodeHome?: string;
}

/** Native lifecycle aliases share the validated event-to-command contract. */
export const PI_HOOK_EVENT_ALIASES: Readonly<Record<string, string>> = {
  tool_call: 'PreToolUse',
  tool_execution_end: 'PostToolUse',
  session_before_compact: 'PreCompact',
  session_compact: 'PostCompact',
  input: 'UserPromptSubmit',
  agent_settled: 'Stop',
  session_start: 'SessionStart',
  session_shutdown: 'SessionEnd',
};

const digest = (value: string): string => createHash('sha256').update(value).digest('hex');

function readSource(sourcePath: string, workspaceRoot: string | undefined): string {
  const stat = fs.lstatSync(sourcePath);
  if (stat.isSymbolicLink()) throw new Error('Hook source symlinks are not allowed');
  if (!stat.isFile()) throw new Error('Hook source is not a regular file');
  if (stat.size > MAX_HOOK_SOURCE_BYTES) throw new Error('Hook source exceeds the 1 MiB size limit');
  if (workspaceRoot) {
    const realWorkspace = fs.realpathSync(workspaceRoot);
    const realSource = fs.realpathSync(sourcePath);
    const relative = path.relative(realWorkspace, realSource);
    if (relative.startsWith('..') || path.isAbsolute(relative)) throw new Error('Workspace hook source escapes the trusted workspace');
  }
  return fs.readFileSync(sourcePath, 'utf8');
}

function decodedSource(sourcePath: string, raw: string): unknown {
  if (sourcePath.endsWith('.json')) return JSON.parse(raw) as unknown;
  const decoded = parseToml(raw) as Record<string, unknown>;
  return decoded['hooks'] && typeof decoded['hooks'] === 'object' ? { hooks: decoded['hooks'] } : { hooks: {} };
}

export function discoverCodexHookSources(options: CodexHookDiscoveryOptions): CodexHookDiscoveryResult {
  const workspace = path.resolve(options.workspace);
  const userCodexDir = path.resolve(options.userCodexDir ?? process.env['CODEX_HOME'] ?? path.join(os.homedir(), '.codex'));
  const catalog = options.catalog ?? new HookCatalog();
  const sources: HookSourceDescriptor[] = [];
  const definitions: PiHookDefinition[] = [];
  const errors: HookDiscoveryError[] = [];
  const candidates: Array<{ sourcePath: string; scope: HookScope; workspaceRoot?: string }> = [
    { sourcePath: path.join(userCodexDir, 'hooks.json'), scope: 'user' },
    { sourcePath: path.join(userCodexDir, 'config.toml'), scope: 'user' },
    { sourcePath: path.join(workspace, '.codex', 'hooks.json'), scope: 'workspace', workspaceRoot: workspace },
    { sourcePath: path.join(workspace, '.codex', 'config.toml'), scope: 'workspace', workspaceRoot: workspace },
  ];
  let discoveryOrder = 0;
  for (const candidate of candidates) {
    if (!fs.existsSync(candidate.sourcePath)) continue;
    try {
      const raw = readSource(candidate.sourcePath, candidate.workspaceRoot);
      const configuration = parseCodexHooks(decodedSource(candidate.sourcePath, raw));
      const normalizedHash = capabilityDefinitionRevision(configuration);
      const descriptor: HookSourceDescriptor = {
        id: stableCapabilitySourceId({ kind: 'hook', host: 'codex', scope: candidate.scope, path: candidate.sourcePath }),
        scope: candidate.scope,
        provenance: candidate.sourcePath,
        managed: false,
        rawHash: digest(raw),
        normalizedHash,
        trust: 'trusted',
        revision: revision(digest(raw)),
        discoveryOrder: discoveryOrder++,
      };
      catalog.register(descriptor, configuration);
      sources.push(descriptor);
      definitions.push({ name: path.basename(candidate.sourcePath), source: descriptor, configuration, status: 'discovered' });
    } catch (error) {
      errors.push({ path: candidate.sourcePath, message: error instanceof Error ? error.message : 'Hook discovery failed' });
    }
  }
  return { catalog, sources, errors, definitions };
}

function nativeConfiguration(value: unknown): CodexHookConfiguration {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return parseCodexHooks(value);
  const root = value as Record<string, unknown>;
  const events = root['hooks'] && typeof root['hooks'] === 'object' && !Array.isArray(root['hooks'])
    ? root['hooks'] as Record<string, unknown> : root;
  const mapped: Record<string, unknown> = {};
  for (const [name, groups] of Object.entries(events)) {
    const event = PI_HOOK_EVENT_ALIASES[name] ?? name;
    const normalized = Array.isArray(groups) ? groups.map(group => {
      if (group && typeof group === 'object' && !Array.isArray(group) && 'command' in group && !('hooks' in group)) {
        return { hooks: [group] };
      }
      return group;
    }) : groups;
    mapped[event] = Array.isArray(mapped[event]) && Array.isArray(normalized)
      ? [...mapped[event] as unknown[], ...normalized] : normalized;
  }
  return parseCodexHooks({ hooks: mapped });
}

/** Discovery reads declarative data only. Pi's native extension loader remains its owner. */
export function discoverPiHookSources(options: PiHookDiscoveryOptions): PiHookDiscoveryResult {
  const workspace = path.resolve(options.workspace);
  const paths = capabilitySourcePaths(workspace, { octocodeHome: options.octocodeHome });
  const codex = discoverCodexHookSources(options);
  const catalog = new HookCatalog();
  const definitions: PiHookDefinition[] = [];
  const errors = [...codex.errors];
  const candidates: Array<{ sourcePath: string; name: string; scope: HookScope; workspaceRoot?: string }> = [];
  for (const [directory, scope] of [[paths.native.hooksDir, 'user'], [paths.native.workspaceHooksDir, 'workspace']] as const) {
    try {
      if (!fs.existsSync(directory)) continue;
      const stat = fs.lstatSync(directory);
      if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error('Hook directory must be a regular directory');
      for (const name of fs.readdirSync(directory).filter(name => name.endsWith('.json')).sort()) {
        candidates.push({ sourcePath: path.join(directory, name), name: path.basename(name, '.json'), scope, ...(scope === 'workspace' ? { workspaceRoot: workspace } : {}) });
      }
    } catch (error) {
      errors.push({ path: directory, message: error instanceof Error ? error.message : 'Hook directory is unavailable' });
    }
  }
  const workspaceNames = new Set(candidates.filter(candidate => candidate.scope === 'workspace').map(candidate => candidate.name));
  for (const candidate of candidates) {
    try {
      const raw = readSource(candidate.sourcePath, candidate.workspaceRoot);
      const configuration = nativeConfiguration(JSON.parse(raw) as unknown);
      const normalizedHash = capabilityDefinitionRevision(configuration);
      const source: HookSourceDescriptor = {
        id: stableCapabilitySourceId({ kind: 'hook', host: 'octocode', scope: candidate.scope, path: candidate.sourcePath, name: candidate.name }),
        scope: candidate.scope,
        provenance: candidate.sourcePath,
        managed: false,
        rawHash: digest(raw), normalizedHash, trust: 'trusted', revision: revision(normalizedHash),
        discoveryOrder: definitions.length,
      };
      const status = candidate.scope === 'user' && workspaceNames.has(candidate.name) ? 'shadowed' : 'discovered';
      definitions.push({ name: candidate.name, source, configuration, status });
      catalog.register(source, configuration, status !== 'shadowed');
    } catch (error) {
      errors.push({ path: candidate.sourcePath, message: error instanceof SyntaxError ? 'Invalid hook JSON' : error instanceof Error ? error.message : 'Hook discovery failed' });
    }
  }
  // Existing Codex data stays discoverable. Review and execution use the same catalog.
  for (const definition of codex.definitions) {
    const descriptor = { ...definition.source, discoveryOrder: definitions.length };
    definitions.push({ ...definition, source: descriptor });
    catalog.register(descriptor, definition.configuration);
  }
  return { catalog, definitions, sources: definitions.map(definition => definition.source), errors };
}
