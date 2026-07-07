/**
 * cli.test.ts — subprocess-based CLI contract tests for dist/bin/awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/bin/awareness.js');
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

function run(dbPath: string, args: string[], opts: { cwd?: string } = {}): RunResult {
  const result = spawnSync(NODE, [SCRIPT, '--db', dbPath, ...args], {
    cwd: opts.cwd ?? process.cwd(),
    encoding: 'utf8',
    timeout: 10000,
  });
  let parsed: Record<string, unknown> | null = null;
  try { parsed = JSON.parse(result.stdout) as Record<string, unknown>; } catch { /* non-JSON */ }
  return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}

function ok(dbPath: string, args: string[], opts: { cwd?: string } = {}): Record<string, unknown> {
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

// ─── init ─────────────────────────────────────────────────────────────────────

describe('init', () => {
  it('creates DB and returns initialized=true', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const result = ok(db, ['init']);
      expect(result['initialized']).toBe(true);
      expect(result['memory_count']).toBe(0);
      expect(existsSync(db)).toBe(true);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('is idempotent — second init succeeds', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try { ok(db, ['init']); ok(db, ['init']); }
    finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── tell-memory ──────────────────────────────────────────────────────────────

describe('tell-memory', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('stores a memory and returns correct shape', () => {
    const result = ok(db, [
      'tell-memory', '--agent-id', 'agent-a',
      '--task-context', 'unit test context',
      '--observation', 'node:sqlite works for memory storage',
      '--importance', '7',
      '--label', 'GOTCHA',
    ]);
    const m = result['memory'] as Record<string, unknown>;
    expect(m['memory_id']).toMatch(/^mem_/);
    expect(m['agent_id']).toBe('agent-a');
    expect(m['importance']).toBe(7);
    expect(m['label']).toBe('GOTCHA');
    expect(m['state']).toBe('ACTIVE');
    expect(m['created_at']).toBeTruthy();
  });

  it('tags are stored and normalized', () => {
    const result = ok(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5', '--tag', 'FOO', '--tag', 'bar-baz',
    ]);
    expect((result['memory'] as Record<string, unknown>)['tags']).toEqual(['foo', 'bar-baz']);
  });

  it('references are stored', () => {
    const result = ok(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5',
      '--reference', 'https://example.com',
      '--reference', 'pr:owner/repo#123',
    ]);
    expect((result['memory'] as Record<string, unknown>)['references']).toEqual([
      'https://example.com', 'pr:owner/repo#123',
    ]);
  });

  it('unknown label defaults to OTHER', () => {
    const result = ok(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5', '--label', 'NOTAREAL',
    ]);
    expect((result['memory'] as Record<string, unknown>)['label']).toBe('OTHER');
  });

  it('supersedes marks old memory SUPERSEDED', () => {
    const first = ok(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'old',
      '--importance', '5',
    ]);
    const oldId = (first['memory'] as Record<string, unknown>)['memory_id'];
    const second = ok(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'new',
      '--importance', '6', '--supersedes', oldId as string,
    ]);
    expect(second['superseded']).toContain(oldId);
  });

  it('importance out of range exits 1', () => {
    fail(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '11',
    ]);
  });

  it('missing --task-context exits 1', () => {
    fail(db, ['tell-memory', '--agent-id', 'a', '--observation', 'obs', '--importance', '5']);
  });

  it('missing --observation exits 1', () => {
    fail(db, ['tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--importance', '5']);
  });

  it('--compact produces single-line JSON', () => {
    const r = run(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5', '--compact',
    ]);
    expect(r.status).toBe(0);
    const lines = r.stdout.trim().split('\n');
    expect(lines).toHaveLength(1);
    const parsed = JSON.parse(lines[0]!) as Record<string, unknown>;
    expect((parsed['memory'] as Record<string, unknown>)['memory_id']).toMatch(/^mem_/);
  });

  it('duplicate tags are deduplicated', () => {
    const result = ok(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5', '--tag', 'dup', '--tag', 'dup', '--tag', 'dup',
    ]);
    expect((result['memory'] as Record<string, unknown>)['tags']).toEqual(['dup']);
  });
});

