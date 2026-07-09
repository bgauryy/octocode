/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, file presence,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
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

function hookRunStateDir(): string {
  const stateDir = join(dirname(resolveDbPath(null)), 'hook-state', 'runs');
  mkdirSync(stateDir, { recursive: true });
  return stateDir;
}

function hookRunStateFile(key: string): string {
  return join(hookRunStateDir(), `${key}.json`);
}

function readHookRunEntries(key: string): HookRunStateEntry[] {
  try {
    const parsed = JSON.parse(readFileSync(hookRunStateFile(key), 'utf8')) as unknown;
    return Array.isArray(parsed) ? parsed as HookRunStateEntry[] : [];
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
  const entries = readHookRunEntries(key);
  entries.push({
    runId,
    files: files.map(file => resolveHookPath(file, cwd)),
    createdAt: new Date().toISOString(),
  });
  writeHookRunEntries(key, entries.slice(-20));
}

function consumeHookRun(payload: Record<string, unknown>, files: string[], cwd: string): string | null {
  const key = hookRunKey(payload, files, cwd);
  const entries = readHookRunEntries(key);
  const entry = entries.shift();
  writeHookRunEntries(key, entries);
  return entry?.runId ?? null;
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
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:post-edit');
    const hookAgentId = agentId(payload);
    const hookWorkspace = workspace(payload) ?? process.cwd();
    const hookArtifact = artifact(payload);
    const correlatedRunId = consumeHookRun(payload, files, hookWorkspace)
      ?? activeTaskClaimForAgent(database, {
        agentId: hookAgentId,
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
      })?.run_id
      ?? activeRunForFiles(database, {
        agentId: hookAgentId,
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
        files,
        origins: ['WORK', 'HOOK'],
      });
    if (!correlatedRunId) {
      console.error('octocode-awareness post-edit warning (continuing): could not identify a unique work run; leaving presence for expiry.');
      return 0;
    }
    const origin = runOrigin(database, correlatedRunId);
    if (origin === 'HOOK') {
      endWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
      });
    } else {
      touchWork(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        targetFiles: files,
        ttlMs: 10 * 60_000,
      });
    }
    for (const file of files) {
      insertEditLog(database, {
        agentId: hookAgentId,
        runId: correlatedRunId,
        filePath: resolveHookPath(file, hookWorkspace),
        operation: 'update',
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
      });
    }
  } catch (error) {
    console.error(`octocode-awareness post-edit warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
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

function maybeRunDigest(payload: Record<string, unknown>): void {
  if (process.env.OCTOCODE_NO_DIGEST === '1') return;
  if (process.env.OCTOCODE_NOTIFY_RUN_DIGEST !== '1') return;
  const intervalHours = Number(process.env.OCTOCODE_DIGEST_INTERVAL_HOURS ?? 4);
  const intervalMs = Number.isFinite(intervalHours) && intervalHours > 0 ? intervalHours * 3600_000 : 4 * 3600_000;
  const memoryHome = process.env.OCTOCODE_MEMORY_HOME || `${process.env.HOME ?? ''}/.octocode/memory`;
  const markerPath = join(memoryHome, '.last-digest-epoch-ms');
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
      mkdirSync(memoryHome, { recursive: true });
      writeFileSync(markerPath, String(now), 'utf8');
      digest(database, { workspace: workspace(payload), memoryHome });
    }
  } catch (error) {
    console.error(`octocode-awareness digest warning (continuing): ${error instanceof Error ? error.message : String(error)}`);
  }
}

async function runNotifyDeliver(payload: Record<string, unknown>): Promise<number> {
  if (process.env.OCTOCODE_NO_NOTIFY === '1') return 0;
  maybeRunDigest(payload);
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:notify-deliver');
    const result = notifyGet(database, {
      agent_id: agentId(payload),
      workspace: workspace(payload) ?? undefined,
      artifact: artifact(payload) ?? undefined,
      format: 'hook',
    }) as { additionalContext?: string };
    if (result.additionalContext) {
      process.stdout.write(JSON.stringify({
        additionalContext: result.additionalContext,
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
