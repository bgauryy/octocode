import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { realpathSync } from 'node:fs';
import type { DatabaseSync } from 'node:sqlite';
import { connectCachedDb, resolveDbPath } from './db.js';
import { insertEditLog } from './audit.js';

// HOOK-2: A one-time session startup token that survives process.pid reuse across
// OS restarts. We combine the session file name (if available) with a UUID suffix
// generated once at import time so the agentId is stable within a session but
// unique across sessions even when PIDs repeat.
const _sessionStartupToken = randomUUID().slice(0, 8);
import { normalizeArtifact, utcNow } from './helpers.js';
import { activeTaskClaimForAgent } from './tasks.js';
import { endWork, listWork, startWork, touchWork } from './work.js';
import type { WorkPeer } from './types.js';
import { auditUnverified } from './verify.js';
import { notifyGet, sessionCapture } from './maintenance.js';
import { registerAgent } from './agents.js';

export interface PiLikeSessionManager {
  getSessionFile?: () => string | null | undefined;
}

export interface PiLikeUi {
  notify?: (message: string, level?: string) => void;
}

export interface PiLikeContext {
  cwd?: string;
  dbPath?: string;
  artifact?: string;
  sessionManager?: PiLikeSessionManager;
  ui?: PiLikeUi;
}

export interface PiLikeApi {
  on?: (eventName: string, handler: (event: Record<string, unknown>, ctx: PiLikeContext) => unknown | Promise<unknown>) => void;
  sendMessage?: (message: Record<string, unknown>, options?: Record<string, unknown>) => void;
}

export interface PiToolEvent {
  toolName?: string;
  toolCallId?: string;
  input?: unknown;
}

export interface PiAwarenessBridgeOptions {
  pendingToolFiles?: Map<string, string[]>;
  pendingToolRuns?: Map<string, string>;
  peerFingerprints?: Map<string, string>;
  dbPath?: string | null;
  getDb?: (ctx?: PiLikeContext) => DatabaseSync;
  skillRoot?: string | null;
}

function addPathValue(paths: string[], value: unknown): void {
  if (typeof value === 'string' && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) addPathValue(paths, item);
  }
}

function addApplyPatchPaths(paths: string[], command: unknown): void {
  if (typeof command !== 'string') return;
  for (const line of command.split('\n')) {
    const addUpdDel = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);
    if (addUpdDel) {
      paths.push(addUpdDel[1]!.trim());
      continue;
    }
    const moveTo = line.match(/^\*\*\* Move to: (.+)$/);
    if (moveTo) paths.push(moveTo[1]!.trim());
  }
}

function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

function addQueryPaths(paths: string[], value: unknown): void {
  if (!Array.isArray(value)) return;
  for (const query of value) {
    const payload = objectOrEmpty(query);
    addPathValue(paths, payload.path);
    addPathValue(paths, payload.filePath);
    addPathValue(paths, payload.file_path);
    addPathValue(paths, payload.paths);
    addPathValue(paths, payload.filePaths);
    addPathValue(paths, payload.file_paths);
  }
}

export function extractPiWriteTargetPaths(
  toolName: unknown,
  input: unknown = {},
  options: { assumeWrite?: boolean } = {},
): string[] {
  const normalizedToolName = String(toolName ?? '').toLowerCase();
  const isWriteTool = Boolean(options.assumeWrite) || [
    'write',
    'edit',
    'multi_edit',
    'multiedit',
    'notebookedit',
    'notebook_edit',
    'apply_patch',
    'applypatch',
  ].includes(normalizedToolName);
  const payload = objectOrEmpty(input);
  // Source for apply_patch marker scanning (addApplyPatchPaths). Only true patch
  // carriers — a raw string input, or `command`/`patch` fields — are scanned.
  // `text`/`content` are the FILE BODY for Write/Edit; scanning them would turn
  // any file line like `*** Add File: X` (e.g. these very docs) into a phantom
  // lock + edit_log target. Write/Edit paths come from the explicit path fields
  // below, not from the body.
  const command = typeof input === 'string'
    ? input
    : firstString(payload.command, payload.patch);

  if (!isWriteTool) {
    const patchPaths: string[] = [];
    addApplyPatchPaths(patchPaths, command);
    return [...new Set(patchPaths)];
  }

  const paths: string[] = [];
  addPathValue(paths, payload.path);
  addPathValue(paths, payload.filePath);
  addPathValue(paths, payload.file_path);
  addPathValue(paths, payload.paths);
  addPathValue(paths, payload.filePaths);
  addPathValue(paths, payload.file_paths);
  addQueryPaths(paths, payload.queries);
  addApplyPatchPaths(paths, command);

  return [...new Set(paths)];
}