// ─── get-memory ───────────────────────────────────────────────────────────────

describe('get-memory', () => {
  let dir: string;
  let db: string;
  beforeAll(() => {
    dir = mktemp();
    db = join(dir, 'test.sqlite3');
    ok(db, ['tell-memory', '--agent-id', 'a', '--task-context', 'sqlite fts unit test', '--observation', 'node:sqlite FTS5 recall works', '--importance', '8', '--tag', 'sqlite', '--label', 'GOTCHA']);
    ok(db, ['tell-memory', '--agent-id', 'a', '--task-context', 'docker networking', '--observation', 'bridge networks isolate traffic', '--importance', '3', '--tag', 'docker', '--label', 'DECISION']);
    ok(db, ['tell-memory', '--agent-id', 'a', '--task-context', 'python gotcha', '--observation', 'mutable defaults in python cause bugs', '--importance', '9', '--tag', 'python', '--label', 'BUG']);
  });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('returns matching memories for a query', () => {
    const result = ok(db, ['get-memory', '--query', 'sqlite recall', '--limit', '5']);
    expect(result['count'] as number).toBeGreaterThanOrEqual(1);
    expect((result['memories'] as Record<string, unknown>[])[0]!['memory_id']).toMatch(/^mem_/);
  });

  it('min-importance filters correctly', () => {
    const result = ok(db, ['get-memory', '--query', 'docker', '--min-importance', '5']);
    const dockerMem = (result['memories'] as Record<string, unknown>[]).find(
      m => (m['task_context'] as string).includes('docker')
    );
    expect(dockerMem).toBeUndefined();
  });

  it('returns empty list for unmatched query', () => {
    const result = ok(db, ['get-memory', '--query', 'xyzzy_nonexistent_123abc', '--min-importance', '1']);
    expect(result['count']).toBe(0);
  });

  it('label filter works', () => {
    const result = ok(db, ['get-memory', '--query', 'test', '--label', 'BUG', '--min-importance', '1']);
    const mems = result['memories'] as Record<string, unknown>[];
    expect(mems.every(m => m['label'] === 'BUG')).toBe(true);
  });

  it('all memories have a numeric score', () => {
    const result = ok(db, ['get-memory', '--query', 'sqlite', '--min-importance', '1', '--limit', '5']);
    const mems = result['memories'] as Record<string, unknown>[];
    for (const m of mems) {
      expect(typeof m['score']).toBe('number');
      expect(m['score'] as number).toBeGreaterThanOrEqual(0);
    }
  });

  it('compact mode returns single-line output', () => {
    const r = run(db, ['get-memory', '--query', 'sqlite', '--compact']);
    expect(r.status).toBe(0);
    expect(r.stdout.trim().split('\n')).toHaveLength(1);
  });

  it('smart=true returns same or more results', () => {
    const base = ok(db, ['get-memory', '--query', 'sqlite recall', '--min-importance', '9', '--limit', '5']);
    const smart = ok(db, ['get-memory', '--query', 'sqlite recall', '--min-importance', '9', '--smart', '--limit', '5']);
    expect(smart['count'] as number).toBeGreaterThanOrEqual(base['count'] as number);
  });

  it('bumps access_count on recall', () => {
    const w = ok(db, [
      'tell-memory', '--agent-id', 'a', '--task-context', 'access bump test',
      '--observation', 'uniqueaccesstoken77', '--importance', '6',
    ]);
    const memId = (w['memory'] as Record<string, unknown>)['memory_id'];
    ok(db, ['get-memory', '--query', 'uniqueaccesstoken77', '--min-importance', '1']);
    ok(db, ['get-memory', '--query', 'uniqueaccesstoken77', '--min-importance', '1']);
    const result = ok(db, ['get-memory', '--query', 'uniqueaccesstoken77', '--min-importance', '1']);
    const found = (result['memories'] as Record<string, unknown>[]).find(m => m['memory_id'] === memId);
    if (found) expect(found['access_count'] as number).toBeGreaterThanOrEqual(2);
  });
});

