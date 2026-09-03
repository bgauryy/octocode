import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { parse as parseToml } from 'smol-toml';
import {
  HookCatalog,
  parseCodexHooks,
  revision,
  type HookScope,
  type HookSourceDescriptor,
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
}

function stable(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stable);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(Object.entries(value as Record<string, unknown>)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([key, child]) => [key, stable(child)]));
}

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
      const normalized = JSON.stringify(stable(configuration));
      const descriptor: HookSourceDescriptor = {
        id: `${candidate.scope}:${candidate.sourcePath}`,
        scope: candidate.scope,
        provenance: candidate.sourcePath,
        managed: false,
        rawHash: digest(raw),
        normalizedHash: digest(normalized),
        trust: 'trusted',
        revision: revision(digest(raw)),
        discoveryOrder: discoveryOrder++,
      };
      catalog.register(descriptor, configuration);
      sources.push(descriptor);
    } catch (error) {
      errors.push({ path: candidate.sourcePath, message: error instanceof Error ? error.message : 'Hook discovery failed' });
    }
  }
  return { catalog, sources, errors };
}
