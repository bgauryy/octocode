import path from 'node:path';
import type { AwarenessClientContext } from '@octocodeai/octocode-awareness';
import { defaultDbPath, type AwarenessStorageScope } from '@octocodeai/octocode-awareness/host';
import type { PiContext } from '../types.js';
import { getAwarenessAgentIdentity } from './awareness-shared.js';
import { resolveAwarenessCoordinationScope } from '../assets.js';

/** One database binding for native tools, guards, delivery and inherited workers. */
export function resolveAwarenessDatabase(workspace: string, scope: AwarenessStorageScope = resolveAwarenessCoordinationScope(workspace)): string {
  const inherited = process.env.OCTOCODE_AWARENESS_DB?.trim();
  return inherited ? path.resolve(workspace, inherited) : defaultDbPath(workspace, scope);
}

/** Native bindings use the explicit inherited DB when a worker runs in a worktree. */
export function buildAwarenessContext(ctx?: PiContext): AwarenessClientContext {
  const workspace = path.resolve(ctx?.cwd ?? process.cwd());
  const scope = resolveAwarenessCoordinationScope(workspace);
  const database = resolveAwarenessDatabase(workspace, scope);
  return { workspace, scope, database, agentId: getAwarenessAgentIdentity(ctx).agentId };
}
