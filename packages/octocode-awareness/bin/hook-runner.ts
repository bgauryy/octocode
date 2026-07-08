/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, locking,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';
import { registerAgent } from '../src/agents.js';
import { insertEditLog } from '../src/audit.js';
import { connectDb, resolveDbPath } from '../src/db.js';
import { canonicalizePath, normalizeWorkspacePath } from '../src/git.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { auditUnverified } from '../src/verify.js';
import { digest, notifyGet, sessionCapture } from '../src/maintenance.js';
import { extractPiWriteTargetPaths } from '../src/pi-hooks.js';

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
    payload.session_id,
    payload.sessionId,
    payload.agent_id,
    payload.agentId,
    input.session_id,
    input.sessionId,
    input.agent_id,
    input.agentId,
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
    console.error(`octocode-awareness: OCTOCODE_AGENT_ID or host session id missing; using fallback agent id "${fallback}". Set OCTOCODE_AGENT_ID for reliable multi-agent lock isolation.`);
  }
  return fallback;
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
  return resolve(cwd, file);
}

function isInsidePath(candidate: string, root: string): boolean {
  // Shared with the workspace-scope resolver (src/git.ts) so containment
  // checks and scope keys always agree on symlinked paths (e.g. macOS
  // /tmp -> /private/tmp) instead of maintaining two divergent copies.
  const resolvedRoot = canonicalizePath(root);
  const resolvedCandidate = canonicalizePath(candidate);
  if (resolvedCandidate === resolvedRoot) return true;
  // A real path is inside root iff its relative path neither escapes upward
  // (`..`) nor is absolute (different drive/root) — string prefixes are unsafe
  // because `/a/b-sibling` textually starts with `/a/b`.
  const rel = relative(resolvedRoot, resolvedCandidate);
  return rel !== '' && !rel.startsWith('..') && !isAbsolute(rel);
}

function db() {
  return connectDb(resolveDbPath(null));
}

interface HookTaskStateEntry {
  taskId: string;
  files: string[];
  createdAt: string;
}

type HookTaskState = Record<string, HookTaskStateEntry[]>;

function hookTaskStateFile(): string {
  const stateDir = join(dirname(resolveDbPath(null)), 'hook-state');
  mkdirSync(stateDir, { recursive: true });
  return join(stateDir, 'shell-hook-tasks.json');
}

function readHookTaskState(): HookTaskState {
  try {
    return JSON.parse(readFileSync(hookTaskStateFile(), 'utf8')) as HookTaskState;
  } catch {
    return {};
  }
}

