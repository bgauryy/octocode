/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, file presence,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { createHash } from 'node:crypto';
import {
  closeSync, mkdirSync, openSync, readFileSync, renameSync, statSync, unlinkSync, writeFileSync,
} from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { registerAgent } from '../src/agents.js';
import { insertEditLog } from '../src/audit.js';
import { connectDb, resolveDbPath } from '../src/db.js';
import { canonicalizePath, normalizeWorkspacePath } from '../src/git.js';
import { activeTaskClaimForAgent } from '../src/tasks.js';
import { endWork, listWork, startWork, touchWork } from '../src/work.js';
import type { WorkPeer } from '../src/types.js';
import { auditUnverified } from '../src/verify.js';
import { digest, notifyGet, sessionCapture } from '../src/maintenance.js';
import { endSession } from '../src/sessions.js';
import { evaluateHarnessGuard, extractPiWriteTargetPaths } from '../src/pi-hooks.js';

function readStdin(): Promise<string> {
  return new Promise((resolve) => {
    let raw = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => { raw += chunk; });
    process.stdin.on('end', () => resolve(raw));
    process.stdin.on('error', () => resolve(raw));
  });
}

function parsePayload(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw || '{}');
    return parsed && typeof parsed === 'object' ? parsed as Record<string, unknown> : {};
  } catch {
    return raw.trim() ? { input: raw } : {};
  }
}

function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

function payloadInput(payload: Record<string, unknown>): unknown {
  return payload.tool_input ?? payload.input ?? payload.args ?? payload;
}

function payloadForFileExtraction(payload: Record<string, unknown>): unknown {
  const input = payloadInput(payload);
  const inputObj = objectOrEmpty(input);
  if (inputObj === payload) return input;
  if (Object.keys(inputObj).length === 0) return input;
  return { ...payload, ...inputObj };
}

let warnedFallbackAgentId = false;

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

function agentId(payload: Record<string, unknown>): string {
  const input = objectOrEmpty(payloadInput(payload));
  const explicit = firstString(
    process.env.OCTOCODE_AGENT_ID,
    payload.agent_id,
    payload.agentId,
    input.agent_id,
    input.agentId,
    payload.session_id,
    payload.sessionId,
    input.session_id,
    input.sessionId,
  );
  if (explicit) return explicit;

  const host = firstString(
    process.env.OCTOCODE_AGENT_HOST,
    payload.host,
    payload.client,
    payload.source,
    payload.context,
  ) ?? 'shell';
  const scope = `${host}\0${workspace(payload) ?? process.cwd()}`;
  const suffix = createHash('sha1').update(scope).digest('hex').slice(0, 12);
  const fallback = `hook:${host.replace(/[^a-zA-Z0-9_.:-]/g, '_')}:${suffix}`;
  if (!warnedFallbackAgentId) {
    warnedFallbackAgentId = true;
    console.error(`octocode-awareness: OCTOCODE_AGENT_ID or host session id missing; using fallback agent id "${fallback}". Set OCTOCODE_AGENT_ID for reliable multi-agent awareness.`);
  }
  return fallback;
}

function sessionId(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.session_id, payload.sessionId, input.session_id, input.sessionId,
  );
}

function toolName(payload: Record<string, unknown>): string {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.tool_name, payload.toolName, payload.name, input.tool_name, input.toolName,
  ) ?? '';
}

// Build an informative auto-claim rationale from the tool + target files so a
// blocked agent sees WHAT the holder is doing, not a generic "file edit".
function autoClaimRationale(payload: Record<string, unknown>, files: string[]): string {
  const tool = toolName(payload);
  const names = files.map((f) => f.split('/').pop() || f);
  const shown = names.slice(0, 3).join(', ');
  const extra = names.length > 3 ? ` +${names.length - 3} more` : '';
  const action = tool ? `${tool}` : 'edit';
  return `auto: ${action} ${shown}${extra} (lifecycle hook)`;
}

