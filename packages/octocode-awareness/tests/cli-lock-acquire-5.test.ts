/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
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

// ─── pre-flight-intentrelease-file-lock ───────────────────────────────────

describe('lock acquire', () => {
  let dir: string;
  let db: string;
  let targetFile: string;
  beforeAll(() => {
    dir = mktemp();
    db = join(dir, 'test.sqlite3');
    targetFile = join(dir, 'target.txt');
    writeFileSync(targetFile, 'content');
  });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('acquires lock and returns run shape', () => {
    const result = ok(db, [
      'lock', 'acquire', '--agent-id', 'agent-a', '--workspace', dir,
      '--rationale', 'test write', '--test-plan', 'verify afterwards',
      '--target-file', targetFile,
    ]);
    const executionRun = result['run'] as Record<string, unknown>;
    expect(executionRun['run_id']).toMatch(/^run_/);
    expect(executionRun['status']).toBe('ACTIVE');
    expect(executionRun['target_files']).toContain(realpathSync(targetFile));
  });

  it('second agent blocked with exit 2', () => {
    // agent-a still holds from above test
    const r = run(db, [
      'lock', 'acquire', '--agent-id', 'agent-b', '--workspace', dir,
      '--target-file', targetFile,
    ]);
    expect(r.status).toBe(2);
    expect(r.parsed?.['conflicts']).toBeTruthy();
  });

  it('conflict details include file_path and agent_id', () => {
    const r = run(db, [
      'lock', 'acquire', '--agent-id', 'agent-c', '--target-file', targetFile,
    ]);
    const conflicts = r.parsed?.['conflicts'] as Record<string, unknown>[] | undefined;
    expect(conflicts?.[0]?.['file_path']).toBe(realpathSync(targetFile));
    expect(conflicts?.[0]?.['agent_id']).toBe('agent-a');
  });

  it('rejects ttl-minutes below schema minimum', () => {
    const r = run(db, [
      'lock', 'acquire', '--agent-id', 'agent-z', '--target-file', targetFile, '--ttl-minutes', '0',
    ]);
    expect(r.status).toBe(1);
    expect(r.parsed?.['error']).toContain('--ttl-minutes must be >= 1');
  });

  it('rejects wait and retry values outside the schema/runtime bounds', () => {
    const tooLongWait = run(db, [
      'lock', 'wait', '--agent-id', 'agent-z', '--target-file', targetFile, '--wait-seconds', '3601',
    ]);
    expect(tooLongWait.status).toBe(1);
    expect(tooLongWait.parsed?.['error']).toContain('--wait-seconds must be <= 3600');

    const tooSlowRetry = run(db, [
      'lock', 'acquire', '--agent-id', 'agent-z', '--target-file', targetFile, '--retry-interval', '301',
    ]);
    expect(tooSlowRetry.status).toBe(1);
    expect(tooSlowRetry.parsed?.['error']).toContain('--retry-interval must be <= 300');

    const nonIntegerWait = run(db, [
      'lock', 'wait', '--agent-id', 'agent-z', '--target-file', targetFile, '--wait-seconds', '1.5',
    ]);
    expect(nonIntegerWait.status).toBe(1);
    expect(nonIntegerWait.parsed?.['error']).toContain('--wait-seconds must be an integer');
  });

  it('accepts retry interval and persists context_ref', () => {
    const plannedFile = join(dir, 'planned.txt');
    writeFileSync(plannedFile, 'planned');
    const result = ok(db, [
      'lock', 'acquire', '--agent-id', 'agent-plan', '--workspace', dir,
      '--rationale', 'planned edit', '--test-plan', 'planned test',
      '--context-ref', 'docs/plans/awareness.md',
      '--target-file', plannedFile,
      '--retry-interval', '1',
    ]);
    const executionRun = result['run'] as Record<string, unknown>;
    expect(executionRun['context_ref']).toBe('docs/plans/awareness.md');

    ok(db, [
      'lock', 'release', '--agent-id', 'agent-plan',
      '--run-id', executionRun['run_id'] as string,
      '--status', 'PENDING',
    ]);
    const auditResult = run(db, [
      'verify', 'audit', '--agent-id', 'agent-plan', '--workspace', dir,
    ]);
    expect(auditResult.status).toBe(1);
    expect((auditResult.parsed?.['unverified'] as Record<string, unknown>[])[0]?.['context_ref']).toBe('docs/plans/awareness.md');
  });

  it('rejects ttl-minutes above the runtime cap', () => {
    const r = run(db, [
      'lock', 'acquire', '--agent-id', 'agent-long', '--target-file', targetFile, '--ttl-minutes', '11',
    ]);
    expect(r.status).toBe(1);
    expect(r.parsed?.['error']).toContain('--ttl-minutes must be <= 10');
  });
});

