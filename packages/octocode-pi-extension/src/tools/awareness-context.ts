import path from 'node:path';
import type { AwarenessClientContext } from '@octocodeai/octocode-awareness';
import {
  defaultDbPath,
  storageScopeForOperation,
  type AwarenessStorageScope,
} from '@octocodeai/octocode-awareness/host';
import type { PiContext } from '../types.js';
import { getAwarenessAgentIdentity } from './awareness-shared.js';

export interface AwarenessStorageBindings {
  workspace: string;
  scope: AwarenessStorageScope;
  database: string;
}

export interface AwarenessHostBindings {
  client: AwarenessClientContext;
  identity: ReturnType<typeof getAwarenessAgentIdentity>;
}

export function resolveAwarenessCoordinationScope(workspace: string): AwarenessStorageScope {
  return storageScopeForOperation('context.orient', workspace);
}

/** One database binding for native tools, guards, delivery and inherited workers. */
export function resolveAwarenessDatabase(workspace: string, scope: AwarenessStorageScope = resolveAwarenessCoordinationScope(workspace)): string {
  const inherited = process.env.OCTOCODE_AWARENESS_DB?.trim();
  return inherited ? path.resolve(workspace, inherited) : defaultDbPath(workspace, scope);
}

export function resolveAwarenessStorageBindings(workspace: string): AwarenessStorageBindings {
  const resolvedWorkspace = path.resolve(workspace);
  const scope = resolveAwarenessCoordinationScope(resolvedWorkspace);
  return {
    workspace: resolvedWorkspace,
    scope,
    database: resolveAwarenessDatabase(resolvedWorkspace, scope),
  };
}

/** Resolve identity and storage together so every Pi surface observes one binding snapshot. */
export function resolveAwarenessHostBindings(ctx?: PiContext): AwarenessHostBindings {
  const storage = resolveAwarenessStorageBindings(ctx?.cwd ?? process.cwd());
  const identity = getAwarenessAgentIdentity(ctx);
  return { client: { ...storage, agentId: identity.agentId }, identity };
}

/** Native bindings use the explicit inherited DB when a worker runs in a worktree. */
export function buildAwarenessContext(ctx?: PiContext): AwarenessClientContext {
  return resolveAwarenessHostBindings(ctx).client;
}
