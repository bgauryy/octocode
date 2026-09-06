import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { DEFAULT_AWARENESS_STORAGE_SCOPE, type AwarenessStorageScope } from './storage-scope.js';

export type AwarenessHookProfile = 'guard' | 'coordination' | 'full';

export interface WorkspaceAwarenessPolicy {
  version: 1;
  storage: {
    repository: AwarenessStorageScope;
    memory: AwarenessStorageScope;
  };
  hooks: {
    profile: AwarenessHookProfile;
  };
}

export const DEFAULT_WORKSPACE_POLICY: WorkspaceAwarenessPolicy = Object.freeze({
  version: 1,
  storage: Object.freeze({ repository: DEFAULT_AWARENESS_STORAGE_SCOPE, memory: DEFAULT_AWARENESS_STORAGE_SCOPE }),
  hooks: Object.freeze({ profile: 'coordination' }),
});

const MEMORY_COMMANDS = new Set([
  'tell-memory', 'get-memory', 'memory-lifecycle', 'forget', 'mine-weakness',
  'digest', 'export-harness',
]);

const PROFILE_COMMANDS: Record<AwarenessHookProfile, ReadonlySet<string>> = {
  guard: new Set(['pre-edit', 'post-edit', 'stop-verify']),
  coordination: new Set(['pre-edit', 'post-edit', 'stop-verify']),
  full: new Set(['pre-edit', 'post-edit', 'stop-verify', 'notify-deliver', 'session-compact', 'session-end']),
};

function record(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

function exactKeys(value: Record<string, unknown>, keys: string[], label: string): void {
  const actual = Object.keys(value);
  const unknown = actual.filter((key) => !keys.includes(key));
  const missing = keys.filter((key) => !Object.prototype.hasOwnProperty.call(value, key));
  if (unknown.length || missing.length) {
    throw new Error(`${label} keys invalid${unknown.length ? `; unknown: ${unknown.join(', ')}` : ''}${missing.length ? `; missing: ${missing.join(', ')}` : ''}`);
  }
}

function scope(value: unknown, label: string): AwarenessStorageScope {
  if (value !== 'repo' && value !== 'global') throw new Error(`${label} must be repo or global`);
  return value;
}

function profile(value: unknown): AwarenessHookProfile {
  if (value !== 'guard' && value !== 'coordination' && value !== 'full') {
    throw new Error('hooks.profile must be guard, coordination, or full');
  }
  return value;
}

export function parseWorkspacePolicy(value: unknown): WorkspaceAwarenessPolicy {
  const root = record(value, 'workspace policy');
  exactKeys(root, ['version', 'storage', 'hooks'], 'workspace policy');
  if (root.version !== 1) throw new Error(`unsupported workspace policy version: ${String(root.version)}`);
  const storage = record(root.storage, 'storage');
  exactKeys(storage, ['repository', 'memory'], 'storage');
  const hooks = record(root.hooks, 'hooks');
  exactKeys(hooks, ['profile'], 'hooks');
  return {
    version: 1,
    storage: {
      repository: scope(storage.repository, 'storage.repository'),
      memory: scope(storage.memory, 'storage.memory'),
    },
    hooks: { profile: profile(hooks.profile) },
  };
}

export function workspacePolicyPath(workspace: string): string {
  return join(resolve(workspace), '.octocode', 'awareness.json');
}

export function loadWorkspacePolicy(workspace: string): {
  path: string;
  exists: boolean;
  policy: WorkspaceAwarenessPolicy;
} {
  const path = workspacePolicyPath(workspace);
  if (!existsSync(path)) return { path, exists: false, policy: DEFAULT_WORKSPACE_POLICY };
  try {
    return { path, exists: true, policy: parseWorkspacePolicy(JSON.parse(readFileSync(path, 'utf8'))) };
  } catch (error) {
    throw new Error(`cannot load workspace Awareness policy at ${path}: ${(error as Error).message}`);
  }
}

export function writeWorkspacePolicy(workspace: string, value: WorkspaceAwarenessPolicy): string {
  const path = workspacePolicyPath(workspace);
  const policy = parseWorkspacePolicy(value);
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const temporary = `${path}.tmp-${process.pid}-${randomUUID()}`;
  try {
    writeFileSync(temporary, `${JSON.stringify(policy, null, 2)}\n`, { encoding: 'utf8', mode: 0o600 });
    renameSync(temporary, path);
  } finally {
    try { unlinkSync(temporary); } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
  return path;
}

export function storageScopeForCommand(
  command: string,
  workspace: string,
  explicit?: AwarenessStorageScope,
): AwarenessStorageScope {
  if (explicit) return explicit;
  const policy = loadWorkspacePolicy(workspace).policy;
  return MEMORY_COMMANDS.has(command) ? policy.storage.memory : policy.storage.repository;
}

export function hookCommandEnabled(profileName: AwarenessHookProfile, command: string): boolean {
  return PROFILE_COMMANDS[profileName].has(command);
}
