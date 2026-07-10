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
import { normalizeArtifact } from '../src/helpers.js';
import { activeTaskClaimForAgent } from '../src/tasks.js';
import { endWork, listWork, startWork, touchWork } from '../src/work.js';
import type { WorkPeer } from '../src/types.js';
import { auditUnverified } from '../src/verify.js';
import { digest, notifyGet, sessionCapture } from '../src/maintenance.js';
import { endSession } from '../src/sessions.js';
import { evaluateHarnessGuard, extractPiWriteTargetPaths } from '../src/pi-hooks.js';

export type ShellHookHost = 'claude' | 'codex' | 'cursor';

export interface HookRunOptions {
  host?: ShellHookHost;
  skillRoot?: string;
}

export interface HookControlOutcome {
  exitCode: number;
  payload?: Record<string, unknown>;
  stderr?: string;
}

const INTERNAL_HOOK_HOST = '__octocode_hook_host';
const INTERNAL_SKILL_ROOT = '__octocode_skill_root';

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

function normalizeShellHookHost(value: unknown): ShellHookHost | null {
  if (typeof value !== 'string') return null;
  const normalized = value.trim().toLowerCase();
  return normalized === 'claude' || normalized === 'codex' || normalized === 'cursor'
    ? normalized
    : null;
}

function shellHookHost(payload: Record<string, unknown>): ShellHookHost {
  const explicit = normalizeShellHookHost(
    payload[INTERNAL_HOOK_HOST]
      ?? process.env.OCTOCODE_AGENT_HOST
      ?? payload.host
      ?? payload.client,
  );
  if (explicit) return explicit;
  const eventName = firstString(payload.hook_event_name, payload.eventName) ?? '';
  if (eventName && eventName[0] === eventName[0]?.toLowerCase()) return 'cursor';
  return 'claude';
}

function hookSkillRoot(payload: Record<string, unknown>): string | null {
  return firstString(payload[INTERNAL_SKILL_ROOT], process.env.OCTOCODE_SKILL_ROOT);
}

export function hookContextEnvelope(
  host: ShellHookHost,
  eventName: string,
  message: string,
): Record<string, unknown> {
  if (host === 'cursor') {
    if (eventName === 'sessionStart') return { additional_context: message };
    return { permission: 'allow', agent_message: message };
  }
  return {
    hookSpecificOutput: {
      hookEventName: eventName,
      additionalContext: message,
    },
  };
}

export function hookBlockOutcome(
  host: ShellHookHost,
  phase: 'pre-edit' | 'stop',
  message: string,
): HookControlOutcome {
  if (host !== 'cursor') return { exitCode: 2, stderr: message };
  if (phase === 'stop') {
    return { exitCode: 0, payload: { followup_message: message } };
  }
  return {
    exitCode: 0,
    payload: {
      permission: 'deny',
      user_message: message,
      agent_message: message,
    },
  };
}