// ─── reflect ─────────────────────────────────────────────────────────────────

describe('reflect', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('emits exactly ONE JSON object (no stdout monkey-patching)', () => {
    const r = run(db, ['reflect', '--agent-id', 'a', '--task', 'some task', '--outcome', 'worked']);
    expect(r.status).toBe(0);
    // Must be parseable as a single document
    const parsed = JSON.parse(r.stdout.trim()) as Record<string, unknown>;
    expect(parsed['outcome']).toBe('worked');
    expect(parsed['learning_memory_id']).toMatch(/^mem_/);
  });

  it('fix_repo creates a refinement', () => {
    const result = ok(db, [
      'reflect', '--agent-id', 'a', '--task', 'fix task', '--outcome', 'partial',
      '--fix-repo', 'Wire the build step',
    ]);
    expect(result['repo_fix_refinement_id']).toMatch(/^ref_/);
  });

  it('without fix_repo, repo_fix_refinement_id is null', () => {
    const result = ok(db, ['reflect', '--agent-id', 'a', '--task', 'simple', '--outcome', 'worked']);
    expect(result['repo_fix_refinement_id']).toBeNull();
  });

  it('invalid outcome coerces to partial', () => {
    const result = ok(db, ['reflect', '--agent-id', 'a', '--task', 'task', '--outcome', 'INVALID']);
    expect(result['outcome']).toBe('partial');
  });

  it('fix_harness sets harness_fix=true', () => {
    const result = ok(db, [
      'reflect', '--agent-id', 'a', '--task', 'harness', '--outcome', 'partial',
      '--fix-harness', 'improve retry logic',
    ]);
    expect(result['harness_fix']).toBe(true);
  });

  it('includes next field as non-empty string', () => {
    const result = ok(db, ['reflect', '--agent-id', 'a', '--task', 't', '--outcome', 'worked']);
    expect(typeof result['next']).toBe('string');
    expect((result['next'] as string).length).toBeGreaterThan(10);
  });

  it('missing --task exits 1', () => {
    fail(db, ['reflect', '--agent-id', 'a', '--outcome', 'worked']);
  });
});

// ─── refine-set / refine-get ──────────────────────────────────────────────────

