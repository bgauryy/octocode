/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
const SOURCE_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../bin/awareness.ts');
const TSX_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../../../node_modules/tsx/dist/cli.mjs');
const NODE = process.execPath;
// ─── Helpers ─────────────────────────────────────────────────────────────────
function mktemp(): string {
    return mkdtempSync(join(tmpdir(), 'oc-cli-test-'));
}
interface RunResult {
    status: number;
    stdout: string;
    stderr: string;
    parsed: Record<string, unknown> | null;
}
function run(dbPath: string, args: string[], opts: {
    cwd?: string;
} = {}): RunResult {
    const result = spawnSync(NODE, [SCRIPT, '--db', dbPath, ...args], {
        cwd: opts.cwd ?? process.cwd(),
        encoding: 'utf8',
        // repo inject / heavy CLI paths can exceed 10s on cold machines
        timeout: 30000,
    });
    let parsed: Record<string, unknown> | null = null;
    try {
        parsed = JSON.parse(result.stdout) as Record<string, unknown>;
    }
    catch { /* non-JSON */ }
    return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}
function runSource(args: string[], opts: {
    cwd?: string;
    env?: NodeJS.ProcessEnv;
} = {}): RunResult {
    const result = spawnSync(NODE, [TSX_SCRIPT, SOURCE_SCRIPT, ...args], {
        cwd: opts.cwd ?? process.cwd(),
        env: opts.env ?? process.env,
        encoding: 'utf8',
        timeout: 30000,
    });
    let parsed: Record<string, unknown> | null = null;
    try {
        parsed = JSON.parse(result.stdout) as Record<string, unknown>;
    }
    catch { /* non-JSON */ }
    return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}
function ok(dbPath: string, args: string[], opts: {
    cwd?: string;
} = {}): Record<string, unknown> {
    const r = run(dbPath, args, opts);
    expect(r.status, `expected exit 0 for ${args[0]}: stderr=${r.stderr} stdout=${r.stdout}`).toBe(0);
    expect(r.parsed?.['ok'], `expected ok:true for ${args[0]}: ${r.stdout}`).not.toBe(false);
    return r.parsed!;
}
function fail(dbPath: string, args: string[], expectedStatus = 1): Record<string, unknown> | null {
    const r = run(dbPath, args);
    expect(r.status, `expected exit ${expectedStatus} for ${args[0]}: stdout=${r.stdout}`).toBe(expectedStatus);
    return r.parsed;
}

// ─── integration: full round-trip ────────────────────────────────────────────

