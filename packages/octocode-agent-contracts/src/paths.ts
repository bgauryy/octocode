/**
 * paths.ts — canonical Octocode home + on-disk layout.
 *
 * Single source of truth for where Octocode keeps its state. Everything lives
 * under the Octocode home (`~/.octocode` by default):
 *
 *   <home>/agent/agent.sqlite3                    Agent control/index DB
 *   <home>/agent/sessions/                        Agent session artifacts
 *   <home>/agent/workspaces/<workspace-key>/      workspace-keyed Agent files
 *
 * Product-home resolution is NEVER reimplemented — it delegates to
 * `@octocodeai/config` (`OCTOCODE_HOME` → platform default). The
 * launcher-scoped `OCTOCODE_AGENT_DIR` override applies only to the agent root.
 */
import { createHash } from 'node:crypto';
import { basename, join, resolve } from 'node:path';
import { getOctocodeHome as configGetOctocodeHome } from '@octocodeai/config';

/** Filename of the canonical agent SQLite store, under the agent root. */
export const AGENT_DB_FILENAME = 'agent.sqlite3';

/** Env var that pins the agent DB file, overriding the agent-root default. */
export const OCTOCODE_AGENT_DB_PATH_ENV = 'OCTOCODE_AGENT_DB_PATH';

/** Reserved per-session artifact buckets under `<home>/agent/sessions/<id>/`. */
export type SessionArtifact = 'compaction' | 'plans' | 'logs' | 'db';

/**
 * Resolve the product-wide Octocode home directory through `@octocodeai/config`.
 */
export function getOctocodeHome(env: NodeJS.ProcessEnv = process.env): string {
  return configGetOctocodeHome(env);
}

/**
 * Root for agent-owned databases, sessions, and other artifacts.
 * `OCTOCODE_AGENT_DIR` remains a launcher-compatible root override.
 */
export function agentHome(env: NodeJS.ProcessEnv = process.env): string {
  const override = env.OCTOCODE_AGENT_DIR?.trim();
  if (override) return resolve(override);
  return join(getOctocodeHome(env), 'agent');
}

/** Stable, readable key for workspace-scoped state kept inside the global agent home. */
export function workspaceAgentKey(cwd: string): string {
  const workspace = resolve(cwd);
  const readable = basename(workspace).replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48) || 'workspace';
  return `${readable}-${createHash('sha256').update(workspace).digest('hex').slice(0, 16)}`;
}

/** Global-only root for agent-owned files associated with one workspace. */
export function workspaceAgentRoot(cwd: string, octocodeHome = getOctocodeHome()): string {
  return join(resolve(octocodeHome), 'agent', 'workspaces', workspaceAgentKey(cwd));
}

/**
 * Path to the canonical agent SQLite store.
 * `OCTOCODE_AGENT_DB_PATH` is authoritative.
 */
export function agentDbPath(env: NodeJS.ProcessEnv = process.env): string {
  const override = env[OCTOCODE_AGENT_DB_PATH_ENV]?.trim();
  if (override) return resolve(override);
  return join(agentHome(env), AGENT_DB_FILENAME);
}

/** Root for Agent session artifacts and flat fallback session records. */
export function sessionsRoot(env: NodeJS.ProcessEnv = process.env): string {
  return join(agentHome(env), 'sessions');
}

/**
 * Normalise a session identifier into a filesystem-safe directory segment.
 * Falls back to a process-scoped id when no session id is available, matching
 * the harness convention (`pid-<pid>`).
 */
export function safeSessionId(sessionId?: string | null): string {
  const trimmed = (sessionId ?? '').trim();
  const safe = trimmed.replace(/[^\w.-]+/g, '_').slice(0, 96);
  return safe || `pid-${process.pid}`;
}

/** Directory holding all artifacts for one session. */
export function sessionDir(sessionId?: string | null, env: NodeJS.ProcessEnv = process.env): string {
  return join(sessionsRoot(env), safeSessionId(sessionId));
}

/** Directory for one artifact bucket within a session (e.g. `compaction`). */
export function sessionArtifactDir(
  sessionId: string | null | undefined,
  kind: SessionArtifact,
  env: NodeJS.ProcessEnv = process.env,
): string {
  return join(sessionDir(sessionId, env), kind);
}