function writeHookPayload(payload: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

function emitHookContext(payload: Record<string, unknown>, eventName: string, message: string): void {
  writeHookPayload(hookContextEnvelope(shellHookHost(payload), eventName, message));
}

function completeHookControl(outcome: HookControlOutcome): number {
  if (outcome.payload) writeHookPayload(outcome.payload);
  if (outcome.stderr) console.error(outcome.stderr);
  return outcome.exitCode;
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
    payload[INTERNAL_HOOK_HOST],
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

function promptQuery(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  const prompt = firstString(
    payload.prompt,
    payload.user_prompt,
    payload.userPrompt,
    payload.text,
    payload.message,
    typeof payload.input === 'string' ? payload.input : null,
    input.prompt,
    input.user_prompt,
    input.userPrompt,
    input.text,
    input.message,
  );
  return prompt ? prompt.slice(0, 4_000) : null;
}

function hookSessionCorrelation(payload: Record<string, unknown>): string | null {
  const input = objectOrEmpty(payloadInput(payload));
  return firstString(
    sessionId(payload),
    payload.transcript_path,
    payload.transcriptPath,
    payload.conversation_id,
    payload.conversationId,
    payload.thread_id,
    payload.threadId,
    input.transcript_path,
    input.transcriptPath,
    input.conversation_id,
    input.conversationId,
    input.thread_id,
    input.threadId,
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

function fallbackVerificationPlan(files: string[], cwd: string): string {
  const canonicalWorkspace = canonicalizePath(cwd);
  const normalized = [...new Set(files.map(file => resolveHookPath(file, cwd)))];
  const shown = normalized.slice(0, 3)
    .map(file => relative(canonicalWorkspace, file) || basename(file))
    .join(', ');
  const omitted = normalized.length > 3 ? ` (+${normalized.length - 3} more)` : '';
  return `Verify ${shown || 'the edited files'}${omitted}: run the smallest relevant test/typecheck and inspect the diff; record the check and result.`;
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

const HOOK_AGGREGATE_CONTEXT_PREFIX = 'hook-scope:';

function hookAggregateContextRef(payload: Record<string, unknown>, cwd: string): string | null {
  const sessionCorrelation = hookSessionCorrelation(payload);
  if (!sessionCorrelation) return null;
  const identity = {
    agent: agentId(payload),
    session: sessionCorrelation,
    workspace: normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    artifact: normalizeArtifact(artifact(payload)),
  };
  return `${HOOK_AGGREGATE_CONTEXT_PREFIX}${createHash('sha1').update(JSON.stringify(identity)).digest('hex')}`;
}

function activeFallbackHookRun(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  cwd: string,
): string | null {
  const contextRef = hookAggregateContextRef(payload, cwd);
  if (!contextRef) return null;
  const row = database.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY updated_at DESC, created_at DESC LIMIT 1`).get(
    agentId(payload),
    normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    normalizeArtifact(artifact(payload)),
    contextRef,
  ) as { run_id: string } | undefined;
  return row?.run_id ?? null;
}

function hookAggregateLockKey(payload: Record<string, unknown>, cwd: string): string | null {
  const contextRef = hookAggregateContextRef(payload, cwd);
  return contextRef
    ? `aggregate-${createHash('sha1').update(contextRef).digest('hex')}`
    : null;
}

function startOrAttachFallbackHookRun(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  cwd: string,
  files: string[],
) {
  const contextRef = hookAggregateContextRef(payload, cwd);
  const startOrAttach = () => {
    const existingRunId = activeFallbackHookRun(database, payload, cwd);
    const result = startWork(database, {
      agentId: agentId(payload),
      sessionId: sessionId(payload),
      workspacePath: cwd,
      artifact: artifact(payload),
      runId: existingRunId ?? undefined,
      rationale: autoClaimRationale(payload, files),
      testPlan: fallbackVerificationPlan(files, cwd),
      contextRef: contextRef ?? undefined,
      targetFiles: files,
      origin: 'HOOK',
      source: 'HOOK',
      ttlMs: 10 * 60_000,
    });
    if (result.ok && existingRunId) {
      touchWork(database, {
        agentId: agentId(payload),
        runId: existingRunId,
        ttlMs: 10 * 60_000,
      });
    }
    return result;
  };
  const lockKey = hookAggregateLockKey(payload, cwd);
  return lockKey ? withHookRunStateLock(lockKey, startOrAttach) : startOrAttach();
}

function refreshFallbackVerificationPlan(
  database: DatabaseSync,
  runId: string,
  cwd: string,
): void {
  if (!isAggregatedFallbackHookRun(database, runId)) return;
  const files = database.prepare('SELECT file_path FROM run_files WHERE run_id = ? ORDER BY file_path')
    .all(runId) as unknown as Array<{ file_path: string }>;
  database.prepare("UPDATE task_runs SET test_plan = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE run_id = ? AND origin = 'HOOK'")
    .run(fallbackVerificationPlan(files.map(file => file.file_path), cwd), runId);
}

function isAggregatedFallbackHookRun(database: DatabaseSync, runId: string): boolean {
  const row = database.prepare(`SELECT origin, context_ref FROM task_runs WHERE run_id = ?`).get(runId) as {
    origin: string;
    context_ref: string | null;
  } | undefined;
  return row?.origin === 'HOOK' && row.context_ref?.startsWith(HOOK_AGGREGATE_CONTEXT_PREFIX) === true;
}

function finalizeActiveFallbackHookRuns(
  database: DatabaseSync,
  payload: Record<string, unknown>,
  cwd: string,
): string[] {
  const contextRef = hookAggregateContextRef(payload, cwd);
  if (!contextRef) return [];
  const rows = database.prepare(`SELECT run_id FROM task_runs
    WHERE origin = 'HOOK' AND status = 'ACTIVE' AND agent_id = ?
      AND workspace_path = ? AND artifact IS ? AND context_ref = ?
    ORDER BY created_at`).all(
    agentId(payload),
    normalizeWorkspacePath(cwd, cwd) ?? resolve(cwd),
    normalizeArtifact(artifact(payload)),
    contextRef,
  ) as unknown as Array<{ run_id: string }>;
  const finalized: string[] = [];
  for (const row of rows) {
    endWork(database, { agentId: agentId(payload), runId: row.run_id });
    finalized.push(row.run_id);
  }
  return finalized;
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

function hookAgentContext(payload: Record<string, unknown>, hookName: string): string {
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
    skillRoot: hookSkillRoot(payload),
    cwd: hookWorkspace,
  });
  if (guardReason) {
    return completeHookControl(hookBlockOutcome(
      shellHookHost(payload),
      'pre-edit',
      `${guardReason} Edit blocked.`,
    ));
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
      : activeClaim
        ? startWork(database, {
          agentId: hookAgentId,
          workspacePath: hookWorkspace,
          artifact: hookArtifact,
          runId: activeClaim.run_id,
          targetFiles: files,
          origin: 'HOOK',
          source: 'HOOK',
          ttlMs: 10 * 60_000,
        })
        : startOrAttachFallbackHookRun(database, payload, hookWorkspace, files);
    if (!result.ok) {
      const detail = result.conflicts.slice(0, 3)
        .map(conflict => `${relative(hookWorkspace, conflict.file_path)} (${conflict.agent_id})`)
        .join(', ');
      return completeHookControl(hookBlockOutcome(
        shellHookHost(payload),
        'pre-edit',
        `octocode-awareness: exclusive file work blocks this edit${detail ? `: ${detail}` : ''}.`,
      ));
    }
    withHookDbRetry(() => refreshFallbackVerificationPlan(database, result.run.run_id, hookWorkspace));
    recordHookRun(payload, files, hookWorkspace, result.run.run_id);
    const peerContext = emitPeerDelta(payload, files, hookWorkspace, result.peers);
    if (peerContext) {
      emitHookContext(
        payload,
        shellHookHost(payload) === 'cursor' ? 'preToolUse' : 'PreToolUse',
        peerContext,
      );
    }
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
    if (origin === 'HOOK' && isAggregatedFallbackHookRun(database, correlatedRunId)) {
      withHookDbRetry(() => touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        ttlMs: 10 * 60_000,
      }));
    } else if (origin === 'HOOK') {
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
    skillRoot: hookSkillRoot(payload),
    cwd: process.cwd(),
  });
  if (reason) {
    return completeHookControl(hookBlockOutcome(
      shellHookHost(payload),
      'pre-edit',
      `${reason} Edit blocked.`,
    ));
  }
  return 0;
}

async function runStopVerify(payload: Record<string, unknown>): Promise<number> {
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:stop-verify');
    const finalizedRunIds = withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd(),
    ));
    if (process.env.OCTOCODE_NO_VERIFY_GATE === '1') return 0;
    const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
    if (report.count > 0) {
      // A recursive Stop with no newly finalized work already surfaced this
      // unchanged debt. Allow it to conclude to avoid an infinite host loop.
      // New continuation edits create/finalize a new aggregate and must surface
      // one fresh continuation before the following unchanged recursive Stop.
      if (isStopHookActive(payload) && finalizedRunIds.length === 0) return 0;
      const details = [
        ...report.unverified.map((run) => `${run.status}:${run.run_id}: ${run.test_plan}`),
        ...report.stale_active.map((run) => `STALE:${run.run_id}: ${run.rationale}`),
      ];
      const shown = details.slice(0, 3);
      const omitted = details.length > 3 ? `; +${details.length - 3} omitted` : '';
      return completeHookControl(hookBlockOutcome(
        shellHookHost(payload),
        'stop',
        `octocode-awareness: concluding with unverified work. ${shown.join('; ')}${omitted}`,
      ));
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
    withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd(),
    ));
    const result = notifyGet(database, {
      agent_id: agentId(payload),
      session_id: hookSessionCorrelation(payload) ?? undefined,
      workspace: workspace(payload) ?? undefined,
      artifact: artifact(payload) ?? undefined,
      query: promptQuery(payload) ?? undefined,
      format: 'hook',
    }) as { additionalContext?: string };
    const additionalContext = [result.additionalContext, maintenanceContext].filter(Boolean).join('\n');
    if (additionalContext) {
      emitHookContext(
        payload,
        shellHookHost(payload) === 'cursor' ? 'sessionStart' : 'UserPromptSubmit',
        additionalContext,
      );
    }
  } catch (error) {
    console.error(`octocode-awareness session-capture warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
  return 0;
}

async function runSessionEnd(payload: Record<string, unknown>): Promise<number> {
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:session-end');
    withHookDbRetry(() => finalizeActiveFallbackHookRuns(
      database,
      payload,
      workspace(payload) ?? process.cwd(),
    ));
    if (process.env.OCTOCODE_NO_SESSION_CAPTURE !== '1' && hookReason(payload) !== 'clear') {
      sessionCapture(database, {
        agent_id: agentId(payload),
        workspace: workspace(payload) ?? undefined,
        artifact: artifact(payload) ?? undefined,
        reason: hookReason(payload) || undefined,
      });
    }
    // Mark the session ended so its still-held locks read as abandoned
    // (holder_session_active:false) to any agent that later conflicts on them.
    const sid = sessionId(payload);
    if (sid) endSession(database, {
      sessionId: sid,
      agentId: agentId(payload),
      workspacePath: workspace(payload) ?? process.cwd(),
      artifact: artifact(payload),
    });
  } catch {
    // fail-open
  }
  return 0;
}

export async function runHookCommand(
  command: string,
  rawPayload?: string,
  options: HookRunOptions = {},
): Promise<number> {
  if (command === 'help' || command === '--help' || command === '-h') {
    process.stdout.write('usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json\n');
    return 0;
  }

  const payload = {
    ...parsePayload(rawPayload ?? await readStdin()),
    ...(options.host ? { [INTERNAL_HOOK_HOST]: options.host } : {}),
    ...(options.skillRoot ? { [INTERNAL_SKILL_ROOT]: options.skillRoot } : {}),
  };
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
  const hostIndex = process.argv.indexOf('--host');
  const rawHost = hostIndex >= 0 ? process.argv[hostIndex + 1] : undefined;
  const host = normalizeShellHookHost(rawHost);
  if (rawHost && !host) {
    console.error(`unknown hook host: ${rawHost}`);
    return 1;
  }
  const skillRootIndex = process.argv.indexOf('--skill-root');
  const skillRoot = skillRootIndex >= 0 ? process.argv[skillRootIndex + 1] : undefined;
  return runHookCommand(process.argv[2] ?? 'help', undefined, {
    ...(host ? { host } : {}),
    ...(skillRoot ? { skillRoot } : {}),
  });
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