describe('integration: full round-trip', () => {
  it('reserves exit 2 for a live task-claim conflict', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const createdPlan = ok(db, [
        'plan', 'create', '--name', 'Claim exits', '--objective', 'Classify task claim failures',
        '--lead-agent-id', 'lead', '--workspace', dir,
      ]);
      const planId = String((createdPlan['plan'] as Record<string, unknown>)['plan_id']);
      const createdTask = ok(db, [
        'task', 'create', '--plan-id', planId, '--title', 'Claim once',
        '--reasoning', 'Exercise coordination exits', '--acceptance', 'Exit codes match',
        '--path', 'src/a.ts', '--agent-id', 'lead',
      ]);
      const taskId = String((createdTask['task'] as Record<string, unknown>)['task_id']);

      expect(runSource(['--db', db, 'task', 'claim', '--task-id', 'task_missing', '--agent-id', 'worker']).status)
        .toBe(1);
      expect(runSource(['--db', db, 'task', 'claim', '--task-id', taskId, '--agent-id', 'worker']).status)
        .toBe(0);
      expect(runSource(['--db', db, 'task', 'claim', '--task-id', taskId, '--agent-id', 'other']).status)
        .toBe(2);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('creates a plan, chooses a ready task, claims, submits, and verifies it', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const createdPlan = ok(db, [
        'plan', 'create', '--name', 'Release readiness', '--objective', 'Coordinate remaining release work',
        '--lead-agent-id', 'lead', '--workspace', dir,
      ]);
      const plan = createdPlan['plan'] as Record<string, unknown>;
      const planId = String(plan['plan_id']);
      expect(existsSync(join(dir, String(plan['doc_dir']), 'PLAN.md'))).toBe(true);

      const createdTask = ok(db, [
        'task', 'create', '--plan-id', planId, '--title', 'Schema migration',
        '--reasoning', 'Consumers need the storage contract first', '--acceptance', 'Focused tests pass',
        '--path', 'src/db.ts', '--agent-id', 'lead', '--priority', '10',
      ]);
      const taskId = String((createdTask['task'] as Record<string, unknown>)['task_id']);
      const ready = ok(db, ['task', 'ready', '--plan-id', planId]);
      expect((ready['tasks'] as Record<string, unknown>[]).map((task) => task['task_id'])).toContain(taskId);

      const claimed = ok(db, ['task', 'claim', '--task-id', taskId, '--agent-id', 'worker']);
      const runId = String((claimed['run'] as Record<string, unknown>)['run_id']);
      expect(runId).toMatch(/^run_/);

      ok(db, ['task', 'submit', '--task-id', taskId, '--run-id', runId, '--agent-id', 'worker', '--message', 'focused tests pass']);
      ok(db, ['verify', 'mark', '--run-id', runId, '--agent-id', 'worker', '--message', 'focused tests pass']);
      const shown = ok(db, ['task', 'show', '--task-id', taskId]);
      expect((shown['task'] as Record<string, unknown>)['status']).toBe('DONE');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('two-agent claim → conflict → release → reclaim cycle', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const tf = join(dir, 'shared.txt');
    writeFileSync(tf, 'seed');
    try {
      const a = ok(db, ['lock', 'acquire', '--agent-id', 'agent-a', '--target-file', tf]);
      const runId = (a['run'] as Record<string, unknown>)['run_id'] as string;

      const blocked = run(db, ['lock', 'acquire', '--agent-id', 'agent-b', '--target-file', tf]);
      expect(blocked.status).toBe(2);

      ok(db, ['lock', 'release', '--agent-id', 'agent-a', '--run-id', runId, '--status', 'PENDING']);
      const reclaim = ok(db, ['lock', 'acquire', '--agent-id', 'agent-b', '--target-file', tf]);
      expect((reclaim['run'] as Record<string, unknown>)['agent_id']).toBe('agent-b');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('memory recall after record -> reflect round-trip', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const unique = `integration_test_${Date.now()}`;
      ok(db, [
        'memory', 'record', '--agent-id', 'a',
        '--task-context', unique,
        '--observation', `${unique} observation`,
        '--importance', '8',
      ]);
      ok(db, ['reflect', 'record', '--agent-id', 'a', '--task', unique, '--outcome', 'worked']);
      const found = ok(db, ['memory', 'recall', '--query', unique, '--min-importance', '1', '--limit', '5']);
      expect(found['count'] as number).toBeGreaterThanOrEqual(1);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('docs list returns skill-ref catalog JSON', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const listed = ok(db, ['docs', 'list', '--compact']);
      expect(listed['count'] as number).toBeGreaterThan(0);
      const docs = listed['docs'] as Array<Record<string, unknown>>;
      expect(docs.some((doc) => doc['name'] === 'architecture')).toBe(true);
      expect(docs.every((doc) => !('path' in doc))).toBe(true);
      expect(listed).not.toHaveProperty('root');
      expect(docs.every((doc) => Object.keys(doc).sort().join(',') === 'name,title')).toBe(true);
      expect(Buffer.byteLength(JSON.stringify(listed), 'utf8')).toBeLessThanOrEqual(3 * 1024);
      expect(String(listed['next'])).toContain('docs show');

      const full = ok(db, ['docs', 'list', '--full', '--compact']);
      const fullDocs = full['docs'] as Array<Record<string, unknown>>;
      expect(fullDocs.every((doc) => typeof doc['path'] === 'string')).toBe(true);
      expect(typeof full['root']).toBe('string');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('docs show prints markdown by default and JSON with --compact', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const raw = run(db, ['docs', 'show', 'hooks']);
      expect(raw.status).toBe(0);
      expect(raw.stdout).toMatch(/^#/m);
      expect(raw.parsed).toBeNull();

      const compact = ok(db, ['docs', 'show', 'hooks', '--compact']);
      expect(compact['name']).toBe('hooks');
      expect(String(compact['content'])).toContain('#');
      expect(compact['kind']).toBe('skill-ref');

      const missing = fail(db, ['docs', 'show', 'no-such-doc-xyz', '--compact']);
      expect(missing?.['ok']).toBe(false);
      expect(String(missing?.['error'])).toContain('docs list');
    } finally { rmSync(dir, { recursive: true }); }
  });
});