function writeHookTaskState(state: HookTaskState): void {
  writeFileSync(hookTaskStateFile(), JSON.stringify(state, null, 2) + '\n', 'utf8');
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

function hookTaskKey(payload: Record<string, unknown>, files: string[], cwd: string): string {
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

function recordHookTask(payload: Record<string, unknown>, files: string[], cwd: string, taskId: string): void {
  const state = readHookTaskState();
  const key = hookTaskKey(payload, files, cwd);
  const entries = state[key] ?? [];
  entries.push({
    taskId,
    files: files.map(file => resolveHookPath(file, cwd)),
    createdAt: new Date().toISOString(),
  });
  state[key] = entries.slice(-20);
  writeHookTaskState(state);
}

function consumeHookTask(payload: Record<string, unknown>, files: string[], cwd: string): string | null {
  const state = readHookTaskState();
  const key = hookTaskKey(payload, files, cwd);
  const entries = state[key] ?? [];
  const entry = entries.shift();
  if (entries.length > 0) state[key] = entries;
  else delete state[key];
  writeHookTaskState(state);
  return entry?.taskId ?? null;
}

function uniqueActiveHookTaskId(
  database: DatabaseSync,
  params: { agentId: string; workspacePath: string; artifact: string | null; files: string[] },
): string | null {
  const absFiles = params.files.map(file => resolveHookPath(file, params.workspacePath));
  if (absFiles.length === 0) return null;
  const where = [
    'fl.agent_id = ?',
    "ai.status = 'ACTIVE'",
    `fl.file_path IN (${absFiles.map(() => '?').join(',')})`,
    'ai.workspace_path = ?',
  ];
  const binds: (string | number)[] = [
    params.agentId,
    ...absFiles,
    normalizeWorkspacePath(params.workspacePath, params.workspacePath) ?? resolve(params.workspacePath),
  ];
  if (params.artifact) {
    where.push('(ai.artifact = ? OR ai.artifact IS NULL)');
    binds.push(params.artifact);
  }
  const rows = database.prepare(
    `SELECT DISTINCT fl.task_id
       FROM locks fl
       JOIN tasks ai ON ai.task_id = fl.task_id
      WHERE ${where.join(' AND ')}
      ORDER BY fl.task_id ASC`
  ).all(...binds) as Array<{ task_id: string }>;
  return rows.length === 1 ? rows[0]!.task_id : null;
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
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:pre-edit');
    const result = preFlightIntent(database, {
      agentId: agentId(payload),
      workspacePath: workspace(payload) ?? process.cwd(),
      artifact: artifact(payload),
      rationale: 'auto: file edit via lifecycle hook',
      testPlan: 'post-edit verification',
      targetFiles: files,
      ttlMs: 10 * 60_000,
    });
    if (!result.ok) {
      console.error('octocode-awareness: target file is locked by another agent — edit blocked.');
      console.error(JSON.stringify(result));
      return 2;
    }
    recordHookTask(payload, files, workspace(payload) ?? process.cwd(), result.task.task_id);
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
    const correlatedTaskId = consumeHookTask(payload, files, hookWorkspace)
      ?? uniqueActiveHookTaskId(database, {
        agentId: hookAgentId,
        workspacePath: hookWorkspace,
        artifact: hookArtifact,
        files,
      });
    if (!correlatedTaskId) {
      console.error('octocode-awareness post-edit warning (continuing): could not identify a unique hook task to release; leaving locks for verify/cleanup.');
      return 0;
    }
    const release = releaseFileLock(database, {
      agentId: hookAgentId,
      workspacePath: hookWorkspace,
      artifact: hookArtifact,
      taskId: correlatedTaskId,
      status: 'PENDING',
    });
    const taskId = release.task_ids.length === 1 ? release.task_ids[0] : correlatedTaskId;
    for (const file of files) {
      insertEditLog(database, {
        agentId: hookAgentId,
        taskId,
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
  const skillRoot = process.env.OCTOCODE_SKILL_ROOT;
  if (!skillRoot) return 0;
  const files = extractFiles(payload);
  if (files.length === 0) return 0;
  const insideSkill = files.some((file) => isInsidePath(resolveHookPath(file), skillRoot));
  if (!insideSkill) return 0;

  if (process.env.OCTOCODE_ALLOW_HARNESS_APPLY !== '1') {
    console.error('octocode-awareness: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1. Edit blocked.');
    return 2;
  }

  // "Dedicated branch" is checked against the skill root's actual git branch.
  // main/master is never allowed (self-harness.md Hard NO); a detached HEAD or
  // non-repo needs the explicit OCTOCODE_HARNESS_BRANCH_OK=1 acknowledgement.
  const branch = gitBranchOf(skillRoot);
  if (branch === 'main' || branch === 'master') {
    console.error(`octocode-awareness: harness self-fix is never allowed on ${branch}. Create a dedicated branch first. Edit blocked.`);
    return 2;
  }
  if (!branch || branch === 'HEAD') {
    if (process.env.OCTOCODE_HARNESS_BRANCH_OK !== '1') {
      console.error('octocode-awareness: cannot confirm a dedicated git branch for the skill. Create one, or set OCTOCODE_HARNESS_BRANCH_OK=1 to acknowledge. Edit blocked.');
      return 2;
    }
  }

  return 0;
}

function gitBranchOf(dir: string): string | null {
  try {
    const r = spawnSync('git', ['-C', dir, 'rev-parse', '--abbrev-ref', 'HEAD'], {
      encoding: 'utf8', timeout: 5000,
    });
    return r.status === 0 ? String(r.stdout).trim() : null;
  } catch {
    return null;
  }
}

async function runStopVerify(payload: Record<string, unknown>): Promise<number> {
  if (process.env.OCTOCODE_NO_VERIFY_GATE === '1' || isStopHookActive(payload)) return 0;
  try {
    const database = db();
    registerHookAgent(database, payload, 'hook:stop-verify');
    const report = auditUnverified(database, { agentId: agentId(payload), ...scopeArgs(payload) });
    if (report.count > 0) {
      const parts: string[] = [];
      if (report.unverified.length > 0) {
        parts.push(report.unverified.map((u) => `${u.status}:${u.task_id}: ${u.test_plan}`).join('; '));
      }
      if (report.stale_active.length > 0) {
        parts.push('Stale active (lock expired): ' + report.stale_active.map((s) => `${s.task_id}: ${s.rationale}`).join('; '));
      }
      console.error(`octocode-awareness: concluding with unverified work. ${parts.join(' | ')}`);
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
        additional_context: result.additionalContext,
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