function artifactFrom(ctx?: PiLikeContext, event?: Record<string, unknown>): string | null {
  const input = objectOrEmpty(event?.input);
  return normalizeArtifact(firstString(
    process.env.OCTOCODE_ARTIFACT,
    process.env.OCTOCODE_PACKAGE,
    process.env.OCTOCODE_SERVICE,
    ctx?.artifact,
    event?.artifact,
    event?.package,
    event?.service,
    input.artifact,
    input.package,
    input.service,
  ));
}

export function getPiAwarenessSessionId(ctx?: PiLikeContext): string {
  const sessionFile = ctx?.sessionManager?.getSessionFile?.();
  if (sessionFile) return `pi-session:${path.basename(sessionFile, path.extname(sessionFile))}`;
  // HOOK-2: Same pid-reuse fix as getPiAwarenessAgentId — append startup token so
  // sessions from different OS boots with the same PID don't share lock scope.
  return `pi-session:${process.pid}-${_sessionStartupToken}`;
}

export function getPiAwarenessAgentId(ctx?: PiLikeContext): string {
  if (process.env.OCTOCODE_AGENT_ID) return process.env.OCTOCODE_AGENT_ID;

  const sessionFile = ctx?.sessionManager?.getSessionFile?.();
  if (sessionFile) return `pi:${path.basename(sessionFile, path.extname(sessionFile))}`;

  // HOOK-2: Append the startup token to the pid so that two processes with the
  // same pid (OS pid reuse across restarts) produce different agent IDs and do
  // not mix memory contexts. The token is stable for the lifetime of this process.
  return `pi:${process.pid}-${_sessionStartupToken}`;
}

function notify(ctx: PiLikeContext | undefined, message: string, level: string = 'info'): void {
  ctx?.ui?.notify?.(message, level);
}

function defaultGetDb(options: PiAwarenessBridgeOptions, ctx?: PiLikeContext): DatabaseSync {
  // HOOK-1: Use the cached connection; never call connectDb twice for the same path.
  return connectCachedDb(ctx?.dbPath ?? options.dbPath ?? resolveDbPath(null));
}

function ensurePiSession(
  db: DatabaseSync,
  params: { agentId: string; sessionId: string; workspacePath: string; artifact: string | null },
): void {
  db.prepare(`INSERT OR IGNORE INTO sessions
    (session_id, agent_id, workspace_path, artifact, started_at) VALUES (?, ?, ?, ?, ?)`)
    .run(params.sessionId, params.agentId, params.workspacePath, params.artifact, utcNow());
}

function canonicalPath(input: string): string {
  const resolved = path.resolve(input);
  try {
    return realpathSync(resolved);
  } catch {
    const missingParts: string[] = [];
    let cursor = resolved;
    while (true) {
      const parent = path.dirname(cursor);
      if (parent === cursor) return resolved;
      missingParts.unshift(path.basename(cursor));
      cursor = parent;
      try {
        return path.join(realpathSync(cursor), ...missingParts);
      } catch {
        continue;
      }
    }
  }
}

function resolvePiTargetPath(file: string, cwd: string): string {
  return canonicalPath(path.isAbsolute(file) ? file : path.resolve(cwd, file));
}

