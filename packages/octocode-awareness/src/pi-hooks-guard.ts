import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import type { DatabaseSync } from 'node:sqlite';
import { normalizeArtifact } from './helpers.js';
import { normalizeWorkspacePath } from './git.js';
import { endWork, listWork, startWork, touchWork } from './work.js';
import type { WorkPeer } from './types.js';
import { canonicalPath, PiLikeContext } from './pi-hooks-inputs.js';

export function resolvePiTargetPath(file: string, cwd: string): string {
  return canonicalPath(path.isAbsolute(file) ? file : path.resolve(cwd, file));
}

export function activeWorkRunForFiles(
  db: DatabaseSync,
  params: { agentId: string; workspacePath: string; artifact: string | null; targetFiles: string[] },
): string | null {
  const targets = params.targetFiles.map(file => resolvePiTargetPath(file, params.workspacePath));
  const rows = listWork(db, {
    agentId: params.agentId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    activeOnly: true,
  }).files.filter(entry => entry.origin === 'WORK');
  const byRun = new Map<string, Set<string>>();
  for (const row of rows) {
    const files = byRun.get(row.run_id) ?? new Set<string>();
    files.add(row.file_path);
    byRun.set(row.run_id, files);
  }
  const matches = [...byRun].filter(([, files]) => targets.every(target => files.has(target)));
  return matches.length === 1 ? matches[0]![0] : null;
}

export function workRunOrigin(db: DatabaseSync, runId: string): 'TASK' | 'WORK' | 'HOOK' | null {
  const row = db.prepare('SELECT origin FROM task_runs WHERE run_id = ?').get(runId) as { origin: 'TASK' | 'WORK' | 'HOOK' } | undefined;
  return row?.origin ?? null;
}

export const PI_HOOK_AGGREGATE_CONTEXT_PREFIX = 'pi-hook-scope:';

export function piHookAggregateContextRef(params: {
  agentId: string;
  sessionId: string;
  workspacePath: string;
  artifact: string | null;
}): string {
  const identity = {
    agent: params.agentId,
    session: params.sessionId,
    workspace: normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? path.resolve(params.workspacePath),
    artifact: normalizeArtifact(params.artifact),
  };
  return `${PI_HOOK_AGGREGATE_CONTEXT_PREFIX}${createHash('sha1').update(JSON.stringify(identity)).digest('hex')}`;
}

export function activePiFallbackHookRun(db: DatabaseSync, params: {
  agentId: string;
  sessionId: string;
  workspacePath: string;
  artifact: string | null;
}): string | null {
  const row = db.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY updated_at DESC, created_at DESC LIMIT 1`).get(
    params.agentId,
    normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? path.resolve(params.workspacePath),
    normalizeArtifact(params.artifact),
    piHookAggregateContextRef(params),
  ) as { run_id: string } | undefined;
  return row?.run_id ?? null;
}

export function isAggregatedPiFallbackRun(db: DatabaseSync, runId: string): boolean {
  const row = db.prepare('SELECT origin, context_ref FROM task_runs WHERE run_id = ?').get(runId) as {
    origin: string;
    context_ref: string | null;
  } | undefined;
  return row?.origin === 'HOOK' && row.context_ref?.startsWith(PI_HOOK_AGGREGATE_CONTEXT_PREFIX) === true;
}

export function piFallbackVerificationPlan(files: string[], workspacePath: string): string {
  const root = canonicalPath(workspacePath);
  const normalized = [...new Set(files.map(file => resolvePiTargetPath(file, workspacePath)))];
  const shown = normalized.slice(0, 3).map(file => path.relative(root, file) || path.basename(file)).join(', ');
  const omitted = normalized.length > 3 ? ` (+${normalized.length - 3} more)` : '';
  return `Verify ${shown || 'the edited files'}${omitted}: run the smallest relevant test/typecheck and inspect the diff; record the check and result.`;
}

export function refreshPiFallbackVerificationPlan(db: DatabaseSync, runId: string, workspacePath: string): void {
  if (!isAggregatedPiFallbackRun(db, runId)) return;
  const files = db.prepare('SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path')
    .all(runId) as unknown as Array<{ file_path: string }>;
  db.prepare("UPDATE task_runs SET test_plan = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE run_id = ? AND origin = 'HOOK'")
    .run(piFallbackVerificationPlan(files.map(file => file.file_path), workspacePath), runId);
}

export function startOrAttachPiFallbackRun(db: DatabaseSync, params: {
  agentId: string;
  sessionId: string;
  workspacePath: string;
  artifact: string | null;
  targetFiles: string[];
}) {
  // Pi dispatch is in-process. Keep lookup + synchronous SQLite mutation in one
  // no-await critical section so parallel tool callbacks coalesce deterministically.
  const existingRunId = activePiFallbackHookRun(db, params);
  const result = startWork(db, {
    agentId: params.agentId,
    sessionId: params.sessionId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    runId: existingRunId ?? undefined,
    rationale: 'auto: Pi write/edit tool call via octocode-awareness',
    testPlan: piFallbackVerificationPlan(params.targetFiles, params.workspacePath),
    contextRef: piHookAggregateContextRef(params),
    targetFiles: params.targetFiles,
    origin: 'HOOK',
    source: 'HOOK',
    ttlMs: 10 * 60_000,
  });
  if (result.ok && existingRunId) {
    touchWork(db, { agentId: params.agentId, runId: existingRunId, ttlMs: 10 * 60_000 });
  }
  if (result.ok) refreshPiFallbackVerificationPlan(db, result.run.run_id, params.workspacePath);
  return result;
}

export function finalizeActivePiFallbackRuns(db: DatabaseSync, params: {
  agentId: string;
  sessionId: string;
  workspacePath: string;
  artifact: string | null;
}): string[] {
  const rows = db.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY created_at`).all(
    params.agentId,
    normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? path.resolve(params.workspacePath),
    normalizeArtifact(params.artifact),
    piHookAggregateContextRef(params),
  ) as unknown as Array<{ run_id: string }>;
  for (const row of rows) endWork(db, { agentId: params.agentId, runId: row.run_id });
  return rows.map(row => row.run_id);
}