function agentName(payload: Record<string, unknown>): string {
  const value =
    process.env.OCTOCODE_AGENT_NAME
    ?? payload.agent_name
    ?? payload.agentName
    ?? payload.agent_display_name
    ?? payload.agentDisplayName;
  return typeof value === 'string' && value.trim() ? value.trim() : '';
}

function workspace(payload: Record<string, unknown>): string | null {
  const value = payload.cwd ?? payload.workspace ?? payload.workspacePath;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function artifact(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  const value =
    process.env.OCTOCODE_ARTIFACT
    ?? process.env.OCTOCODE_PACKAGE
    ?? process.env.OCTOCODE_SERVICE
    ?? payload.artifact
    ?? payload.package
    ?? payload.service
    ?? input.artifact
    ?? input.package
    ?? input.service;
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function hookReason(payload: Record<string, unknown>): string {
  return typeof payload.reason === 'string' ? payload.reason : '';
}

function isStopHookActive(payload: Record<string, unknown>): boolean {
  return Boolean(payload.stop_hook_active);
}

function extractFiles(payload: Record<string, unknown>): string[] {
  const input = payloadForFileExtraction(payload);
  const inputObj = objectOrEmpty(input);
  const toolName = payload.tool_name ?? payload.toolName ?? payload.name ?? inputObj.tool_name ?? inputObj.toolName ?? '';
  return extractPiWriteTargetPaths(toolName, input, { assumeWrite: true });
}

function resolveHookPath(file: string, cwd = process.cwd()): string {
  // Absolutize AND normalize: `..`/`.` segments and non-absolute inputs (Codex
  // apply_patch and Cursor payloads often carry repo-relative paths) must be
  // collapsed before any containment check, or a traversal path that actually
  // resolves inside the skill root can slip past a textual prefix comparison.
  return canonicalizePath(resolve(cwd, file));
}

function db() {
  return connectDb(resolveDbPath(null));
}

interface HookRunStateEntry {
  runId: string;
  files: string[];
  createdAt: string;
}

const HOOK_RUN_STATE_TTL_MS = 10 * 60_000;
const HOOK_RUN_STATE_LOCK_WAIT = new Int32Array(new SharedArrayBuffer(4));
const HOOK_RUN_STATE_LOCK_RETRY_MS = 10;
const HOOK_RUN_STATE_LOCK_TIMEOUT_MS = 2_000;
const HOOK_RUN_STATE_LOCK_STALE_MS = 30_000;
const HOOK_DB_RETRY_TIMEOUT_MS = 5_000;

function isHookDbBusy(error: unknown): boolean {
  const sqlite = error as { errcode?: number; errstr?: string; message?: string } | null;
  const message = sqlite && typeof sqlite === 'object'
    ? `${sqlite.errstr ?? ''} ${sqlite.message ?? ''}`
    : String(error);
  return sqlite?.errcode === 5 || /database is (?:locked|busy)/i.test(message);
}

function withHookDbRetry<T>(operation: () => T): T {
  const deadline = Date.now() + HOOK_DB_RETRY_TIMEOUT_MS;
  for (;;) {
    try {
      return operation();
    } catch (error) {
      if (!isHookDbBusy(error) || Date.now() >= deadline) throw error;
      Atomics.wait(HOOK_RUN_STATE_LOCK_WAIT, 0, 0, HOOK_RUN_STATE_LOCK_RETRY_MS);
    }
  }
}

function hookRunStateDir(): string {
  const stateDir = join(dirname(resolveDbPath(null)), 'hook-state', 'runs');
  mkdirSync(stateDir, { recursive: true });
  return stateDir;
}

function hookRunStateFile(key: string): string {
  return join(hookRunStateDir(), `${key}.json`);
}

function processIsAlive(pid: number): boolean {
  if (!Number.isSafeInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code !== 'ESRCH';
  }
}

function removeStaleHookRunStateLock(lockFile: string): boolean {
  try {
    const owner = Number.parseInt(readFileSync(lockFile, 'utf8'), 10);
    const staleByAge = Date.now() - statSync(lockFile).mtimeMs > HOOK_RUN_STATE_LOCK_STALE_MS;
    if (processIsAlive(owner) && !staleByAge) return false;
    unlinkSync(lockFile);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return true;
    return false;
  }
}

function withHookRunStateLock<T>(key: string, operation: () => T): T {
  const lockFile = `${hookRunStateFile(key)}.lock`;
  const deadline = Date.now() + HOOK_RUN_STATE_LOCK_TIMEOUT_MS;
  for (;;) {
    try {
      const fd = openSync(lockFile, 'wx', 0o600);
      try {
        writeFileSync(fd, `${process.pid}\n`, 'utf8');
      } finally {
        closeSync(fd);
      }
      try {
        return operation();
      } finally {
        try { unlinkSync(lockFile); } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
        }
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      if (removeStaleHookRunStateLock(lockFile)) continue;
      if (Date.now() >= deadline) {
        throw new Error(`timed out waiting for hook correlation state: ${lockFile}`);
      }
      Atomics.wait(HOOK_RUN_STATE_LOCK_WAIT, 0, 0, HOOK_RUN_STATE_LOCK_RETRY_MS);
    }
  }
}

function readHookRunEntries(key: string): HookRunStateEntry[] {
  try {
    const parsed = JSON.parse(readFileSync(hookRunStateFile(key), 'utf8')) as unknown;
    if (!Array.isArray(parsed)) return [];
    const cutoff = Date.now() - HOOK_RUN_STATE_TTL_MS;
    return parsed.filter((entry): entry is HookRunStateEntry => {
      if (!entry || typeof entry !== 'object') return false;
      const candidate = entry as Partial<HookRunStateEntry>;
      const createdAt = typeof candidate.createdAt === 'string' ? Date.parse(candidate.createdAt) : NaN;
      return typeof candidate.runId === 'string'
        && candidate.runId.length > 0
        && Array.isArray(candidate.files)
        && candidate.files.every((file) => typeof file === 'string' && file.length > 0)
        && Number.isFinite(createdAt)
        && createdAt >= cutoff;
    });
  } catch {
    return [];
  }
}

function writeHookRunEntries(key: string, entries: HookRunStateEntry[]): void {
  const file = hookRunStateFile(key);
  if (entries.length === 0) {
    try { unlinkSync(file); } catch { /* already absent */ }
    return;
  }
  const tempFile = `${file}.${process.pid}.${Date.now()}.tmp`;
  writeFileSync(tempFile, JSON.stringify(entries, null, 2) + '\n', 'utf8');
  renameSync(tempFile, file);
}

function hookEventId(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    payload.tool_use_id,
    payload.toolUseId,
    payload.tool_call_id,
    payload.toolCallId,
    payload.event_id,
    payload.eventId,
    payload.id,
    input.tool_use_id,
    input.toolUseId,
    input.tool_call_id,
    input.toolCallId,
    input.event_id,
    input.eventId,
    input.id,
  );
}

function hookRunKey(payload: Record<string, unknown>, files: string[], cwd: string): string {
  const explicitId = hookEventId(payload);
  const identity = {
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    artifact: artifact(payload),
    event: explicitId,
    files: explicitId ? [] : files.map(file => resolveHookPath(file, cwd)).sort(),
  };
  return createHash('sha1').update(JSON.stringify(identity)).digest('hex');
}

function recordHookRun(payload: Record<string, unknown>, files: string[], cwd: string, runId: string): void {
  const key = hookRunKey(payload, files, cwd);
  withHookRunStateLock(key, () => {
    const entries = readHookRunEntries(key);
    entries.push({
      runId,
      files: files.map(file => resolveHookPath(file, cwd)),
      createdAt: new Date().toISOString(),
    });
    writeHookRunEntries(key, entries.slice(-20));
  });
}

function consumeHookRun(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  files: string[],
  cwd: string,
): string | null {
  const key = hookRunKey(payload, files, cwd);
  return withHookRunStateLock(key, () => {
    const entries = readHookRunEntries(key);
    const activeEntries = entries.filter((entry) => {
      const activeFiles = new Set(listWork(database, {
        agentId: agentId(payload),
        workspacePath: cwd,
        artifact: artifact(payload),
        runId: entry.runId,
        activeOnly: true,
      }).files.map((file) => file.file_path));
      return entry.files.every((file) => activeFiles.has(file));
    });
    // Newest-first avoids a previously abandoned same-key event consuming the
    // post-edit for a later retry. Other live entries stay queued.
    const entry = activeEntries.pop() ?? null;
    writeHookRunEntries(key, activeEntries);
    return entry?.runId ?? null;
  });
}

function activeRunForFiles(
  database: DatabaseSync,
  params: {
    agentId: string;
    workspacePath: string;
    artifact: string | null;
    files: string[];
    origins: Array<'WORK' | 'HOOK'>;
  },
): string | null {
  const absFiles = params.files.map(file => resolveHookPath(file, params.workspacePath));
  if (absFiles.length === 0) return null;
  const rows = listWork(database, {
    agentId: params.agentId,
    workspacePath: params.workspacePath,
    artifact: params.artifact,
    activeOnly: true,
  }).files.filter((entry) => params.origins.includes(entry.origin as 'WORK' | 'HOOK'));
  const byRun = new Map<string, Set<string>>();
  for (const row of rows) {
    const paths = byRun.get(row.run_id) ?? new Set<string>();
    paths.add(row.file_path);
    byRun.set(row.run_id, paths);
  }
  const matches = [...byRun].filter(([, paths]) => absFiles.every(file => paths.has(file)));
  return matches.length === 1 ? matches[0]![0] : null;
}

function runOrigin(database: DatabaseSync, runId: string): 'TASK' | 'WORK' | 'HOOK' | null {
  const row = database.prepare('SELECT origin FROM task_runs WHERE run_id = ?').get(runId) as { origin: 'TASK' | 'WORK' | 'HOOK' } | undefined;
  return row?.origin ?? null;
}

function peerStateDir(): string {
  const stateDir = join(dirname(resolveDbPath(null)), 'hook-state', 'peers');
  mkdirSync(stateDir, { recursive: true });
  return stateDir;
}

function peerStateKey(payload: Record<string, unknown>, files: string[], cwd: string): string {
  return createHash('sha1').update(JSON.stringify({
    agent: agentId(payload),
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    artifact: artifact(payload),
    files: files.map(file => resolveHookPath(file, cwd)).sort(),
  })).digest('hex');
}

function peerFingerprint(peers: WorkPeer[]): string {
  return createHash('sha1').update(JSON.stringify(peers.map((peer) => ({
    agent: peer.agent_id,
    file: peer.file_path,
    task: peer.task_id,
    origin: peer.origin,
    rationale: peer.rationale,
    exclusive: peer.exclusive,
  })).sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b))))).digest('hex');
}

