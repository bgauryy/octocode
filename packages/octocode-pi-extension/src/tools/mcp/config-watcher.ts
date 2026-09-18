import fs from 'node:fs';
import path from 'node:path';
import { ensurePrivateDirectory } from '@octocodeai/octocode-awareness/host';
import type { NotifyFn, PiContext } from '../../types.js';
import { isWorkerCapabilityClient } from '../worker-capabilities.js';
import { computeReload } from './catalog-refresh.js';
import {
  globalMcpConfigPaths,
  globalMcpPath,
  loadMcpConfig,
  projectMcpConfigPaths,
} from './config.js';

export interface McpConfigWatcher {
  start(ctx: PiContext | undefined, notify: NotifyFn): number;
  stop(): number;
}

export function createMcpConfigWatcher(options: {
  runningConfigSignatures(): Map<string, string>;
  stopConnection(name: string): Promise<boolean>;
  invalidateServer(name: string): void;
  invalidateWorkspace(ctx?: PiContext): void;
  markPromptStale(ctx?: PiContext): void;
  queueRefresh(ctx?: PiContext): void;
}): McpConfigWatcher {
  const watchers: fs.FSWatcher[] = [];
  let debounce: ReturnType<typeof setTimeout> | null = null;

  async function reconcile(ctx: PiContext | undefined, notify: NotifyFn): Promise<void> {
    try {
      const loaded = await loadMcpConfig(ctx);
      const { changed, removed } = computeReload(
        options.runningConfigSignatures(),
        loaded.servers,
      );
      for (const name of [...changed, ...removed]) {
        await options.stopConnection(name);
        options.invalidateServer(name);
      }
      options.invalidateWorkspace(ctx);
      options.markPromptStale(ctx);
      options.queueRefresh(ctx);
      if (changed.length || removed.length) {
        const parts: string[] = [];
        if (changed.length) parts.push(`reloaded ${changed.join(', ')}`);
        if (removed.length) parts.push(`removed ${removed.join(', ')}`);
        notify(
          ctx,
          `MCP config changed — ${parts.join('; ')}. Execution and routing refresh automatically; the next turn receives the current catalog.`,
          'info',
        );
      }
    } catch {
      // Atomic config replacement can expose a transient incomplete write. Per-call
      // loading remains the authoritative safety path when a watch refresh fails.
    }
  }

  function stop(): number {
    const count = watchers.length;
    for (const watcher of watchers) {
      try {
        watcher.close();
      } catch {
        // The platform may close a watcher before session shutdown.
      }
    }
    watchers.length = 0;
    if (debounce) {
      clearTimeout(debounce);
      debounce = null;
    }
    return count;
  }

  return {
    start(ctx, notify) {
      if (isWorkerCapabilityClient()) return 0;
      stop();
      const cwd = ctx?.cwd ?? process.cwd();
      const directories = new Set([
        ...globalMcpConfigPaths().map(filePath => path.dirname(filePath)),
        ...projectMcpConfigPaths(cwd).map(filePath => path.dirname(filePath)),
      ]);
      const canonicalGlobalDirectory = path.dirname(globalMcpPath());
      for (const directory of directories) {
        try {
          if (directory === canonicalGlobalDirectory) ensurePrivateDirectory(directory);
          else if (!fs.existsSync(directory)) continue;
          const watcher = fs.watch(
            directory,
            { persistent: false },
            (_event, filename) => {
              if (filename && !String(filename).startsWith('mcp.json')) return;
              if (debounce) clearTimeout(debounce);
              debounce = setTimeout(() => void reconcile(ctx, notify), 250);
            },
          );
          watchers.push(watcher);
        } catch {
          // Watch support is best effort; per-call loading and drift reconnect are mandatory.
        }
      }
      return watchers.length;
    },
    stop,
  };
}
