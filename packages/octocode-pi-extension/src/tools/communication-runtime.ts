import { createRequire } from 'node:module';
import path from 'node:path';
import { isPersistentStorageEnabledForExtension } from '@octocodeai/config';
import { getAssetPaths } from '../assets.js';
import type { PiContext, PiInstance } from '../types.js';
import { extractBashWriteTargets } from './bash-tool.js';

interface Binding { session: string; workspace: string; database?: string }
interface Controller {
  getBinding(): Binding | undefined;
  isBoundContext(ctx: PiContext): boolean;
  call(command: string, input?: Record<string, unknown>): Promise<{ ok?: boolean; conflicts?: unknown[] }>;
}
interface InboxModule {
  registerPiInbox(pi: PiInstance, options: {
    binary: string;
    enabled(ctx: PiContext): boolean;
    onBinding(binding: Binding | null): void;
  }): Controller;
}

const mutationTools = new Set([
  'file', 'write', 'edit', 'multi_edit', 'multiedit', 'notebookedit',
  'notebook_edit', 'strreplace', 'delete', 'apply_patch', 'applypatch',
]);

/** Best-effort shell targets plus structured writes; reads never require a lease. */
export function communicationMutationTargets(
  event: { toolName?: string; input?: Record<string, unknown> }, cwd: string,
): string[] {
  const tool = (event.toolName ?? '').toLowerCase();
  const input = event.input ?? {};
  const queries = Array.isArray(input['queries']) ? input['queries'] : [input];
  const paths = new Set<string>();
  const add = (value: unknown): void => {
    for (const item of Array.isArray(value) ? value : [value]) {
      if (typeof item === 'string' && item.trim()) paths.add(path.resolve(cwd, item));
    }
  };
  for (const query of queries) {
    if (!query || typeof query !== 'object') continue;
    const row = query as Record<string, unknown>;
    if (tool === 'bash' && typeof row['command'] === 'string') {
      add(extractBashWriteTargets(row['command'], cwd));
    } else if (mutationTools.has(tool)) {
      for (const key of ['path', 'filePath', 'file_path', 'paths', 'filePaths', 'file_paths']) add(row[key]);
      const patch = typeof row['patch'] === 'string' ? row['patch'] : row['command'];
      if (typeof patch === 'string') {
        for (const line of patch.split('\n')) {
          const match = /^\*\*\* (?:Add|Update|Delete) File: (.+)$|^\*\*\* Move to: (.+)$/.exec(line);
          if (match) add(match[1] ?? match[2]);
        }
      }
    }
  }
  return [...paths];
}

/** Shared Rust catalog, durable inbox and presence; Pi retains its own local state. */
export function registerCommunicationRuntime(
  pi: PiInstance, options: { refreshUi?: (ctx?: PiContext) => void } = {},
): void {
  const scripts = path.join(getAssetPaths().skillsDir, 'octocode-agents-communication', 'scripts');
  // Node 24 supports synchronous require(ESM). Register handlers before session_start.
  const adapter = createRequire(import.meta.url)(path.join(scripts, 'pi-inbox.mjs')) as InboxModule;
  const controller = adapter.registerPiInbox(pi, {
    binary: path.join(scripts, 'agents-communication'),
    enabled: () => isPersistentStorageEnabledForExtension(),
    onBinding: () => options.refreshUi?.(),
  });
  pi.on('tool_call', async (event, ctx) => {
    if (!isPersistentStorageEnabledForExtension()) return;
    const targets = communicationMutationTargets(event, ctx.cwd ?? process.cwd());
    if (!targets.length) return;
    if (!controller.getBinding() || !controller.isBoundContext(ctx)) {
      return { block: true, reason: 'Communication identity is unavailable or belongs to another session/workspace. Restore the session before checking shared edit leases.' };
    }
    try {
      // Bound batches avoid argument limits. This checks advisory conflicts, not permissions.
      for (let offset = 0; offset < targets.length; offset += 128) {
        const result = await controller.call('check_paths', {
          paths: targets.slice(offset, offset + 128).map(target => ({
            path: target,
            // Shell mutations can remove/move whole directories; check descendants too.
            kind: event.toolName.toLowerCase() === 'bash' ? 'tree' : 'file',
          })),
        });
        if (result.ok !== true) return {
          block: true,
          reason: `Shared edit lease conflict: ${JSON.stringify(result.conflicts ?? [])}. Ask the owner for a handoff with your reason, or work elsewhere until release/expiry. Reads remain available.`,
        };
      }
    } catch (error) {
      return { block: true, reason: `Cannot verify shared edit leases: ${error instanceof Error ? error.message : String(error)}` };
    }
    return undefined;
  });
}