export function piPeerDelta(
  peerFingerprints: Map<string, string>,
  params: { agentId: string; workspacePath: string; targetFiles: string[]; peers: WorkPeer[] },
): string | null {
  const targetSet = new Set(params.targetFiles.map(file => resolvePiTargetPath(file, params.workspacePath)));
  const peers = params.peers.filter(peer => peer.agent_id !== params.agentId && targetSet.has(peer.file_path));
  const key = JSON.stringify({
    agent: params.agentId,
    workspace: path.resolve(params.workspacePath),
    files: params.targetFiles.map(file => resolvePiTargetPath(file, params.workspacePath)).sort(),
  });
  const fingerprint = JSON.stringify(peers.map(peer => ({
    agent: peer.agent_id,
    file: peer.file_path,
    task: peer.task_id,
    origin: peer.origin,
    rationale: peer.rationale,
    exclusive: peer.exclusive,
  })).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))));
  if (peerFingerprints.get(key) === fingerprint) return null;
  peerFingerprints.set(key, fingerprint);
  if (peers.length === 0) return null;
  const shown = peers.slice(0, 3).map((peer) => {
    const work = peer.task_id ?? peer.origin;
    const reason = peer.rationale.replace(/\s+/g, ' ').trim().slice(0, 40);
    return `${peer.agent_id}:${work}${reason ? `(${reason})` : ''}`;
  }).join('; ');
  const omitted = peers.length > 3 ? ` +${peers.length - 3}` : '';
  const targets = params.targetFiles.slice(0, 2).join(',');
  return `AWARE ${targets} | peers ${shown}${omitted}`;
}

export function isInsidePath(candidate: string, root: string): boolean {
  const resolvedCandidate = canonicalPath(candidate);
  const resolvedRoot = canonicalPath(root);
  const rel = path.relative(resolvedRoot, resolvedCandidate);
  return rel === '' || Boolean(rel && !rel.startsWith('..') && !path.isAbsolute(rel));
}

export function gitBranchOf(dir: string): string | null {
  try {
    const result = spawnSync('git', ['-C', dir, 'rev-parse', '--abbrev-ref', 'HEAD'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    return result.status === 0 ? String(result.stdout).trim() : null;
  } catch {
    return null;
  }
}

/**
 * Single source of truth for the harness self-edit gate, shared by the Pi
 * bridge and the shell hook runner (bin/hook-runner.ts) so the two vendors can
 * never drift. Returns a human-readable block reason, or null to allow.
 *
 * Gate (only when a target resolves inside `skillRoot`):
 *   1. OCTOCODE_ALLOW_HARNESS_APPLY=1 must be set (human approval).
 *   2. The skill root's git branch must not be main/master.
 *   3. A detached HEAD or non-repo skill root needs OCTOCODE_HARNESS_BRANCH_OK=1.
 */
export function evaluateHarnessGuard(params: {
  targetFiles: string[];
  skillRoot: string | null | undefined;
  cwd: string;
  env?: NodeJS.ProcessEnv;
}): string | null {
  const { targetFiles, skillRoot, cwd } = params;
  const env = params.env ?? process.env;
  if (!skillRoot) return null;
  if (targetFiles.length === 0) return null;
  const insideSkill = targetFiles.some((file) => isInsidePath(resolvePiTargetPath(file, cwd), skillRoot));
  if (!insideSkill) return null;

  if (env.OCTOCODE_ALLOW_HARNESS_APPLY !== '1') {
    return 'octocode-awareness: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1.';
  }

  const branch = gitBranchOf(skillRoot);
  if (branch === 'main' || branch === 'master') {
    return `octocode-awareness: harness self-fix is never allowed on ${branch}. Create a dedicated branch first.`;
  }
  if (!branch || branch === 'HEAD') {
    if (env.OCTOCODE_HARNESS_BRANCH_OK !== '1') {
      return 'octocode-awareness: cannot confirm a dedicated git branch for the skill. Create one, or set OCTOCODE_HARNESS_BRANCH_OK=1 to acknowledge.';
    }
  }

  return null;
}

export function guardPiHarnessEdit(targetFiles: string[], ctx: PiLikeContext | undefined, skillRoot: string | null | undefined): string | null {
  return evaluateHarnessGuard({ targetFiles, skillRoot, cwd: ctx?.cwd ?? process.cwd() });
}
