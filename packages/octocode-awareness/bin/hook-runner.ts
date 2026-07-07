/**
 * hook-runner.ts — shared implementation for octocode-awareness lifecycle hooks.
 *
 * Shell hook files are intentionally thin wrappers. All parsing, locking,
 * verification, briefing, and session-capture logic lives here so Claude/Codex
 * skill hooks and Pi native adapters share the same package-owned behavior.
 */
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
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

function agentId(payload: Record<string, unknown>): string {
  return process.env.OCTOCODE_AGENT_ID
    || String(payload.session_id ?? payload.sessionId ?? payload.agent_id ?? payload.agentId ?? 'claude-agent');
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
  const input = payloadInput(payload);
  const inputObj = objectOrEmpty(input);
  const toolName = payload.tool_name ?? payload.toolName ?? payload.name ?? inputObj.tool_name ?? inputObj.toolName ?? '';
  return extractPiWriteTargetPaths(toolName, input);
}

function resolveHookPath(file: string, cwd = process.cwd()): string {
  return file.startsWith('/') ? file : `${cwd}/${file}`;
}

function isInsidePath(candidate: string, root: string): boolean {
  const normalizedRoot = root.endsWith('/') ? root : `${root}/`;
  return candidate === root || candidate.startsWith(normalizedRoot);
}

function db() {
  return connectDb(resolveDbPath(null));
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
    const result = preFlightIntent(db(), {
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
    releaseFileLock(db(), {
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
    const report = auditUnverified(db(), { agentId: agentId(payload), ...scopeArgs(payload) });
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
      const result = notifyGet(db(), {
        agent_id: agentId(payload),
        workspace: workspace(payload) ?? undefined,
        artifact: artifact(payload) ?? undefined,
        format: 'hook',
    }) as { additionalContext?: string };
    if (result.additionalContext) {
      process.stdout.write(JSON.stringify({ additionalContext: result.additionalContext }) + '\n');
    }
  } catch {
    // fail-open
  }
  return 0;
}

async function runSessionEnd(payload: Record<string, unknown>): Promise<number> {
  if (process.env.OCTOCODE_NO_SESSION_CAPTURE === '1' || hookReason(payload) === 'clear') return 0;
  try {
    sessionCapture(db(), {
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