describe('refine-set / refine-get', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('stores refinement with correct shape', () => {
    const result = ok(db, [
      'refine-set', '--agent-id', 'a',
      '--reasoning', 'Something needs fixing',
      '--remember', 'Run yarn test after',
      '--quality', 'bad', '--state', 'open',
    ]);
    const r = result['refinement'] as Record<string, unknown>;
    expect(r['refinement_id']).toMatch(/^ref_/);
    expect(r['quality']).toBe('bad');
    expect(r['state']).toBe('open');
  });

  it('refine-get returns open refinements by default', () => {
    ok(db, [
      'refine-set', '--agent-id', 'a',
      '--reasoning', 'Fix the DB schema query',
      '--remember', 'Add index before deploy',
      '--quality', 'bad', '--state', 'open',
    ]);
    const result = ok(db, ['refine-get', '--state', 'open']);
    expect(result['count'] as number).toBeGreaterThanOrEqual(1);
    const refs = result['refinements'] as Record<string, unknown>[];
    expect(refs.every(r => r['state'] === 'open')).toBe(true);
  });

  it('refine-get filters by state=done returns 0', () => {
    const result = ok(db, ['refine-get', '--state', 'done']);
    expect(result['count']).toBe(0);
  });

  it('refine-get filters by quality', () => {
    ok(db, [
      'refine-set', '--agent-id', 'a',
      '--reasoning', 'Good handoff',
      '--remember', 'Good one',
      '--quality', 'good', '--state', 'open',
    ]);
    ok(db, [
      'refine-set', '--agent-id', 'a',
      '--reasoning', 'Bad handoff',
      '--remember', 'Bad one',
      '--quality', 'bad', '--state', 'open',
    ]);
    const result = ok(db, ['refine-get', '--state', 'open', '--quality', 'bad']);
    const refs = result['refinements'] as Record<string, unknown>[];
    expect(refs.length).toBeGreaterThanOrEqual(1);
    expect(refs.every(r => r['quality'] === 'bad')).toBe(true);
  });

  it('refine-get hides handoffs by default and includes them on opt-in', () => {
    ok(db, [
      'refine-set', '--agent-id', 'a',
      '--reasoning', 'Session handoff',
      '--remember', 'Review session handoff for agent-a',
      '--quality', 'handoff', '--state', 'open',
    ]);
    const defaultResult = ok(db, ['refine-get', '--state', 'open']);
    const defaultRefs = defaultResult['refinements'] as Record<string, unknown>[];
    expect(defaultRefs.every(r => r['quality'] !== 'handoff')).toBe(true);

    const handoffResult = ok(db, ['refine-get', '--state', 'open', '--include-handoffs']);
    const handoffRefs = handoffResult['refinements'] as Record<string, unknown>[];
    expect(handoffRefs.some(r => r['quality'] === 'handoff')).toBe(true);
  });

  it('missing --reasoning exits 1', () => {
    fail(db, ['refine-set', '--agent-id', 'a', '--remember', 'do X']);
  });
});

// ─── pre-flight-intent / release-file-lock ───────────────────────────────────

describe('pre-flight-intent', () => {
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

  it('acquires lock and returns task shape', () => {
    const result = ok(db, [
      'pre-flight-intent', '--agent-id', 'agent-a', '--workspace', dir,
      '--rationale', 'test write', '--test-plan', 'verify afterwards',
      '--target-file', targetFile,
    ]);
    const task = result['task'] as Record<string, unknown>;
    expect(task['task_id']).toMatch(/^task_/);
    expect(task['status']).toBe('ACTIVE');
    expect(task['target_files']).toContain(targetFile);
  });

  it('second agent blocked with exit 2', () => {
    // agent-a still holds from above test
    const r = run(db, [
      'pre-flight-intent', '--agent-id', 'agent-b', '--workspace', dir,
      '--target-file', targetFile,
    ]);
    expect(r.status).toBe(2);
    expect(r.parsed?.['conflicts']).toBeTruthy();
  });

  it('conflict details include file_path and agent_id', () => {
    const r = run(db, [
      'pre-flight-intent', '--agent-id', 'agent-c', '--target-file', targetFile,
    ]);
    const conflicts = r.parsed?.['conflicts'] as Record<string, unknown>[] | undefined;
    expect(conflicts?.[0]?.['file_path']).toBe(targetFile);
    expect(conflicts?.[0]?.['agent_id']).toBe('agent-a');
  });

  it('rejects ttl-minutes below schema minimum', () => {
    const r = run(db, [
      'pre-flight-intent', '--agent-id', 'agent-z', '--target-file', targetFile, '--ttl-minutes', '0',
    ]);
    expect(r.status).toBe(1);
    expect(r.parsed?.['error']).toContain('--ttl-minutes must be >= 1');
  });
});