function activeWorkRunForFiles(
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

function workRunOrigin(db: DatabaseSync, runId: string): 'TASK' | 'WORK' | 'HOOK' | null {
  const row = db.prepare('SELECT origin FROM task_runs WHERE run_id = ?').get(runId) as { origin: 'TASK' | 'WORK' | 'HOOK' } | undefined;
  return row?.origin ?? null;
}

function piPeerDelta(
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

function isInsidePath(candidate: string, root: string): boolean {
  const resolvedCandidate = canonicalPath(candidate);
  const resolvedRoot = canonicalPath(root);
  const rel = path.relative(resolvedRoot, resolvedCandidate);
  return rel === '' || Boolean(rel && !rel.startsWith('..') && !path.isAbsolute(rel));
}

function gitBranchOf(dir: string): string | null {
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

function guardPiHarnessEdit(targetFiles: string[], ctx: PiLikeContext | undefined, skillRoot: string | null | undefined): string | null {
  return evaluateHarnessGuard({ targetFiles, skillRoot, cwd: ctx?.cwd ?? process.cwd() });
}

export function createPiAwarenessBridge(options: PiAwarenessBridgeOptions = {}) {
  const pendingToolFiles = options.pendingToolFiles ?? new Map<string, string[]>();
  const pendingToolRuns = options.pendingToolRuns ?? new Map<string, string>();
  const peerFingerprints = options.peerFingerprints ?? new Map<string, string>();
  const getDb = options.getDb ?? ((ctx?: PiLikeContext) => defaultGetDb(options, ctx));
  const skillRoot = options.skillRoot ?? process.env.OCTOCODE_SKILL_ROOT ?? null;

  return {
    pendingToolFiles,
    pendingToolRuns,
    peerFingerprints,

    async handleToolCall(event: PiToolEvent, ctx?: PiLikeContext) {
      const targetFiles = extractPiWriteTargetPaths(event?.toolName, event?.input);
      if (targetFiles.length === 0) return undefined;
      // Dedupe key: a host may emit BOTH tool_call and tool_execution_start for
      // one edit. Prefer toolCallId; when it is missing, fall back to the sorted
      // target-file set (identical across both events for the same edit) so a
      // missing id cannot cause a duplicate presence declaration. Distinct edits yield
      // distinct file sets, so this never over-dedupes real work.
      const dedupeKey = event?.toolCallId || `nofid:${[...targetFiles].sort().join('|')}`;
      if (pendingToolRuns.has(dedupeKey)) return undefined;
      const harnessBlockReason = guardPiHarnessEdit(targetFiles, ctx, skillRoot);
      if (harnessBlockReason) return { block: true, reason: harnessBlockReason };

      const agentId = getPiAwarenessAgentId(ctx);
      try {
        const db = getDb(ctx);
        const activeClaim = activeTaskClaimForAgent(db, {
          agentId,
          workspacePath: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, event as Record<string, unknown>),
        });
        const workspacePath = ctx?.cwd ?? process.cwd();
        const artifact = artifactFrom(ctx, event as Record<string, unknown>);
        ensurePiSession(db, {
          agentId,
          sessionId: getPiAwarenessSessionId(ctx),
          workspacePath,
          artifact,
        });
        const explicitRunId = activeClaim ? null : activeWorkRunForFiles(db, {
          agentId,
          workspacePath,
          artifact,
          targetFiles,
        });
        const result = explicitRunId
          ? { ok: true as const, ...touchWork(db, {
            agentId,
            runId: explicitRunId,
            targetFiles,
            ttlMs: 10 * 60_000,
          }) }
          : startWork(db, {
            agentId,
            sessionId: getPiAwarenessSessionId(ctx),
            workspacePath,
            artifact,
            runId: activeClaim?.run_id,
            rationale: 'auto: Pi write/edit tool call via octocode-awareness',
            testPlan: targetFiles.length > 0
              ? `verify edit applied to: ${targetFiles.slice(0, 3).join(', ')}${targetFiles.length > 3 ? ` + ${targetFiles.length - 3} more` : ''}`
              : 'post-edit verification',
            targetFiles,
            origin: 'HOOK',
            source: 'HOOK',
            ttlMs: 10 * 60_000,
          });

        if (!result.ok) {
          const detail = (result.conflicts || [])
            .map((conflict) => `${conflict.file_path} (held by ${conflict.agent_id})`)
            .join(', ');
          return { block: true, reason: `Octocode awareness blocked this edit: ${detail || 'conflict'}` };
        }

        pendingToolFiles.set(dedupeKey, targetFiles);
        pendingToolRuns.set(dedupeKey, result.run.run_id);
        const peerContext = piPeerDelta(peerFingerprints, {
          agentId,
          workspacePath,
          targetFiles,
          peers: result.peers,
        });
        if (!peerContext) return undefined;
        notify(ctx, peerContext);
        return { additionalContext: peerContext };
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
        return undefined;
      }
    },

    async handleToolResult(event: PiToolEvent, ctx?: PiLikeContext) {
      const extracted = extractPiWriteTargetPaths(event?.toolName, event?.input);
      // Mirror the acquire-side key (toolCallId, else sorted target files) so the
      // post-edit handling finds the run the matching handleToolCall recorded.
      const dedupeKey = event?.toolCallId || `nofid:${[...extracted].sort().join('|')}`;
      const trackedFiles = pendingToolRuns.has(dedupeKey) ? pendingToolFiles.get(dedupeKey) : undefined;
      const runId = pendingToolRuns.get(dedupeKey);
      const fallbackFiles = trackedFiles ?? extracted;
      if (fallbackFiles.length === 0 && !runId) return undefined;

      pendingToolFiles.delete(dedupeKey);
      pendingToolRuns.delete(dedupeKey);
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const sessionId = getPiAwarenessSessionId(ctx);
        const workspacePath = ctx?.cwd ?? process.cwd();
        const artifact = artifactFrom(ctx, event as Record<string, unknown>);
        if (!runId) {
          notify(ctx, 'Octocode awareness post-edit warning; continuing: missing correlated work run.', 'warning');
          return undefined;
        }
        if (workRunOrigin(db, runId) === 'HOOK') {
          endWork(db, { agentId, runId, targetFiles: fallbackFiles });
        } else {
          touchWork(db, { agentId, runId, targetFiles: fallbackFiles, ttlMs: 10 * 60_000 });
        }
        for (const file of fallbackFiles) {
          insertEditLog(db, {
            sessionId,
            runId,
            agentId,
            filePath: resolvePiTargetPath(file, workspacePath),
            operation: 'update',
            workspacePath,
            artifact,
          });
        }
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      }
      return undefined;
    },

    async handleBeforeAgentStart(_event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      // ARCH-5: Register / refresh agent identity at the start of each session.
      // Uses OCTOCODE_AGENT_NAME env (if set) or session file basename as display name.
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const envName = process.env.OCTOCODE_AGENT_NAME ?? '';
        const sessionFile = ctx?.sessionManager?.getSessionFile?.();
        const derivedName = envName
          || (sessionFile ? path.basename(sessionFile, path.extname(sessionFile)) : '');
        registerAgent(db, { agentId, agentName: derivedName, workspacePath: ctx?.cwd ?? process.cwd(), artifact: artifactFrom(ctx, _event), context: 'pi' });
      } catch { /* fail-open: identity registration is non-critical */ }

      if (process.env.OCTOCODE_NO_NOTIFY === '1') return undefined;
      try {
        const db = getDb(ctx);
        const result = notifyGet(db, {
          agent_id: getPiAwarenessAgentId(ctx),
          workspace: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, _event),
          format: 'hook',
        }) as { additionalContext?: string };
        if (!result.additionalContext) return undefined;
        return {
          message: {
            customType: 'octocode-awareness-briefing',
            content: result.additionalContext,
            display: false,
          },
        };
      } catch (error) {
        notify(ctx, `Octocode awareness briefing warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
        return undefined;
      }
    },

    async handleSessionShutdown(event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      if (process.env.OCTOCODE_NO_SESSION_CAPTURE === '1') return undefined;
      if (event.reason === 'new') return undefined;
      try {
        const db = getDb(ctx);
        sessionCapture(db, {
          agent_id: getPiAwarenessAgentId(ctx),
          workspace: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, event),
          reason: event.reason,
        });
      } catch {
        // fail-open: shutdown hooks must never wedge session replacement/quit
      }
      return undefined;
    },
  };
}

export function wirePiAwarenessHooks(pi: PiLikeApi, options: PiAwarenessBridgeOptions = {}) {
  if (!pi?.on) return null;
  const bridge = createPiAwarenessBridge(options);
  const verifyReminderKeys = new Set<string>();

  pi.on('tool_call', async (event, ctx) => bridge.handleToolCall(event as PiToolEvent, ctx));
  pi.on('tool_result', async (event, ctx) => bridge.handleToolResult(event as PiToolEvent, ctx));
  pi.on('tool_execution_start', async (event, ctx) => bridge.handleToolCall({
    toolCallId: String(event?.toolCallId ?? ''),
    toolName: String(event?.toolName ?? ''),
    input: event?.args,
  }, ctx));
  pi.on('tool_execution_end', async (event, ctx) => bridge.handleToolResult({
    toolCallId: String(event?.toolCallId ?? ''),
    toolName: String(event?.toolName ?? ''),
  }, ctx));
  pi.on('before_agent_start', async (event, ctx) => bridge.handleBeforeAgentStart(event, ctx));
  pi.on('agent_end', async (_event, ctx) => {
    if (process.env.OCTOCODE_NO_VERIFY_GATE === '1') return undefined;
    try {
      const db = (options.getDb ?? ((hookCtx?: PiLikeContext) => defaultGetDb(options, hookCtx)))(ctx);
      const result = auditUnverified(db, {
        agentId: getPiAwarenessAgentId(ctx),
        workspacePath: ctx?.cwd ?? process.cwd(),
        artifact: artifactFrom(ctx, _event),
      });
      if (result.count === 0) {
        verifyReminderKeys.clear();
        return undefined;
      }
      const reminderKey = JSON.stringify({
        agentId: getPiAwarenessAgentId(ctx),
        workspacePath: ctx?.cwd ?? process.cwd(),
        artifact: artifactFrom(ctx, _event),
        runIds: [
          ...result.unverified.map((intent) => intent.run_id),
          ...result.stale_active.map((intent) => intent.run_id),
        ].sort(),
      });
      if (verifyReminderKeys.has(reminderKey)) return undefined;
      verifyReminderKeys.add(reminderKey);
      const details = [
        ...result.unverified.map((intent) => `${intent.status}:${intent.run_id}: ${intent.test_plan}`),
        ...result.stale_active.map((intent) => `STALE:${intent.run_id}: ${intent.rationale}`),
      ];
      const shown = details.slice(0, 3).join('; ');
      const omitted = details.length > 3 ? `; +${details.length - 3} omitted` : '';
      pi.sendMessage?.({
        customType: 'octocode-awareness-verify-gate',
        content: [
          'Octocode awareness verify gate: you have unverified edits before concluding.',
          shown ? `Pending: ${shown}${omitted}` : '',
          'Run the stated verification, then use octocode-awareness verify mark to clear the pending runs.',
        ].filter(Boolean).join('\n'),
        display: true,
      }, { deliverAs: 'followUp', triggerTurn: true });
      return undefined;
    } catch (error) {
      notify(ctx, `Octocode awareness verify warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      return undefined;
    }
  });
  pi.on('session_before_compact', async (event, ctx) => bridge.handleSessionShutdown({
    reason: typeof event?.reason === 'string' ? `compact:${event.reason}` : 'compact',
  }, ctx));
  pi.on('session_shutdown', async (event, ctx) => bridge.handleSessionShutdown(event, ctx));

  return bridge;
}
