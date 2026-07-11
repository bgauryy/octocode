/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, file presence,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { registerAgent } from '../src/agents.js';
import { resolveDbPath } from '../src/db.js';
import { canonicalizePath, normalizeWorkspacePath } from '../src/git.js';
import type { WorkPeer } from '../src/types.js';
import { INTERNAL_HOOK_HOST, agentId, agentName, artifact, resolveHookPath, workspace } from './hook-payload.js';

export function peerStateDir(): string {
  const stateDir = join(dirname(resolveDbPath(null)), 'hook-state', 'peers');
  mkdirSync(stateDir, { recursive: true });
  return stateDir;
}

export function peerStateKey(payload: Record<string, unknown>, files: string[], cwd: string): string {
  return createHash('sha1').update(JSON.stringify({
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    artifact: artifact(payload),
    files: files.map(file => resolveHookPath(file, cwd)).sort(),
  })).digest('hex');
}

export function peerFingerprint(peers: WorkPeer[]): string {
  return createHash('sha1').update(JSON.stringify(peers.map((peer) => ({
    agent: peer.agent_id,
    file: peer.file_path,
    task: peer.task_id,
    origin: peer.origin,
    rationale: peer.rationale,
    exclusive: peer.exclusive,
  })).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))))).digest('hex');
}

export function peerLabel(peer: WorkPeer): string {
  const work = peer.task_id ?? peer.origin;
  const reason = peer.rationale.replace(/\s+/g, ' ').trim().slice(0, 40);
  return `${peer.agent_id}:${work}${reason ? `(${reason})` : ''}`;
}

export function emitPeerDelta(
  payload: Record<string, unknown>,
  files: string[],
  cwd: string,
  allPeers: WorkPeer[],
): string | null {
  const targetSet = new Set(files.map(file => resolveHookPath(file, cwd)));
  const peers = allPeers.filter(peer => peer.agent_id !== agentId(payload) && targetSet.has(peer.file_path));
  const key = peerStateKey(payload, files, cwd);
  const stateFile = join(peerStateDir(), `${key}.txt`);
  const fingerprint = peerFingerprint(peers);
  let previous: string | null = null;
  try { previous = readFileSync(stateFile, 'utf8').trim(); } catch { /* first delivery */ }
  if (previous === fingerprint) return null;
  writeFileSync(stateFile, fingerprint, 'utf8');
  if (peers.length === 0) return null;

  const shown = peers.slice(0, 3).map(peerLabel).join('; ');
  const omitted = peers.length > 3 ? ` +${peers.length - 3}` : '';
  const canonicalWorkspace = canonicalizePath(cwd);
  const targets = files.slice(0, 2).map(file => relative(canonicalWorkspace, resolveHookPath(file, cwd)) || basename(file)).join(',');
  return `AWARE ${targets} | peers ${shown}${omitted}`;
}

export function hookAgentContext(payload: Record<string, unknown>, hookName: string): string {
  const value =
    process.env.OCTOCODE_AGENT_CONTEXT
    ?? payload[INTERNAL_HOOK_HOST]
    ?? process.env.OCTOCODE_AGENT_HOST
    ?? payload.context
    ?? payload.host
    ?? payload.client
    ?? payload.source;
  return typeof value === 'string' && value.trim() ? value.trim() : hookName;
}

export function registerHookAgent(database: DatabaseSync, payload: Record<string, unknown>, hookName: string): void {
  try {
    registerAgent(database, {
      agentId: agentId(payload),
      agentName: agentName(payload),
      workspacePath: workspace(payload),
      artifact: artifact(payload),
      context: hookAgentContext(payload, hookName),
    });
  } catch {
    // Registry identity is useful for delivery, but hooks must fail open.
  }
}

export function scopeArgs(payload: Record<string, unknown>): { workspacePath?: string; artifact?: string } {
  const ws = workspace(payload);
  const art = artifact(payload);
  return {
    ...(ws ? { workspacePath: ws } : {}),
    ...(art ? { artifact: art } : {}),
  };
}