function peerLabel(peer: WorkPeer): string {
  const work = peer.task_id ?? peer.origin;
  const reason = peer.rationale.replace(/\s+/g, ' ').trim().slice(0, 40);
  return `${peer.agent_id}:${work}${reason ? `(${reason})` : ''}`;
}

function emitPeerDelta(
  payload: Record<string, unknown>,
  files: string[],
  cwd: string,
  allPeers: WorkPeer[],
): void {
  const targetSet = new Set(files.map(file => resolveHookPath(file, cwd)));
  const peers = allPeers.filter(peer => peer.agent_id !== agentId(payload) && targetSet.has(peer.file_path));
  const key = peerStateKey(payload, files, cwd);
  const stateFile = join(peerStateDir(), `${key}.txt`);
  const fingerprint = peerFingerprint(peers);
  let previous: string | null = null;
  try { previous = readFileSync(stateFile, 'utf8').trim(); } catch { /* first delivery */ }
  if (previous === fingerprint) return;
  writeFileSync(stateFile, fingerprint, 'utf8');
  if (peers.length === 0) return;

  const shown = peers.slice(0, 3).map(peerLabel).join('; ');
  const omitted = peers.length > 3 ? ` +${peers.length - 3}` : '';
  const canonicalWorkspace = canonicalizePath(cwd);
  const targets = files.slice(0, 2).map(file => relative(canonicalWorkspace, resolveHookPath(file, cwd)) || basename(file)).join(',');
  const message = `AWARE ${targets} | peers ${shown}${omitted}`;
  process.stdout.write(`${JSON.stringify({ additionalContext: message })}\n`);
}