describe('release-file-lock', () => {
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

  it('releases by task_id, then allows re-claim', () => {
    const claim = ok(db, [
      'pre-flight-intent', '--agent-id', 'agent-a', '--target-file', targetFile,
    ]);
    const taskId = (claim['task'] as Record<string, unknown>)['task_id'] as string;

    const releaseAttempt = run(db, [
      'release-file-lock', '--agent-id', 'agent-a', '--task-id', taskId, '--status', 'SUCCESS',
    ]);
    expect(releaseAttempt.status).toBe(2);
    const rel = releaseAttempt.parsed!;
    expect(rel['ok']).toBe(false);
    expect(rel['released']).toBe(true);
    expect(rel['locks_released']).toBe(1);
    expect(rel['status']).toBe('PENDING');
    expect(rel['unverifiedConclusion']).toContain('SUCCESS requested without --verified');

    // Should now be claimable by agent-b
    const b = ok(db, ['pre-flight-intent', '--agent-id', 'agent-b', '--target-file', targetFile]);
    expect((b['task'] as Record<string, unknown>)['agent_id']).toBe('agent-b');
  });

  it('PENDING and FAILED statuses accepted', () => {
    // Release agent-b's lock from the previous test first
    ok(db, ['release-file-lock', '--agent-id', 'agent-b', '--target-file', targetFile, '--status', 'PENDING']);
    const claim = ok(db, ['pre-flight-intent', '--agent-id', 'agent-x', '--target-file', targetFile]);
    const taskId = (claim['task'] as Record<string, unknown>)['task_id'] as string;
    const rel = ok(db, [
      'release-file-lock', '--agent-id', 'agent-x', '--task-id', taskId, '--status', 'PENDING',
    ]);
    expect(rel['status']).toBe('PENDING');
  });

  it('no --task-id and no --target-file exits 1', () => {
    fail(db, ['release-file-lock', '--agent-id', 'a']);
  });
});

// ─── status ───────────────────────────────────────────────────────────────────

describe('status', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('returns correct memory_count', () => {
    ok(db, ['tell-memory', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'o', '--importance', '5']);
    const result = ok(db, ['status']);
    expect(result['memory_count'] as number).toBeGreaterThanOrEqual(1);
  });

  it('fts_enabled=true when FTS5 available', () => {
    const result = ok(db, ['status']);
    expect(result['fts_enabled']).toBe(true);
  });

  it('memory_states contains ACTIVE count', () => {
    const result = ok(db, ['status']);
    expect((result['memory_states'] as Record<string, number>)['ACTIVE']).toBeGreaterThan(0);
  });

  it('locks array is present', () => {
    const result = ok(db, ['status']);
    expect(Array.isArray(result['locks'])).toBe(true);
  });

  it('memory_labels is present', () => {
    const result = ok(db, ['status']);
    expect(typeof result['memory_labels']).toBe('object');
  });
});

// ─── self-test ────────────────────────────────────────────────────────────────

describe('self-test', () => {
  it('all checks pass (uses in-memory DB)', () => {
    // self-test ignores --db flag, always uses :memory:
    const r = spawnSync(NODE, [SCRIPT, 'self-test'], { encoding: 'utf8', timeout: 10000 });
    expect(r.status).toBe(0);
    const parsed = JSON.parse(r.stdout) as Record<string, unknown>;
    expect(parsed['ok']).toBe(true);
    const checks = parsed['checks'] as Record<string, boolean>;
    expect(checks['write']).toBe(true);
    expect(checks['fts_recall']).toBe(true);
    expect(checks['scoring']).toBe(true);
    expect(checks['refinement']).toBe(true);
  });
});

// ─── CLI ─────────────────────────────────────────────────────────────────────

