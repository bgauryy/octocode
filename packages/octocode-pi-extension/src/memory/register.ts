import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { repoKey, sharedAgentDb } from '../agentdb/db.js';
import type { Subcommands } from '../shared/commands.js';
import { envFlag, MEMORY_AUTO_ENV } from '../shared/env.js';
import { projectTrustNow } from '../shared/trust.js';
import { registerMemoryCommand } from './command.js';
import { injectedIds, MEMORY_MESSAGE_TYPE, selectInjection } from './inject.js';
import { MemoryStore } from './store.js';
import { registerMemoryTool } from './tool.js';

/** BM25 hits injected per prompt beside the pinned memories: fewer in a subagent, whose task is narrow. */
const TOP_K = 5;
const SUBAGENT_TOP_K = 3;

/** The memory store of `cwd`'s repository in the shared agent DB (the DB reopens when its path changes). */
export function memoryStore(cwd: string, env: NodeJS.ProcessEnv = process.env): MemoryStore {
  return new MemoryStore(sharedAgentDb(env), repoKey(cwd).key);
}

/**
 * Durable memories: the `memory` tool, automatic injection of pinned and relevant memories before each prompt (hidden
 * from the transcript), and `/octocode memory`. Project memories reach the model only in a trusted project.
 */
export function registerMemory(pi: ExtensionAPI, options: { commands: Subcommands; isSubagent: boolean; env?: NodeJS.ProcessEnv }): void {
  const env = options.env ?? process.env;
  const storeFor = (ctx: Pick<ExtensionContext, 'cwd'>) => memoryStore(ctx.cwd, env);
  const autoEnv = () => envFlag(env, MEMORY_AUTO_ENV, true);

  registerMemoryTool(pi, storeFor, { isSubagent: options.isSubagent });

  pi.on('before_agent_start', async (event, ctx) => {
    if (!autoEnv()) return undefined;
    try {
      const store = storeFor(ctx);
      if (!store.autoSetting()) return undefined;
      const injection = selectInjection(store, {
        query: event.prompt,
        scope: projectTrustNow(ctx) === true ? 'all' : 'global',
        seen: injectedIds(ctx.sessionManager.getBranch()),
        topK: options.isSubagent ? SUBAGENT_TOP_K : TOP_K,
        // A subagent's parent already holds the pinned memories and briefs it through the task.
        pinned: !options.isSubagent,
      });
      if (!injection) return undefined;
      store.markUsed(injection.ids);
      return { message: { customType: MEMORY_MESSAGE_TYPE, content: injection.content, display: false, details: { ids: injection.ids } } };
    } catch {
      // A missing or foreign agent DB never blocks a prompt; the tool and command report the error.
      return undefined;
    }
  });

  if (!options.isSubagent) registerMemoryCommand(options.commands, { store: storeFor, autoEnv });
}