function hookAgentContext(payload: Record<string, unknown>, hookName: string): string {
  const value =
    process.env.OCTOCODE_AGENT_CONTEXT
    ?? process.env.OCTOCODE_AGENT_HOST
    ?? payload.context
    ?? payload.host
    ?? payload.client
    ?? payload.source;
  return typeof value === 'string' && value.trim() ? value.trim() : hookName;
}

function registerHookAgent(database: DatabaseSync, payload: Record<string, unknown>, hookName: string): void {
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

function scopeArgs(payload: Record<string, unknown>): { workspacePath?: string; artifact?: string } {
  const ws = workspace(payload);
  const art = artifact(payload);
  return {
    ...(ws ? { workspacePath: ws } : {}),
    ...(art ? { artifact: art } : {}),
  };
}

async function runPreEdit(payload: Record<string, unknown>): Promise<number> {
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  const hookWorkspace = workspace(payload) ?? process.cwd();
  const guardReason = evaluateHarnessGuard({
    targetFiles: files,
    skillRoot: process.env.OCTOCODE_SKILL_ROOT,
    cwd: hookWorkspace,
  });
  if (guardReason) {
    console.error(`${guardReason} Edit blocked.`);
    return 2;
  }
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:pre-edit');
    const hookAgentId = agentId(payload);
    const hookArtifact = artifact(payload);
    const activeClaim = activeTaskClaimForAgent(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
    });
    const explicitRunId = activeClaim ? null : activeRunForFiles(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      files,
      origins: ['WORK'],
    });
    const result = explicitRunId
      ? { ok: true as const, ...touchWork(database, {
        agentId: hookAgentId,
        runId: explicitRunId,
        targetFiles: files,
        ttlMs: 10 * 60_000,
      }) }
      : startWork(database, {
      agentId: hookAgentId,
      sessionId: sessionId(payload),
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      runId: activeClaim?.run_id,
      rationale: autoClaimRationale(payload, files),
      testPlan: 'post-edit verification',
      targetFiles: files,
      origin: 'HOOK',
      source: 'HOOK',
      ttlMs: 10 * 60_000,
    });
    if (!result.ok) {
      const detail = result.conflicts.slice(0, 3)
        .map(conflict => `${relative(hookWorkspace, conflict.file_path)} (${conflict.agent_id})`)
        .join(', ');
      console.error(`octocode-awareness: exclusive file work blocks this edit${detail ? `: ${detail}` : ''}.`);
      return 2;
    }
    recordHookRun(payload, files, hookWorkspace, result.run.run_id);
    emitPeerDelta(payload, files, hookWorkspace, result.peers);
    return 0;
  } catch (error) {
    console.error(`octocode-awareness pre-flight warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
    return 0;
  }
}

async function runPostEdit(payload: Record<string, unknown>): Promise<number> {
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  const hookWorkspace = workspace(payload) ?? process.cwd();
  let consumedRunId: string | null = null;
  let stage = 'open database';
  try {
    const database = db();
    stage = 'register hook agent';
    withHookDbRetry(() => registerHookAgent(database, payload, 'hook:post-edit'));
    const hookAgentId = agentId(payload);
    const hookArtifact = artifact(payload);
    stage = 'consume correlation';
    consumedRunId = withHookDbRetry(() => consumeHookRun(database, payload, files, hookWorkspace));
    stage = 'resolve fallback run';
    const correlatedRunId = consumedRunId
      ?? withHookDbRetry(() => activeTaskClaimForAgent(database, {
        agentId: hookAgentId,
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
      }))?.run_id
      ?? withHookDbRetry(() => activeRunForFiles(database, {
        agentId: hookAgentId,
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
        files,
        origins: ['WORK', 'HOOK'],
      }));
    if (!correlatedRunId) {
      console.error('octocode-awareness post-edit warning (continuing): could not identify a unique work run; leaving presence for expiry.');
      return 0;
    }
    stage = 'read run origin';
    const origin = withHookDbRetry(() => runOrigin(database, correlatedRunId));
    stage = 'finish work lifecycle';
    if (origin === 'HOOK') {
      withHookDbRetry(() => endWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
      }));
    } else {
      withHookDbRetry(() => touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
        ttlMs: 10 * 60_000,
      }));
    }
    // The lifecycle mutation committed, so this correlation must not be
    // restored even if a later audit-log write fails.
    consumedRunId = null;
    stage = 'write edit log';
    for (const file of files) {
      withHookDbRetry(() => insertEditLog(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        filePath: resolveHookPath(file, hookWorkspace),
        operation: 'update',
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
      }));
    }
  } catch (error) {
    // Consuming the file-backed correlation and mutating SQLite cannot be one
    // atomic transaction. Restore it on a failed lifecycle mutation so a host
    // retry can finish the same run instead of leaving presence orphaned.
    if (consumedRunId) {
      try { recordHookRun(payload, files, hookWorkspace, consumedRunId); } catch { /* best effort */ }
    }
    console.error(`octocode-awareness post-edit warning during ${stage} (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}

async function runHarnessGuard(payload: Record<string, unknown>): Promise<number> {
  // Gate logic is shared with the Pi bridge (evaluateHarnessGuard in
  // src/pi-hooks.ts) so the shell hosts (claude/codex/cursor) and Pi can never
  // drift. OCTOCODE_SKILL_ROOT is exported by harness-guard.sh; a missing root
  // (unset) makes the guard a no-op, matching the Pi default when no skill root
  // is wired.
  const reason = evaluateHarnessGuard({
    targetFiles: extractFiles(payload),
    skillRoot: process.env.OCTOCODE_SKILL_ROOT,
    cwd: process.cwd(),
  });
  if (reason) {
    console.error(`${reason} Edit blocked.`);
    return 2;
  }
  return 0;
}

async function runStopVerify(payload: Record<string, unknown>): Promise<number> {
  if (process.env.OCTOCODE_NO_VERIFY_GATE === '1' || isStopHookActive(payload)) return 0;
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:stop-verify');
    const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
    if (report.count > 0) {
      const details = [
        ...report.unverified.map((run) => `${run.status}:${run.run_id}: ${run.test_plan}`),
        ...report.stale_active.map((run) => `STALE:${run.run_id}: ${run.rationale}`),
      ];
      const shown = details.slice(0, 3);
      const omitted = details.length > 3 ? `; +${details.length - 3} omitted` : '';
      console.error(`octocode-awareness: concluding with unverified work. ${shown.join('; ')}${omitted}`);
      return 2;
    }
  } catch (error) {
    console.error(`octocode-awareness verify warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}

function maybePreviewDigest(payload: Record<string, unknown>): string | null {
  if (process.env.OCTOCODE_NO_DIGEST === '1') return null;
  if (process.env.OCTOCODE_NOTIFY_RUN_DIGEST !== '1') return null;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 3600_000 : 4 * 3600_000;
  const memoryHome = dirname(resolveDbPath(null));
  const digestScope = workspace(payload) ?? 'global';
  const scopeHash = createHash('sha256').update(digestScope).digest('hex').slice(0, 12);
  const markerPath = join(memoryHome, `.last-digest-preview-${scopeHash}-epoch-ms`);
  try {
    const database = db();
    let last = 0;
    try {
      last = Number(readFileSync(markerPath, 'utf8').trim() || 0);
    } catch {
      last = 0;
    }
    const now = Date.now();
    if (!last || now - last >= intervalMs) {
      const preview = digest(database, {
        workspace: workspace(payload),
        memoryHome,
        dry_run: true,
      });
      mkdirSync(memoryHome, { recursive: true });
      writeFileSync(markerPath, String(now), 'utf8');
      const pressure = {
        archive: preview.would_archive ?? 0,
        memories: preview.would_prune_old ?? 0,
        locks: preview.would_prune_locks ?? 0,
        refinements: preview.would_prune_refinements ?? 0,
      };
      if (Object.values(pressure).some((count) => count > 0)) {
        return `Maintenance pressure: archive ${pressure.archive}, prune memories ${pressure.memories}, locks ${pressure.locks}, refinements ${pressure.refinements}. Review with octocode-awareness maintenance digest --dry-run --workspace "$PWD" --compact; apply only after review.`;
      }
    }
  } catch (error) {
    console.error(`octocode-awareness digest warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return null;
}

async function runNotifyDeliver(payload: Record<string, unknown>): Promise<number> {
  if (process.env.OCTOCODE_NO_NOTIFY === '1') return 0;
  const maintenanceContext = maybePreviewDigest(payload);
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:notify-deliver');
    const result = notifyGet(database, {
      agent_id: agentId(payload),
      workspace: workspace(payload) ?? undefined,
      artifact: artifact(payload) ?? undefined,
      format: 'hook',
    }) as { additionalContext?: string };
    const additionalContext = [result.additionalContext, maintenanceContext].filter(Boolean).join('\n');
    if (additionalContext) {
      process.stdout.write(JSON.stringify({
        additionalContext,
      }) + '\n');
    }
  } catch (error) {
    console.error(`octocode-awareness session-capture warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}

async function runSessionEnd(payload: Record<string, unknown>): Promise<number> {
  if (process.env.OCTOCODE_NO_SESSION_CAPTURE === '1' || hookReason(payload) === 'clear') return 0;
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:session-end');
    sessionCapture(database, {
      agent_id: agentId(payload),
      workspace: workspace(payload) ?? undefined,
      artifact: artifact(payload) ?? undefined,
      reason: hookReason(payload) || undefined,
    });
    // Mark the session ended so its still-held locks read as abandoned
    // (holder_session_active:false) to any agent that later conflicts on them.
    const sid = sessionId(payload);
    if (sid) endSession(database, { sessionId: sid });
  } catch {
    // fail-open
  }
  return 0;
}

export async function runHookCommand(command: string, rawPayload?: string): Promise<number> {
  if (command === 'help' || command === '--help' || command === '-h') {
    process.stdout.write('usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json\n');
    return 0;
  }

  const payload = parsePayload(rawPayload ?? await readStdin());
  switch (command) {
    case 'pre-edit': return runPreEdit(payload);
    case 'post-edit': return runPostEdit(payload);
    case 'harness-guard': return runHarnessGuard(payload);
    case 'stop-verify': return runStopVerify(payload);
    case 'notify-deliver': return runNotifyDeliver(payload);
    case 'session-end': return runSessionEnd(payload);
    default:
      console.error(`unknown hook command: ${command}`);
      return 1;
  }
}

async function main(): Promise<number> {
  return runHookCommand(process.argv[2] ?? 'help');
}

const isMain = process.argv[1]
  ? fileURLToPath(import.meta.url) === resolve(process.argv[1])
  : false;
const invokedAsHookRunner = process.argv[1]
  ? /^hook-runner\.(js|mjs|ts)$/.test(basename(process.argv[1]))
  : false;

if (isMain && invokedAsHookRunner) {
  process.exitCode = await main();
}
