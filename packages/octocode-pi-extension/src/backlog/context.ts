import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { repoKey, sharedAgentDb } from '../agentdb/db.js';
import { BacklogStore, type Actor } from './store.js';

export interface BacklogOptions {
  isSubagent: boolean;
  /** This subagent's team id (the author of its notes); its session id is used when unset. */
  agentId?: () => string | undefined;
  /** The Pi process that started this subagent: its session is the parent whose items the subagent may update. */
  parentPid?: number;
  env?: NodeJS.ProcessEnv;
}

export interface Backlog {
  store: BacklogStore;
  actor: Actor;
  /** This session's id: the assignee its own claims carry. */
  session: string;
  repoName: string;
}

/** The backlog of `ctx.cwd`'s repository (worktrees share their main repository's) and who is acting in it. */
export function openBacklog(ctx: Pick<ExtensionContext, 'cwd' | 'sessionManager'>, options: BacklogOptions): Backlog {
  const repo = repoKey(ctx.cwd);
  const env = options.env ?? process.env;
  const store = new BacklogStore(() => sharedAgentDb(env), repo.key);
  const session = ctx.sessionManager.getSessionId();
  if (!options.isSubagent) return { store, actor: { id: session }, session, repoName: repo.name };
  const id = options.agentId?.() || session;
  // A subagent never claims: it may update only what its parent's sessions hold.
  const owners = new Set(store.sessionsOf(options.parentPid ?? process.ppid));
  return { store, actor: { id, owners }, session, repoName: repo.name };
}