describe('lock release', () => {
  let dir: string;
  let db: string;
  let targetFile: string;
  beforeAll(() => {
    dir = mktemp();
    db = join(dir, 'test.sqlite3');
    targetFile = join(dir, 'target.txt');
    writeFileSync(targetFile, 'seed');
  });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('releases by run_id, then allows re-claim', () => {
    const claim = ok(db, [
      'lock', 'acquire', '--agent-id', 'agent-a', '--target-file', targetFile,
    ]);
    const runId = (claim['run'] as Record<string, unknown>)['run_id'] as string;

    const rel = ok(db, [
      'lock', 'release', '--agent-id', 'agent-a', '--run-id', runId, '--status', 'PENDING',
    ]);
    expect(rel['released']).toBe(true);
    expect(rel['locks_released']).toBe(1);
    expect(rel['status']).toBe('PENDING');

    // Should now be claimable by agent-b
    const b = ok(db, ['lock', 'acquire', '--agent-id', 'agent-b', '--target-file', targetFile]);
    expect((b['run'] as Record<string, unknown>)['agent_id']).toBe('agent-b');
  });

  it('PENDING and FAILED statuses accepted', () => {
    // Release agent-b's lock from the previous test first
    ok(db, ['lock', 'release', '--agent-id', 'agent-b', '--target-file', targetFile, '--status', 'PENDING']);
    const claim = ok(db, ['lock', 'acquire', '--agent-id', 'agent-x', '--target-file', targetFile]);
    const runId = (claim['run'] as Record<string, unknown>)['run_id'] as string;
    const rel = ok(db, [
      'lock', 'release', '--agent-id', 'agent-x', '--run-id', runId, '--status', 'PENDING',
    ]);
    expect(rel['status']).toBe('PENDING');
  });

  it('rejects invalid lock release --status values like ACTIVE', () => {
    const claim = ok(db, [
      'lock', 'acquire', '--agent-id', 'agent-status', '--target-file', targetFile,
      '--rationale', 'status enum', '--test-plan', 'none',
    ]);
    const runId = (claim['run'] as Record<string, unknown>)['run_id'] as string;
    const rejected = fail(db, [
      'lock', 'release', '--agent-id', 'agent-status', '--run-id', runId, '--status', 'ACTIVE',
    ]);
    expect(String(rejected?.['error'] ?? '')).toContain('--status must be PENDING or FAILED');
    ok(db, [
      'lock', 'release', '--agent-id', 'agent-status', '--run-id', runId, '--status', 'PENDING',
    ]);
  });

  it('no --run-id and no --target-file exits 1', () => {
    fail(db, ['lock', 'release', '--agent-id', 'a']);
  });

  it('no matching run-id exits 1 instead of pretending release succeeded', () => {
    const rel = run(db, [
      'lock', 'release', '--agent-id', 'agent-missing', '--run-id', 'run_missing', '--status', 'PENDING',
    ]);
    expect(rel.status).toBe(1);
    expect(rel.parsed?.['ok']).toBe(false);
    expect(rel.parsed?.['released']).toBe(false);
    expect(rel.parsed?.['locks_released']).toBe(0);
  });
});

describe('verify', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('verifies repeated explicit run ids in one command', () => {
    const runIds: string[] = [];
    for (const name of ['one.txt', 'two.txt']) {
      const filePath = join(dir, name);
      writeFileSync(filePath, name);
      const claim = ok(db, [
        'lock', 'acquire',
        '--agent-id', 'agent-v',
        '--workspace', dir,
        '--target-file', filePath,
        '--rationale', `edit ${name}`,
        '--test-plan', 'run focused verification',
      ]);
      const runId = (claim['run'] as Record<string, unknown>)['run_id'] as string;
      runIds.push(runId);
      ok(db, ['lock', 'release', '--agent-id', 'agent-v', '--run-id', runId, '--status', 'PENDING']);
    }

    const before = run(db, ['verify', 'audit', '--agent-id', 'agent-v', '--workspace', dir]);
    expect(before.status).toBe(1);
    expect(before.parsed?.['count']).toBe(2);

    const verified = ok(db, [
      'verify', 'mark',
      '--agent-id', 'agent-v',
      '--workspace', dir,
      '--run-id', runIds[0]!,
      '--run-id', runIds[1]!,
      '--message', 'run focused verification passed',
    ]);
    expect(verified['run_id']).toBeNull();
    expect(verified['run_ids']).toEqual(runIds);
    expect(verified['count']).toBe(2);

    const after = ok(db, ['verify', 'audit', '--agent-id', 'agent-v', '--workspace', dir]);
    expect(after['count']).toBe(0);
  });

  it('caps compact audit details while preserving totals', () => {
    for (let index = 0; index < 5; index++) {
      const filePath = join(dir, `cap-${index}.txt`);
      writeFileSync(filePath, String(index));
      const claim = ok(db, [
        'lock', 'acquire', '--compact', '--agent-id', 'agent-cap', '--workspace', dir,
        '--target-file', filePath, '--rationale', `edit ${index}`, '--test-plan', `test ${index}`,
      ]);
      const runId = (claim['run'] as Record<string, unknown>)['run_id'] as string;
      ok(db, ['lock', 'release', '--compact', '--agent-id', 'agent-cap', '--run-id', runId, '--status', 'PENDING']);
    }

    const audit = run(db, ['verify', 'audit', '--compact', '--agent-id', 'agent-cap', '--workspace', dir]);
    expect(audit.status).toBe(1);
    expect(audit.parsed).toMatchObject({ count: 5, unverified_count: 5, stale_active_count: 0, omitted_count: 2 });
    expect(audit.parsed?.['unverified']).toHaveLength(3);
    expect(Buffer.byteLength(audit.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);

    ok(db, ['verify', 'mark', '--compact', '--agent-id', 'agent-cap', '--workspace', dir,
      '--all-pending', '--status', 'FAILED', '--message', 'compact audit fixture cleanup']);
  });
});
