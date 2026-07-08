/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, locking,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { registerAgent } from '../src/agents.js';
import { connectDb, resolveDbPath } from '../src/db.js';
import { preFlightIntent, releaseFileLock } from '../src/intents.js';
import { auditUnverified } from '../src/verify.js';
import { digest, notifyGet, sessionCapture } from '../src/maintenance.js';
import { extractPiWriteTargetPaths } from '../src/pi-hooks.js';

const command = process.argv[2] ?? 'help';

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

function agentId(payload: Record<string, unknown>): string {
  return process.env.OCTOCODE_AGENT_ID
    || String(payload.session_id ?? payload.sessionId ?? payload.agent_id ?? payload.agentId ?? 'claude-agent');
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
  return extractPiWriteTargetPaths(toolName, input);
}

function resolveHookPath(file: string, cwd = process.cwd()): string {
  // Absolutize AND normalize: `..`/`.` segments and non-absolute inputs (Codex
  // apply_patch and Cursor payloads often carry repo-relative paths) must be
  // collapsed before any containment check, or a traversal path that actually
  // resolves inside the skill root can slip past a textual prefix comparison.
  return resolve(cwd, file);
}

// Canonicalize a path for containment comparison: resolve symlinks on the
// longest existing prefix and keep the (possibly not-yet-created) tail. Without
// this, a skill root under a symlinked dir (e.g. macOS /tmp -> /private/tmp) and
// a cwd-derived candidate that Node has already realpath'd can land on different
// symlink forms and slip past the check.
function canonicalize(input: string): string {
  let dir = resolve(input);
  const tail: string[] = [];
  // Walk up until an existing ancestor is found, then realpath it and rejoin.
  for (let guard = 0; guard < 4096; guard += 1) {
    try {
      return tail.length ? join(realpathSync(dir), ...tail) : realpathSync(dir);
    } catch {
      const parent = dirname(dir);
      if (parent === dir) return resolve(input); // reached filesystem root
      tail.unshift(basename(dir));
      dir = parent;
    }
  }
  return resolve(input);
}

function isInsidePath(candidate: string, root: string): boolean {
  const resolvedRoot = canonicalize(root);
  const resolvedCandidate = canonicalize(candidate);
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
    releaseFileLock(database, {
      agentId: agentId(payload),
      workspacePath: workspace(payload) ?? undefined,
      artifact: artifact(payload),
      targetFiles: files,
      status: 'PENDING',
    });
  } catch {
    // fail-open
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
  } catch {
    // fail-open
  }
  return 0;
}

function maybeRunDigest(payload: Record<string, unknown>): void {
  if (process.env.OCTOCODE_NO_DIGEST === '1') return;
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
  } catch {
    // fail-open
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
  } catch {
    // fail-open
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

async function main(): Promise<number> {
  if (command === 'help' || command === '--help' || command === '-h') {
    process.stdout.write('usage: hook-runner <pre-edit|post-edit|harness-guard|stop-verify|notify-deliver|session-end> < hook-payload.json\n');
    return 0;
  }

  const payload = parsePayload(await readStdin());
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

process.exitCode = await main();