describe('CLI', () => {
  it('--help exits 0', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('tell-memory');
  });

  it('unknown command exits 1 with JSON error', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const r = run(db, ['totally-unknown-command-xyz']);
      expect(r.status).toBe(1);
      expect(r.parsed?.['error']).toContain('unknown command');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('OCTOCODE_AWARENESS_COMPACT=1 env produces compact output', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const r = spawnSync(NODE, [SCRIPT, '--db', db, 'init'], {
        encoding: 'utf8', timeout: 5000,
        env: { ...process.env, OCTOCODE_AWARENESS_COMPACT: '1' },
      });
      expect(r.status).toBe(0);
      expect(r.stdout.trim().split('\n')).toHaveLength(1);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('--db after command works (extractGlobalDb is position-independent)', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const r = spawnSync(NODE, [SCRIPT, 'init', '--db', db], { encoding: 'utf8', timeout: 5000 });
      expect(r.status).toBe(0);
      const parsed = JSON.parse(r.stdout) as Record<string, unknown>;
      expect(parsed['initialized']).toBe(true);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('schema list maps to accepted CLI commands or explicit aliases', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    if (!existsSync(schemaScript)) return;
    const schema = spawnSync(NODE, [schemaScript, 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    const help = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(help.status).toBe(0);
    const aliases: Record<string, string> = {
      tell_memory: 'tell-memory',
      get_memory: 'get-memory',
      memory_index: 'memory-index',
      pre_flight_intent: 'pre-flight-intent',
      wait_for_lock: 'wait-for-lock',
      prune_stale_locks: 'prune-stale-locks',
      release_file_lock: 'release-file-lock',
      forget_memory: 'forget',
      refinement: 'refine-set',
      refine_query: 'refine-get',
      refine_delete: 'refine-delete',
      notify_query: 'notify-get',
      notify_resolve: 'notify-resolve',
      notify_prune: 'notify-prune',
      workspace_status: 'workspace-status',
      export_harness: 'export-harness',
    };
    const unsupported = ['stats', 'embed_index', 'harness_apply', 'memory_export', 'memory_import'];
    expect(listed).not.toEqual(expect.arrayContaining(unsupported));
    for (const key of listed) {
      const command = aliases[key] ?? key.replaceAll('_', '-');
      expect(help.stdout, `${key} should map to CLI command ${command}`).toContain(command);
    }
  });
});

// ─── edge cases ───────────────────────────────────────────────────────────────

describe('edge cases', () => {
  it('WAL mode allows 3 rapid sequential tell-memory calls', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      for (let i = 0; i < 3; i++) {
        ok(db, [
          'tell-memory', '--agent-id', `a${i}`,
          '--task-context', `ctx${i}`, '--observation', `obs${i}`,
          '--importance', '5',
        ]);
      }
      const result = ok(db, ['status']);
      expect(result['memory_count']).toBe(3);
    } finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── integration: full round-trip ────────────────────────────────────────────

describe('integration: full round-trip', () => {
  it('two-agent claim → conflict → release → reclaim cycle', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const tf = join(dir, 'shared.txt');
    writeFileSync(tf, 'seed');
    try {
      const a = ok(db, ['pre-flight-intent', '--agent-id', 'agent-a', '--target-file', tf]);
      const taskId = (a['task'] as Record<string, unknown>)['task_id'] as string;

      const blocked = run(db, ['pre-flight-intent', '--agent-id', 'agent-b', '--target-file', tf]);
      expect(blocked.status).toBe(2);

      ok(db, ['release-file-lock', '--agent-id', 'agent-a', '--task-id', taskId, '--status', 'PENDING']);
      const reclaim = ok(db, ['pre-flight-intent', '--agent-id', 'agent-b', '--target-file', tf]);
      expect((reclaim['task'] as Record<string, unknown>)['agent_id']).toBe('agent-b');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('memory recall after tell → reflect round-trip', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const unique = `integration_test_${Date.now()}`;
      ok(db, [
        'tell-memory', '--agent-id', 'a',
        '--task-context', unique,
        '--observation', `${unique} observation`,
        '--importance', '8',
      ]);
      ok(db, ['reflect', '--agent-id', 'a', '--task', unique, '--outcome', 'worked']);
      const found = ok(db, ['get-memory', '--query', unique, '--min-importance', '1', '--limit', '5']);
      expect(found['count'] as number).toBeGreaterThanOrEqual(1);
    } finally { rmSync(dir, { recursive: true }); }
  });
});
