/**
 * cli.test.ts — subprocess-based CLI contract tests for dist/bin/awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, existsSync, readFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/bin/awareness.js');
const INDEX_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../dist/index.js');
const SKILL_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/awareness.mjs');
const NODE = process.execPath;

// ─── Helpers ─────────────────────────────────────────────────────────────────

function mktemp(): string {
  return mkdtempSync(join(tmpdir(), 'oc-cli-test-'));
}

function initGitRepo(root: string): string {
  const result = spawnSync('git', ['init', '-q'], { cwd: root, encoding: 'utf8', timeout: 5000 });
  expect(result.status, result.stderr || result.stdout).toBe(0);
  const gitRoot = spawnSync('git', ['-C', root, 'rev-parse', '--show-toplevel'], { encoding: 'utf8', timeout: 5000 });
  expect(gitRoot.status, gitRoot.stderr || gitRoot.stdout).toBe(0);
  return gitRoot.stdout.trim();
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

function daysAgo(days: number): string {
  return new Date(Date.now() - days * 86400000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

describe('package main direct execution', () => {
  it('delegates to the CLI when dist/index.js is executed directly', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const result = spawnSync(NODE, [
        INDEX_SCRIPT,
        '--db', db,
        'workspace', 'status',
        '--workspace', dir,
        '--compact',
      ], {
        encoding: 'utf8',
        timeout: 10000,
      });
      expect(result.status, `stderr=${result.stderr} stdout=${result.stdout}`).toBe(0);
      const parsed = JSON.parse(result.stdout) as Record<string, unknown>;
      expect(parsed['ok']).toBe(true);
      expect(parsed['workspace_path']).toBe(realpathSync(dir));
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

// ─── maintenance init ─────────────────────────────────────────────────────────

describe('maintenance init', () => {
  it('creates DB and returns initialized=true', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const result = ok(db, ['maintenance', 'init']);
      expect(result['initialized']).toBe(true);
      expect(result['memory_count']).toBe(0);
      expect(existsSync(db)).toBe(true);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('is idempotent — second init succeeds', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try { ok(db, ['maintenance', 'init']); ok(db, ['maintenance', 'init']); }
    finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── memory record ────────────────────────────────────────────────────────────

describe('memory record', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('stores a memory and returns correct shape', () => {
    const result = ok(db, [
      'memory', 'record', '--agent-id', 'agent-a',
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
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5', '--tag', 'FOO', '--tag', 'bar-baz',
    ]);
    expect((result['memory'] as Record<string, unknown>)['tags']).toEqual(['foo', 'bar-baz']);
  });

  it('references are stored', () => {
    const result = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5',
      '--reference', 'https://example.com',
      '--reference', 'pr:owner/repo#123',
    ]);
    expect((result['memory'] as Record<string, unknown>)['references']).toEqual([
      'https://example.com', 'pr:owner/repo#123',
    ]);
  });

  it('stores --file as file provenance and recalls it with memory recall --file', () => {
    const result = ok(db, [
      'memory', 'record', '--agent-id', 'a',
      '--task-context', 'file provenance',
      '--observation', 'file provenance roundtrip marker',
      '--importance', '6',
      '--workspace', dir,
      '--file', 'src/roundtrip.ts',
    ]);
    const memory = result['memory'] as Record<string, unknown>;
    const memoryId = memory['memory_id'];
    const expectedRef = `file:${join(dir, 'src/roundtrip.ts')}`;
    expect(memory['references']).toContain(expectedRef);

    const recalled = ok(db, [
      'memory', 'recall',
      '--workspace', dir,
      '--file', 'src/roundtrip.ts',
      '--limit', '1',
    ]);
    const memories = recalled['memories'] as Array<Record<string, unknown>>;
    expect(memories.map(m => m['memory_id'])).toEqual([memoryId]);
  });

  it('unknown label defaults to OTHER', () => {
    const result = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5', '--label', 'NOTAREAL',
    ]);
    expect((result['memory'] as Record<string, unknown>)['label']).toBe('OTHER');
  });

  it('supersedes marks old memory SUPERSEDED', () => {
    const first = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'old',
      '--importance', '5',
    ]);
    const oldId = (first['memory'] as Record<string, unknown>)['memory_id'];
    const second = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'new',
      '--importance', '6', '--supersedes', oldId as string,
    ]);
    expect(second['superseded']).toContain(oldId);
  });

  it('importance out of range exits 1', () => {
    fail(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '11',
    ]);
  });

  it('missing --task-context exits 1', () => {
    fail(db, ['memory', 'record', '--agent-id', 'a', '--observation', 'obs', '--importance', '5']);
  });

  it('missing --observation exits 1', () => {
    fail(db, ['memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--importance', '5']);
  });

  it('--compact produces single-line JSON', () => {
    const r = run(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
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
      'memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'obs',
      '--importance', '5', '--tag', 'dup', '--tag', 'dup', '--tag', 'dup',
    ]);
    expect((result['memory'] as Record<string, unknown>)['tags']).toEqual(['dup']);
  });
});

// ─── memory recall ────────────────────────────────────────────────────────────

describe('memory recall', () => {
  let dir: string;
  let db: string;
  beforeAll(() => {
    dir = mktemp();
    db = join(dir, 'test.sqlite3');
    ok(db, ['memory', 'record', '--agent-id', 'a', '--task-context', 'sqlite fts unit test', '--observation', 'node:sqlite FTS5 recall works', '--importance', '8', '--tag', 'sqlite', '--label', 'GOTCHA']);
    ok(db, ['memory', 'record', '--agent-id', 'a', '--task-context', 'docker networking', '--observation', 'bridge networks isolate traffic', '--importance', '3', '--tag', 'docker', '--label', 'DECISION']);
    ok(db, ['memory', 'record', '--agent-id', 'a', '--task-context', 'python gotcha', '--observation', 'mutable defaults in python cause bugs', '--importance', '9', '--tag', 'python', '--label', 'BUG']);
  });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('returns matching memories for a query', () => {
    const result = ok(db, ['memory', 'recall', '--query', 'sqlite recall', '--limit', '5']);
    expect(result['count'] as number).toBeGreaterThanOrEqual(1);
    expect((result['memories'] as Record<string, unknown>[])[0]!['memory_id']).toMatch(/^mem_/);
  });

  it('min-importance filters correctly', () => {
    const result = ok(db, ['memory', 'recall', '--query', 'docker', '--min-importance', '5']);
    const dockerMem = (result['memories'] as Record<string, unknown>[]).find(
      m => (m['task_context'] as string).includes('docker')
    );
    expect(dockerMem).toBeUndefined();
  });

  it('returns empty list for unmatched query', () => {
    const result = ok(db, ['memory', 'recall', '--query', 'xyzzy_nonexistent_123abc', '--min-importance', '1']);
    expect(result['count']).toBe(0);
  });

  it('label filter works', () => {
    const result = ok(db, ['memory', 'recall', '--query', 'test', '--label', 'BUG', '--min-importance', '1']);
    const mems = result['memories'] as Record<string, unknown>[];
    expect(mems.every(m => m['label'] === 'BUG')).toBe(true);
  });

  it('tag filter works and stays advertised in help', () => {
    const result = ok(db, ['memory', 'recall', '--query', 'traffic', '--tag', 'docker', '--min-importance', '1']);
    const mems = result['memories'] as Record<string, unknown>[];
    expect(mems.length).toBeGreaterThanOrEqual(1);
    expect(mems.every(m => (m['tags'] as string[]).includes('docker'))).toBe(true);

    const help = spawnSync(NODE, [SCRIPT, 'memory', 'recall', '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(help.status).toBe(0);
    expect(help.stdout).toContain('[--tag <t>]...');
    expect(help.stdout).toContain('[--semantic]');
  });

  it('rejects unsupported sort values instead of falling back silently', () => {
    const result = fail(db, ['memory', 'recall', '--query', 'test', '--sort', 'label']);
    expect(result?.['error']).toContain('--sort must be one of: smart, score, importance, recent, accessed');
  });

  it('semantic flag returns lexical results with a warning', () => {
    const result = ok(db, ['memory', 'recall', '--query', 'sqlite', '--semantic']);
    expect(result['mode']).toBeTruthy();
    expect(result['warnings']).toEqual(expect.arrayContaining([
      expect.stringContaining('semantic ranking is unavailable in the CLI'),
    ]));
  });

  it('semantic flag ranks when OCTOCODE_EMBED_CMD returns embeddings', () => {
    const dir = mktemp();
    const embedScript = join(dir, 'embed.mjs');
    writeFileSync(embedScript, `#!/usr/bin/env node
import { readFileSync } from 'node:fs';
const text = readFileSync(0, 'utf8');
const embedding = text.includes('alpha') ? [1, 0, 0] : text.includes('beta') ? [0, 1, 0] : [0.2, 0.8, 0];
process.stdout.write(JSON.stringify({ embedding, model: 'test-embed' }));
`, 'utf8');
    const prev = process.env['OCTOCODE_EMBED_CMD'];
    process.env['OCTOCODE_EMBED_CMD'] = `node ${embedScript}`;
    try {
      ok(db, [
        'memory', 'record',
        '--agent-id', 'embed-a',
        '--task-context', 'alpha vector memory',
        '--observation', 'alpha content for semantic ranking',
        '--importance', '8',
        '--label', 'DECISION',
        '--workspace', dir,
      ]);
      ok(db, [
        'memory', 'record',
        '--agent-id', 'embed-b',
        '--task-context', 'beta vector memory',
        '--observation', 'beta content for semantic ranking',
        '--importance', '8',
        '--label', 'DECISION',
        '--workspace', dir,
      ]);
      const result = ok(db, [
        'memory', 'recall',
        '--query', 'alpha',
        '--semantic',
        '--workspace', dir,
        '--limit', '2',
      ]);
      expect(result['mode']).toBe('semantic');
      expect(result['embedding_model']).toBe('test-embed');
      const memories = result['memories'] as Array<Record<string, unknown>>;
      expect(memories[0]?.['task_context']).toContain('alpha');
    } finally {
      if (prev === undefined) delete process.env['OCTOCODE_EMBED_CMD'];
      else process.env['OCTOCODE_EMBED_CMD'] = prev;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('all memories have a numeric score', () => {
    const result = ok(db, ['memory', 'recall', '--query', 'sqlite', '--min-importance', '1', '--limit', '5']);
    const mems = result['memories'] as Record<string, unknown>[];
    for (const m of mems) {
      expect(typeof m['score']).toBe('number');
      expect(m['score'] as number).toBeGreaterThanOrEqual(0);
    }
  });

  it('compact mode returns single-line output', () => {
    const r = run(db, ['memory', 'recall', '--query', 'sqlite', '--compact']);
    expect(r.status).toBe(0);
    expect(r.stdout.trim().split('\n')).toHaveLength(1);
  });

  it('smart=true returns same or more results', () => {
    const base = ok(db, ['memory', 'recall', '--query', 'sqlite recall', '--min-importance', '9', '--limit', '5']);
    const smart = ok(db, ['memory', 'recall', '--query', 'sqlite recall', '--min-importance', '9', '--smart', '--limit', '5']);
    expect(smart['count'] as number).toBeGreaterThanOrEqual(base['count'] as number);
  });

  it('bumps access_count on recall', () => {
    const w = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'access bump test',
      '--observation', 'uniqueaccesstoken77', '--importance', '6',
    ]);
    const memId = (w['memory'] as Record<string, unknown>)['memory_id'];
    ok(db, ['memory', 'recall', '--query', 'uniqueaccesstoken77', '--min-importance', '1']);
    ok(db, ['memory', 'recall', '--query', 'uniqueaccesstoken77', '--min-importance', '1']);
    const result = ok(db, ['memory', 'recall', '--query', 'uniqueaccesstoken77', '--min-importance', '1']);
    const found = (result['memories'] as Record<string, unknown>[]).find(m => m['memory_id'] === memId);
    if (found) expect(found['access_count'] as number).toBeGreaterThanOrEqual(2);
  });
});

// ─── memory forget ────────────────────────────────────────────────────────────

describe('memory forget', () => {
  it('accepts --workspace and scopes broad selectors to that workspace', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const wsA = join(dir, 'workspace-a');
    const wsB = join(dir, 'workspace-b');
    mkdirSync(wsA, { recursive: true });
    mkdirSync(wsB, { recursive: true });
    try {
      const first = ok(db, [
        'memory', 'record',
        '--agent-id', 'a',
        '--task-context', 'forget workspace a',
        '--observation', 'deprecated scoped cli memory a',
        '--importance', '3',
        '--tag', 'deprecated',
        '--workspace', wsA,
      ]);
      const second = ok(db, [
        'memory', 'record',
        '--agent-id', 'a',
        '--task-context', 'forget workspace b',
        '--observation', 'deprecated scoped cli memory b',
        '--importance', '3',
        '--tag', 'deprecated',
        '--workspace', wsB,
      ]);
      const firstId = (first['memory'] as Record<string, unknown>)['memory_id'];
      const secondId = (second['memory'] as Record<string, unknown>)['memory_id'];

      const dryRun = ok(db, [
        'memory', 'forget',
        '--tag', 'deprecated',
        '--workspace', wsA,
        '--dry-run',
      ]);
      expect(dryRun['would_delete']).toBe(1);
      expect(dryRun['memory_ids']).toEqual([firstId]);

      const deleted = ok(db, ['memory', 'forget', '--tag', 'deprecated', '--workspace', wsA]);
      expect(deleted['deleted']).toBe(1);
      expect(deleted['memory_ids']).toEqual([firstId]);

      const recalled = ok(db, ['memory', 'recall', '--query', 'deprecated scoped cli memory', '--tag', 'deprecated', '--workspace', wsB]);
      const memories = recalled['memories'] as Array<Record<string, unknown>>;
      expect(memories.some((m) => m['memory_id'] === secondId)).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

// ─── reflect ─────────────────────────────────────────────────────────────────

describe('reflect', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('emits exactly ONE JSON object (no stdout monkey-patching)', () => {
    const r = run(db, ['reflect', 'record', '--agent-id', 'a', '--task', 'some task', '--outcome', 'worked']);
    expect(r.status).toBe(0);
    // Must be parseable as a single document
    const parsed = JSON.parse(r.stdout.trim()) as Record<string, unknown>;
    expect(parsed['outcome']).toBe('worked');
    expect(parsed['learning_memory_id']).toMatch(/^mem_/);
  });

  it('fix_repo creates a refinement', () => {
    const result = ok(db, [
      'reflect', 'record', '--agent-id', 'a', '--task', 'fix task', '--outcome', 'partial',
      '--fix-repo', 'Wire the build step',
    ]);
    expect(result['repo_fix_refinement_id']).toMatch(/^ref_/);
  });

  it('without fix_repo, repo_fix_refinement_id is null', () => {
    const result = ok(db, ['reflect', 'record', '--agent-id', 'a', '--task', 'simple', '--outcome', 'worked']);
    expect(result['repo_fix_refinement_id']).toBeNull();
  });

  it('invalid outcome coerces to partial', () => {
    const result = ok(db, ['reflect', 'record', '--agent-id', 'a', '--task', 'task', '--outcome', 'INVALID']);
    expect(result['outcome']).toBe('partial');
  });

  it('fix_harness sets harness_fix=true', () => {
    const result = ok(db, [
      'reflect', 'record', '--agent-id', 'a', '--task', 'harness', '--outcome', 'partial',
      '--fix-harness', 'improve retry logic',
    ]);
    expect(result['harness_fix']).toBe(true);
  });

  it('includes next field as non-empty string', () => {
    const result = ok(db, ['reflect', 'record', '--agent-id', 'a', '--task', 't', '--outcome', 'worked']);
    expect(typeof result['next']).toBe('string');
    expect((result['next'] as string).length).toBeGreaterThan(10);
  });

  it('missing --task exits 1', () => {
    fail(db, ['reflect', 'record', '--agent-id', 'a', '--outcome', 'worked']);
  });

  it('accepts judgment-note, duo, and eval-failure-json flags', () => {
    const result = ok(db, [
      'reflect', 'record', '--agent-id', 'a',
      '--task', 'cli reflection contract',
      '--outcome', 'failed',
      '--judgment-note', 'checked CLI output; one branch untested',
      '--duo',
      '--eval-failure-json', '[{"id":"q1","dimension":"correctness","failure_signature":"cli:sig","suggested_lesson":"keep CLI flags covered"}]',
    ]);
    expect(result['eval_failure_count']).toBe(1);
    expect(result['eval_failure_ids']).toHaveLength(1);
    expect((result['reflection_duo'] as Record<string, unknown>)['advisory']).toBe(true);
  });

  it('rejects invalid eval-failure-json', () => {
    const result = fail(db, [
      'reflect', 'record', '--agent-id', 'a',
      '--task', 'bad eval json',
      '--outcome', 'failed',
      '--eval-failure-json', '{"not":"an array"}',
    ]);
    expect(result?.['error']).toContain('--eval-failure-json must be a JSON array');
  });
});

// ─── refinement set/get ───────────────────────────────────────────────────────

describe('refinement set/get', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('stores refinement with correct shape', () => {
    const result = ok(db, [
      'refinement', 'set', '--agent-id', 'a',
      '--reasoning', 'Something needs fixing',
      '--remember', 'Run yarn test after',
      '--quality', 'bad', '--state', 'open',
    ]);
    const r = result['refinement'] as Record<string, unknown>;
    expect(r['refinement_id']).toMatch(/^ref_/);
    expect(r['quality']).toBe('bad');
    expect(r['state']).toBe('open');
  });

  it('refinement get returns open refinements by default', () => {
    ok(db, [
      'refinement', 'set', '--agent-id', 'a',
      '--reasoning', 'Fix the DB schema query',
      '--remember', 'Add index before deploy',
      '--quality', 'bad', '--state', 'open',
    ]);
    const result = ok(db, ['refinement', 'get', '--state', 'open']);
    expect(result['count'] as number).toBeGreaterThanOrEqual(1);
    const refs = result['refinements'] as Record<string, unknown>[];
    expect(refs.every(r => r['state'] === 'open')).toBe(true);
  });

  it('refinement get filters by state=done returns 0', () => {
    const result = ok(db, ['refinement', 'get', '--state', 'done']);
    expect(result['count']).toBe(0);
  });

  it('refinement get filters by quality', () => {
    ok(db, [
      'refinement', 'set', '--agent-id', 'a',
      '--reasoning', 'Good handoff',
      '--remember', 'Good one',
      '--quality', 'good', '--state', 'open',
    ]);
    ok(db, [
      'refinement', 'set', '--agent-id', 'a',
      '--reasoning', 'Bad handoff',
      '--remember', 'Bad one',
      '--quality', 'bad', '--state', 'open',
    ]);
    const result = ok(db, ['refinement', 'get', '--state', 'open', '--quality', 'bad']);
    const refs = result['refinements'] as Record<string, unknown>[];
    expect(refs.length).toBeGreaterThanOrEqual(1);
    expect(refs.every(r => r['quality'] === 'bad')).toBe(true);
  });

  it('refinement get hides handoffs by default and includes them on opt-in', () => {
    ok(db, [
      'refinement', 'set', '--agent-id', 'a',
      '--reasoning', 'Session handoff',
      '--remember', 'Review session handoff for agent-a',
      '--quality', 'handoff', '--state', 'open',
    ]);
    const defaultResult = ok(db, ['refinement', 'get', '--state', 'open']);
    const defaultRefs = defaultResult['refinements'] as Record<string, unknown>[];
    expect(defaultRefs.every(r => r['quality'] !== 'handoff')).toBe(true);

    const handoffResult = ok(db, ['refinement', 'get', '--state', 'open', '--include-handoffs']);
    const handoffRefs = handoffResult['refinements'] as Record<string, unknown>[];
    expect(handoffRefs.some(r => r['quality'] === 'handoff')).toBe(true);
  });

  it('missing --reasoning exits 1', () => {
    fail(db, ['refinement', 'set', '--agent-id', 'a', '--remember', 'do X']);
  });
});

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

  it('acquires lock and returns task shape', () => {
    const result = ok(db, [
      'lock', 'acquire', '--agent-id', 'agent-a', '--workspace', dir,
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
    expect(conflicts?.[0]?.['file_path']).toBe(targetFile);
    expect(conflicts?.[0]?.['agent_id']).toBe('agent-a');
  });

  it('rejects ttl-minutes below schema minimum', () => {
    const r = run(db, [
      'lock', 'acquire', '--agent-id', 'agent-z', '--target-file', targetFile, '--ttl-minutes', '0',
    ]);
    expect(r.status).toBe(1);
    expect(r.parsed?.['error']).toContain('--ttl-minutes must be >= 1');
  });

  it('accepts retry interval and persists plan_doc_ref', () => {
    const plannedFile = join(dir, 'planned.txt');
    writeFileSync(plannedFile, 'planned');
    const result = ok(db, [
      'lock', 'acquire', '--agent-id', 'agent-plan', '--workspace', dir,
      '--rationale', 'planned edit', '--test-plan', 'planned test',
      '--plan-doc-ref', 'docs/plans/awareness.md',
      '--target-file', plannedFile,
      '--retry-interval', '1',
    ]);
    const task = result['task'] as Record<string, unknown>;
    expect(task['plan_doc_ref']).toBe('docs/plans/awareness.md');

    ok(db, [
      'lock', 'release', '--agent-id', 'agent-plan',
      '--task-id', task['task_id'] as string,
      '--status', 'PENDING',
    ]);
    const auditResult = run(db, [
      'verify', 'audit', '--agent-id', 'agent-plan', '--workspace', dir,
    ]);
    expect(auditResult.status).toBe(1);
    expect((auditResult.parsed?.['unverified'] as Record<string, unknown>[])[0]?.['plan_doc_ref']).toBe('docs/plans/awareness.md');
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

  it('releases by task_id, then allows re-claim', () => {
    const claim = ok(db, [
      'lock', 'acquire', '--agent-id', 'agent-a', '--target-file', targetFile,
    ]);
    const taskId = (claim['task'] as Record<string, unknown>)['task_id'] as string;

    const releaseAttempt = run(db, [
      'lock', 'release', '--agent-id', 'agent-a', '--task-id', taskId, '--status', 'SUCCESS',
    ]);
    expect(releaseAttempt.status).toBe(2);
    const rel = releaseAttempt.parsed!;
    expect(rel['ok']).toBe(false);
    expect(rel['released']).toBe(true);
    expect(rel['locks_released']).toBe(1);
    expect(rel['status']).toBe('PENDING');
    expect(rel['unverifiedConclusion']).toContain('SUCCESS requested without --verified');

    // Should now be claimable by agent-b
    const b = ok(db, ['lock', 'acquire', '--agent-id', 'agent-b', '--target-file', targetFile]);
    expect((b['task'] as Record<string, unknown>)['agent_id']).toBe('agent-b');
  });

  it('PENDING and FAILED statuses accepted', () => {
    // Release agent-b's lock from the previous test first
    ok(db, ['lock', 'release', '--agent-id', 'agent-b', '--target-file', targetFile, '--status', 'PENDING']);
    const claim = ok(db, ['lock', 'acquire', '--agent-id', 'agent-x', '--target-file', targetFile]);
    const taskId = (claim['task'] as Record<string, unknown>)['task_id'] as string;
    const rel = ok(db, [
      'lock', 'release', '--agent-id', 'agent-x', '--task-id', taskId, '--status', 'PENDING',
    ]);
    expect(rel['status']).toBe('PENDING');
  });

  it('no --task-id and no --target-file exits 1', () => {
    fail(db, ['lock', 'release', '--agent-id', 'a']);
  });
});

describe('verify', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('verifies repeated explicit task ids in one command', () => {
    const taskIds: string[] = [];
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
      const taskId = (claim['task'] as Record<string, unknown>)['task_id'] as string;
      taskIds.push(taskId);
      ok(db, ['lock', 'release', '--agent-id', 'agent-v', '--task-id', taskId, '--status', 'PENDING']);
    }

    const before = run(db, ['verify', 'audit', '--agent-id', 'agent-v', '--workspace', dir]);
    expect(before.status).toBe(1);
    expect(before.parsed?.['count']).toBe(2);

    const verified = ok(db, [
      'verify', 'mark',
      '--agent-id', 'agent-v',
      '--workspace', dir,
      '--task-id', taskIds[0]!,
      '--task-id', taskIds[1]!,
      '--message', 'run focused verification passed',
    ]);
    expect(verified['task_id']).toBeNull();
    expect(verified['task_ids']).toEqual(taskIds);
    expect(verified['count']).toBe(2);

    const after = ok(db, ['verify', 'audit', '--agent-id', 'agent-v', '--workspace', dir]);
    expect(after['count']).toBe(0);
  });
});

// ─── workspace status ─────────────────────────────────────────────────────────

describe('workspace status', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('returns correct memory_count', () => {
    ok(db, ['memory', 'record', '--agent-id', 'a', '--task-context', 'ctx', '--observation', 'o', '--importance', '5']);
    const result = ok(db, ['workspace', 'status']);
    expect(result['memory_count'] as number).toBeGreaterThanOrEqual(1);
  });

  it('fts_enabled=true when FTS5 available', () => {
    const result = ok(db, ['workspace', 'status']);
    expect(result['fts_enabled']).toBe(true);
  });

  it('memory_states contains ACTIVE count', () => {
    const result = ok(db, ['workspace', 'status']);
    expect((result['memory_states'] as Record<string, number>)['ACTIVE']).toBeGreaterThan(0);
  });

  it('locks array is present', () => {
    const result = ok(db, ['workspace', 'status']);
    expect(Array.isArray(result['locks'])).toBe(true);
  });

  it('memory_labels is present', () => {
    const result = ok(db, ['workspace', 'status']);
    expect(typeof result['memory_labels']).toBe('object');
  });

  it('normalizes package-subdir workspace across memory record, lock acquire, and status', () => {
    const root = mktemp();
    const dbPath = join(root, 'test.sqlite3');
    const pkg = join(root, 'packages/octocode-awareness');
    mkdirSync(join(pkg, 'src'), { recursive: true });
    writeFileSync(join(pkg, 'src/a.ts'), 'export const a = 1;\n');
    const expectedRoot = initGitRepo(root);
    try {
      const recorded = ok(dbPath, [
        'memory', 'record',
        '--agent-id', 'agent-scope',
        '--task-context', 'scope normalization',
        '--observation', 'package subdir memory should be visible from status',
        '--importance', '6',
        '--tag', 'scope-normalization',
        '--workspace', pkg,
      ]);
      const memory = recorded['memory'] as Record<string, unknown>;
      expect(memory['workspace_path']).toBe(expectedRoot);

      const claimed = ok(dbPath, [
        'lock', 'acquire',
        '--agent-id', 'agent-scope',
        '--workspace', pkg,
        '--target-file', 'src/a.ts',
        '--rationale', 'scope normalization',
        '--test-plan', 'focused cli test',
      ]);
      const task = claimed['task'] as Record<string, unknown>;
      expect(task['workspace_path']).toBe(expectedRoot);
      expect(task['target_files']).toEqual([join(pkg, 'src/a.ts')]);

      const status = ok(dbPath, ['workspace', 'status', '--workspace', pkg]);
      expect(status['workspace_path']).toBe(expectedRoot);
      expect(status['memory_count'] as number).toBeGreaterThanOrEqual(1);
      expect(status['locks'] as Record<string, unknown>[]).toHaveLength(1);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

// ─── agent registry ───────────────────────────────────────────────────────────

describe('agent registry', () => {
  let dir: string;
  let db: string;
  beforeAll(() => { dir = mktemp(); db = join(dir, 'test.sqlite3'); });
  afterAll(() => rmSync(dir, { recursive: true }));

  it('registers and lists known agents from the same DB', () => {
    const registered = ok(db, [
      'agent', 'register',
      '--agent-id', 'codex-a',
      '--agent-name', 'Codex A',
      '--workspace', dir,
      '--artifact', 'packages/octocode-awareness',
      '--context', 'codex',
    ]);
    expect(registered['action']).toBe('register');
    expect((registered['agent'] as Record<string, unknown>)['agent_name']).toBe('Codex A');

    const listed = ok(db, [
      'agent', 'list',
      '--workspace', dir,
      '--artifact', 'packages/octocode-awareness',
    ]);
    expect(listed['action']).toBe('list');
    const agents = listed['agents'] as Record<string, unknown>[];
    expect(agents).toHaveLength(1);
    expect(agents[0]?.['agent_id']).toBe('codex-a');
    expect(agents[0]?.['context']).toBe('codex');
  });

  it('requires agent id when registering', () => {
    const result = fail(db, ['agent', 'register']);
    expect(result?.['error']).toContain('--agent-id is required');
  });
});

// ─── maintenance self-test ────────────────────────────────────────────────────

describe('maintenance self-test', () => {
  it('all checks pass (uses in-memory DB)', () => {
    // maintenance self-test ignores --db flag, always uses :memory:
    const r = spawnSync(NODE, [SCRIPT, 'maintenance', 'self-test'], { encoding: 'utf8', timeout: 10000 });
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
  it('package metadata exposes the scoped public npx binary', () => {
    const packageJsonPath = resolve(dirname(fileURLToPath(import.meta.url)), '../package.json');
    const pkg = JSON.parse(readFileSync(packageJsonPath, 'utf8')) as {
      name?: string;
      bin?: Record<string, string>;
      publishConfig?: Record<string, string>;
    };
    expect(pkg.name).toBe('@octocodeai/octocode-awareness');
    expect(pkg.publishConfig?.access).toBe('public');
    expect(pkg.bin?.['octocode-awareness']).toBe('./dist/bin/awareness.js');
    expect(pkg.bin ?? {}).not.toHaveProperty('awareness');
  });

  it('--help exits 0', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('memory record');
    expect(r.stdout).toContain('local-first: use octocode-awareness or a bundled local node path when present');
    expect(r.stdout).toContain('fallback: npx @octocodeai/octocode-awareness <command>');
    expect(r.stdout).toContain('npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common');
    expect(r.stdout).toContain('octocode-awareness schema commands --compact');
    expect(r.stdout).not.toContain('tell-memory');
    expect(r.stdout).not.toContain('get-memory');
  });

  it('no command prints the easy-install discovery guide', () => {
    const r = spawnSync(NODE, [SCRIPT], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('easy install:');
    expect(r.stdout).toContain('If the CLI is bundled locally, tell your agent to run that local CLI');
    expect(r.stdout).toContain('Registry fallback only when no local CLI exists');
    expect(r.stdout).toContain('npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common');
  });

  it('--help --compact returns a short agent guide', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
    expect(r.stdout).toContain('local-first: octocode-awareness <command>');
    expect(r.stdout).toContain('fallback: npx @octocodeai/octocode-awareness <command>');
    expect(r.stdout).toContain('schema commands --compact');
    expect(r.stdout.split('\n').filter(Boolean).length).toBeLessThanOrEqual(8);
  });

  it('no command with --compact prints compact discovery instead of unknown-command JSON', () => {
    const r = spawnSync(NODE, [SCRIPT, '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
    expect(r.stdout).toContain('skill: npx octocode skill --add --path {{path_to_skills_location}}/octocode-awareness --platform common');
    expect(r.stdout).not.toContain('unknown command');
  });

  it('unknown flag-only invocation still exits nonzero', () => {
    const r = spawnSync(NODE, [SCRIPT, '--bogus-flag', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(1);
    expect(r.stdout).toContain('unknown command');
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
      const r = spawnSync(NODE, [SCRIPT, '--db', db, 'maintenance', 'init'], {
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
      const r = spawnSync(NODE, [SCRIPT, 'maintenance', 'init', '--db', db], { encoding: 'utf8', timeout: 5000 });
      expect(r.status).toBe(0);
      const parsed = JSON.parse(r.stdout) as Record<string, unknown>;
      expect(parsed['initialized']).toBe(true);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('removed flat commands fail with canonical replacements', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const removedMemory = run(db, ['get-memory', '--query', 'sqlite']);
      expect(removedMemory.status).toBe(1);
      expect(removedMemory.parsed?.['replacement']).toBe('memory recall');

      const removedView = run(db, ['view', 'all']);
      expect(removedView.status).toBe(1);
      expect(String(removedView.parsed?.['replacement'])).toContain('query all --format html');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('schema list maps to canonical CLI commands', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    const help = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(help.status).toBe(0);
    const commands: Record<string, string> = {
      tell_memory: 'memory record',
      get_memory: 'memory recall',
      pre_flight_intent: 'lock acquire',
      wait_for_lock: 'lock wait',
      prune_stale_locks: 'lock prune',
      release_file_lock: 'lock release',
      audit_unverified: 'verify audit',
      verify: 'verify mark',
      forget_memory: 'memory forget',
      refinement: 'refinement set',
      refine_query: 'refinement get',
      refine_delete: 'refinement delete',
      agent_registry: 'agent register',
      agent_signal: 'signal publish',
      signal_prune: 'signal prune',
      workspace_status: 'workspace status',
      export_harness: 'reflect export-harness',
      session_capture: 'session capture',
      mine_weakness: 'reflect mine-weakness',
      doc_staleness: 'docs staleness',
      docs_catalog: 'docs list',
      digest: 'maintenance digest',
      reflect: 'reflect record',
      attend: 'attend',
      query: 'query',
      repo_inject: 'repo inject',
    };
    const unsupported = ['stats', 'embed_index', 'harness_apply', 'memory_export', 'memory_import', 'memory_index', 'view', 'notify', 'notify_query', 'notify_resolve', 'notify_prune', 'status'];
    expect(listed).not.toEqual(expect.arrayContaining(unsupported));
    expect(listed).toEqual(expect.arrayContaining([
      'session_capture',
      'mine_weakness',
      'digest',
      'doc_staleness',
      'docs_catalog',
      'workspace_status',
      'export_harness',
      'audit_unverified',
    ]));
    for (const key of listed) {
      const command = commands[key];
      expect(command, `${key} should have canonical command mapping`).toBeTruthy();
      expect(help.stdout, `${key} should map to CLI command ${command}`).toContain(command);
    }
  });

  it('schema commands is the compact command-to-schema map for agents', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    const result = spawnSync(NODE, [schemaScript, 'commands', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(result.status).toBe(0);
    expect(result.stdout.trim().split('\n')).toHaveLength(1);
    const parsed = JSON.parse(result.stdout) as {
      ok: boolean;
      commands: Array<{ command: string; schema: string | null; use: string; example: string }>;
    };
    expect(parsed.ok).toBe(true);
    expect(parsed.commands).toEqual(expect.arrayContaining([
      expect.objectContaining({ command: 'workspace status', schema: 'workspace_status' }),
      expect.objectContaining({ command: 'lock acquire', schema: 'pre_flight_intent' }),
      expect.objectContaining({ command: 'schema commands', schema: null }),
    ]));
    for (const row of parsed.commands) {
      expect(row.command).not.toMatch(/tell-memory|get-memory|notify|agent-registry|pre-flight/);
      expect(row.use.length).toBeGreaterThan(8);
      expect(row.example).toContain('octocode-awareness ');
    }
  });

  it('every command in schema commands has focused help or is schema/hook utility', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    const result = spawnSync(NODE, [schemaScript, 'commands', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(result.status).toBe(0);
    const parsed = JSON.parse(result.stdout) as { commands: Array<{ command: string }> };
    const utilityPrefixes = ['schema ', 'hook run', 'hooks '];
    for (const row of parsed.commands) {
      if (utilityPrefixes.some((prefix) => row.command.startsWith(prefix))) continue;
      const help = spawnSync(NODE, [SCRIPT, ...row.command.split(' '), '--help'], { encoding: 'utf8', timeout: 5000 });
      expect(help.status, `${row.command} --help failed`).toBe(0);
      expect(help.stdout, `${row.command} help should mention its command`).toContain(row.command.split(' ')[0]);
      expect(help.stdout, `${row.command} help should not fall back to top-level help`).not.toContain('agent map: octocode-awareness schema commands --compact');
      expect(help.stdout, `${row.command} help should show schema or example`).toMatch(/schema:|example:/);
    }
  });

  it('generated skill CLI delegates schema commands to its sibling schema script', () => {
    expect(existsSync(SKILL_SCRIPT), 'generated awareness.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [SKILL_SCRIPT, 'schema', 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status, schema.stderr || schema.stdout).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    expect(listed).toContain('get_memory');
  });

  it('every listed schema resolves and its example validates', { timeout: 30_000 }, () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'list'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const listed = JSON.parse(schema.stdout) as string[];
    for (const key of listed) {
      const jsonSchema = spawnSync(NODE, [schemaScript, 'json-schema', key], { encoding: 'utf8', timeout: 5000 });
      expect(jsonSchema.status, `${key} json-schema failed: ${jsonSchema.stderr || jsonSchema.stdout}`).toBe(0);

      const example = spawnSync(NODE, [schemaScript, 'example', key], { encoding: 'utf8', timeout: 5000 });
      expect(example.status, `${key} example failed: ${example.stderr || example.stdout}`).toBe(0);
      expect(() => JSON.parse(example.stdout)).not.toThrow();

      const validated = spawnSync(NODE, [schemaScript, 'validate', key, '-'], {
        encoding: 'utf8',
        input: example.stdout,
        timeout: 5000,
      });
      expect(validated.status, `${key} example should validate: ${validated.stdout || validated.stderr}`).toBe(0);
    }
  });

  it('memory label schema stays aligned with runtime labels', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'tell_memory'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    expect(schema.stdout).toContain('"EXPERIENCE"');
    expect(schema.stdout).toContain('"OVERRIDE"');
  });

  it('schema exposes only implemented memory recall options', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'get_memory'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    const sortSchema = parsed.properties['sort'];
    expect(sortSchema).toBeDefined();
    expect(sortSchema?.['enum']).toEqual(['smart', 'score', 'importance', 'recent', 'accessed']);
    expect(parsed.properties).not.toHaveProperty('no_decay');
    expect(parsed.properties).not.toHaveProperty('half_life');
  });

  it('schema aligns pre-flight ttl and retry contract with CLI/runtime', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'pre_flight_intent'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    const ttlSchema = parsed.properties['ttl_minutes'];
    const ttlSecondsSchema = parsed.properties['ttl_seconds'];
    const retrySchema = parsed.properties['retry_interval'];
    expect(ttlSchema).toBeDefined();
    expect(ttlSecondsSchema).toBeDefined();
    expect(retrySchema).toBeDefined();
    expect(ttlSchema?.['default']).toBe(10);
    expect(ttlSchema?.['maximum']).toBe(10);
    expect(ttlSecondsSchema?.['maximum']).toBe(600);
    expect(retrySchema?.['default']).toBe(5);
  });

  it('schema covers runtime drift cases for verify, audit, and handoff refinements', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);

    const verify = spawnSync(NODE, [schemaScript, 'json-schema', 'verify'], { encoding: 'utf8', timeout: 5000 });
    expect(verify.status).toBe(0);
    const verifySchema = JSON.parse(verify.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(verifySchema.properties['status']?.['enum']).toEqual(['SUCCESS', 'FAILED']);

    const audit = spawnSync(NODE, [schemaScript, 'json-schema', 'audit_unverified'], { encoding: 'utf8', timeout: 5000 });
    expect(audit.status).toBe(0);
    expect(audit.stdout).toContain('"abandon"');

    const refinement = spawnSync(NODE, [schemaScript, 'json-schema', 'refinement'], { encoding: 'utf8', timeout: 5000 });
    expect(refinement.status).toBe(0);
    const refinementSchema = JSON.parse(refinement.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(refinementSchema.properties['quality']?.['enum']).toEqual(['good', 'bad', 'handoff']);
  });

  it('schema exposes implemented forget scope filters', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'forget_memory'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(parsed.properties['workspace_path']).toBeDefined();
    expect(parsed.properties['artifact']).toBeDefined();
    expect(parsed.properties['repo']).toBeDefined();
    expect(parsed.properties['ref']).toBeDefined();
  });

  it('supports canonical noun/verb CLI commands', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, ['maintenance', 'init']);
      const record = ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--task-context', 'canonical command',
        '--observation', 'noun verb memory works',
        '--importance', '6',
      ]);
      expect((record['memory'] as Record<string, unknown>)['memory_id']).toBeTruthy();

      const recall = ok(db, ['memory', 'recall', '--query', 'noun verb memory', '--limit', '1']);
      expect((recall['memories'] as unknown[]).length).toBeGreaterThan(0);
    } finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── query markdown ───────────────────────────────────────────────────────────

describe('query memories markdown', () => {
  it('includes provenance references and failure signatures', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'memory', 'record', '--agent-id', 'a',
        '--task-context', 'indexed provenance',
        '--observation', 'memory index should preserve source context',
        '--importance', '8',
        '--label', 'GOTCHA',
        '--workspace', dir,
        '--tag', 'index',
        '--reference', 'file:packages/octocode-awareness/src/memory.ts',
        '--reference', 'pr:owner/repo#123',
        '--failure-signature', 'mechanism:index|cause:lost-provenance',
      ]);

      const result = run(db, ['query', 'memories', '--workspace', dir, '--format', 'markdown']);
      expect(result.status).toBe(0);
      expect(result.stdout).toContain('indexed provenance');
      expect(result.stdout).toContain('memory index should preserve source context');
      expect(result.stdout).toContain('mechanism:index|cause:lost-provenance');
    } finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── repo context query/inject ────────────────────────────────────────────────

describe('repo context projections', () => {
  it('queries awareness views as JSON and CSV', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--task-context', 'repo context gotcha',
        '--observation', 'repo context should preserve gotchas for generated files',
        '--importance', '8',
        '--label', 'GOTCHA',
        '--file', 'src/context.ts',
      ]);

      const json = ok(db, ['query', 'gotchas', '--workspace', dir, '--limit', '10']);
      expect(json['view']).toBe('gotchas');
      expect(json['count']).toBe(1);
      const rows = json['rows'] as Array<Record<string, unknown>>;
      expect(rows[0]?.['label']).toBe('GOTCHA');
      expect(rows[0]?.['references']).toContain(`file:${join(dir, 'src/context.ts')}`);

      const csv = run(db, ['query', 'gotchas', '--workspace', dir, '--format', 'csv']);
      expect(csv.status).toBe(0);
      expect(csv.stdout).toContain('memory_id,label,importance');
      expect(csv.stdout).toContain('repo context should preserve gotchas');

      ok(db, [
        'lock', 'acquire',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--target-file', join(dir, 'src/context.ts'),
        '--rationale', 'verify context file',
        '--test-plan', 'vitest context',
      ]);
      const release = run(db, [
        'lock', 'release',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--target-file', join(dir, 'src/context.ts'),
        '--status', 'SUCCESS',
      ]);
      expect(release.status).toBe(2);

      const workboard = ok(db, ['query', 'workboard', '--workspace', dir, '--limit', '10']);
      expect(workboard['view']).toBe('workboard');
      expect(workboard['rows']).toEqual(expect.arrayContaining([
        expect.objectContaining({ column: 'Verify', item_type: 'task' }),
      ]));
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('returns a compact attend packet with drive state', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--task-context', 'attend auth design',
        '--observation', 'attend should select evidence and keep drive state derived',
        '--importance', '8',
        '--label', 'DECISION',
        '--reference', `file:${join(dir, 'src/auth.ts')}`,
      ]);
      ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--kind', 'decision',
        '--subject', 'attend signal',
        '--body', 'route through workboard',
      ]);

      const result = ok(db, [
        'attend',
        '--workspace', dir,
        '--query', 'attend auth design',
        '--file', join(dir, 'src/auth.ts'),
        '--limit', '10',
        '--explain-organ',
        '--compact',
      ]);
      expect(result['schema_version']).toBe(1);
      expect(result['profile']).toMatchObject({ active_memories: 1 });
      expect(result['workboard']).toMatchObject({ Inbox: expect.any(Array) });
      expect(result['evidence']).toEqual(expect.arrayContaining([
        expect.objectContaining({ kind: 'memory', trust: 'verified_lead' }),
      ]));
      expect(result['drive_state']).toMatchObject({
        goal: 'attend auth design',
        team_norms: expect.arrayContaining(['evidence-first']),
      });
      expect(result['organ_reference']).toEqual(expect.arrayContaining([
        expect.objectContaining({ organ: 'attention' }),
      ]));
      expect(String(result['next'])).toMatch(/lock acquire|verify audit|memory forget|attend --workspace/);
      expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThanOrEqual(8 * 1024);
      expect(JSON.stringify(result)).not.toMatch(/fictional persistent personality/i);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('writes a static HTML awareness view', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const out = join(dir, '.octocode', 'awareness', 'index.html');
    try {
      ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--task-context', 'html awareness',
        '--observation', 'query html output writes browser-readable awareness',
        '--importance', '6',
      ]);

      const result = ok(db, ['query', 'all', '--workspace', dir, '--format', 'html', '--out', out]);
      expect(result['path']).toBe(out);
      expect(readFileSync(out, 'utf8')).toContain('Octocode Awareness: all');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('resolves relative query output paths against the requested workspace', () => {
    const dir = mktemp();
    const cwdDir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const relOut = join('.octocode', 'awareness', 'csv', 'memories.csv');
    const workspaceOut = join(dir, relOut);
    try {
      ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--task-context', 'csv awareness',
        '--observation', 'query csv output stays with the requested workspace',
        '--importance', '6',
      ]);

      // Relative --out must resolve against --workspace, not process cwd.
      // Use memories (row CSV) rather than all (section-count index).
      const result = ok(db, ['query', 'memories', '--workspace', dir, '--format', 'csv', '--out', relOut], { cwd: cwdDir });
      expect(result['path']).toBe(workspaceOut);
      expect(readFileSync(workspaceOut, 'utf8')).toContain('query csv output');
      expect(existsSync(join(cwdDir, relOut))).toBe(false);
    } finally {
      rmSync(dir, { recursive: true });
      rmSync(cwdDir, { recursive: true });
    }
  });

  it('injects .octocode repo context without editing gitignore', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      writeFileSync(join(dir, '.gitignore'), '.octocode\n', 'utf8');
      spawnSync('git', ['init', '-q'], { cwd: dir, encoding: 'utf8', timeout: 5000 });
      ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--task-context', 'inject projection',
        '--observation', 'inject writes markdown csv html and manifest',
        '--importance', '7',
        '--label', 'DECISION',
        '--reference', 'https://example.com/inject-guide',
        '--reference', 'repo:bgauryy/octocode-mcp',
      ]);

      const result = ok(db, ['repo', 'inject', '--workspace', dir, '--mode', 'share']);
      expect(result['out_dir']).toBe(join(dir, '.octocode'));
      expect(result['files']).toEqual(expect.arrayContaining([
        join(dir, '.octocode', 'AGENTS.md'),
        join(dir, '.octocode', 'BOOKMARKS.md'),
        join(dir, '.octocode', 'awareness', 'manifest.json'),
      ]));
      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as Record<string, unknown>;
      expect(manifest['policy']).toMatchObject({ gitignore_modified: false, share_decision: 'user-owned' });
      expect(manifest['budgets']).toMatchObject({
        markdown: {
          'AGENTS.md': { max_lines: 80, within_budget: true },
        },
      });
      expect(manifest['warnings']).toEqual(expect.arrayContaining([
        expect.stringContaining('gitignored'),
      ]));
      expect(readFileSync(join(dir, '.gitignore'), 'utf8')).toBe('.octocode\n');
      expect(readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8')).toContain('Octocode Awareness Map');
      expect(readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8')).toContain('Wiki And Memory Map');
      expect(readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8')).toContain('Projection Health');
      expect(readFileSync(join(dir, '.octocode', 'BOOKMARKS.md'), 'utf8')).toContain('https://example.com/inject-guide');
      expect(readFileSync(join(dir, '.octocode', 'awareness', 'csv', 'lessons.csv'), 'utf8')).toContain('DECISION');
    } finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── digest ──────────────────────────────────────────────────────────────────

describe('digest', () => {
  it('uses documented retention flags in dry-run mode', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const oldMemory = ok(db, [
        'memory', 'record', '--agent-id', 'a',
        '--task-context', 'old superseded',
        '--observation', 'old superseded observation',
        '--importance', '3',
      ]);
      const freshMemory = ok(db, [
        'memory', 'record', '--agent-id', 'a',
        '--task-context', 'fresh superseded',
        '--observation', 'fresh superseded observation',
        '--importance', '3',
      ]);
      const oldHandoff = ok(db, [
        'refinement', 'set', '--agent-id', 'a',
        '--reasoning', 'old handoff',
        '--remember', 'old handoff',
        '--quality', 'handoff',
        '--state', 'open',
      ]);
      const freshHandoff = ok(db, [
        'refinement', 'set', '--agent-id', 'a',
        '--reasoning', 'fresh handoff',
        '--remember', 'fresh handoff',
        '--quality', 'handoff',
        '--state', 'open',
      ]);
      const oldDone = ok(db, [
        'refinement', 'set', '--agent-id', 'a',
        '--reasoning', 'old done',
        '--remember', 'old done',
        '--quality', 'good',
        '--state', 'done',
      ]);
      const freshDone = ok(db, [
        'refinement', 'set', '--agent-id', 'a',
        '--reasoning', 'fresh done',
        '--remember', 'fresh done',
        '--quality', 'bad',
        '--state', 'done',
      ]);

      const conn = new DatabaseSync(db);
      try {
        conn.prepare("UPDATE memories SET state = 'SUPERSEDED', updated_at = ? WHERE memory_id = ?")
          .run(daysAgo(5), (oldMemory['memory'] as Record<string, unknown>)['memory_id'] as string);
        conn.prepare("UPDATE memories SET state = 'SUPERSEDED', updated_at = ? WHERE memory_id = ?")
          .run(daysAgo(0), (freshMemory['memory'] as Record<string, unknown>)['memory_id'] as string);
        conn.prepare('UPDATE refinements SET updated_at = ? WHERE refinement_id = ?')
          .run(daysAgo(5), (oldHandoff['refinement'] as Record<string, unknown>)['refinement_id'] as string);
        conn.prepare('UPDATE refinements SET updated_at = ? WHERE refinement_id = ?')
          .run(daysAgo(0), (freshHandoff['refinement'] as Record<string, unknown>)['refinement_id'] as string);
        conn.prepare('UPDATE refinements SET updated_at = ? WHERE refinement_id = ?')
          .run(daysAgo(5), (oldDone['refinement'] as Record<string, unknown>)['refinement_id'] as string);
        conn.prepare('UPDATE refinements SET updated_at = ? WHERE refinement_id = ?')
          .run(daysAgo(0), (freshDone['refinement'] as Record<string, unknown>)['refinement_id'] as string);
      } finally {
        conn.close();
      }

      const defaults = ok(db, ['maintenance', 'digest', '--dry-run']);
      expect(defaults['would_prune_old']).toBe(0);
      expect(defaults['would_prune_refinements']).toBe(0);

      const result = ok(db, [
        'maintenance', 'digest', '--dry-run',
        '--retention-days', '3',
        '--refinement-handoff-retention-days', '1',
        '--refinement-done-retention-days', '2',
      ]);
      expect(result['dry_run']).toBe(true);
      expect(result['would_prune_old']).toBe(1);
      expect(result['would_prune_refinements']).toBe(2);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('exports memory docs with provenance references', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const report = join(dir, 'MEMORY.md');
    try {
      ok(db, [
        'memory', 'record', '--agent-id', 'a',
        '--task-context', 'digest provenance export',
        '--observation', 'digest export should keep memory references',
        '--importance', '8',
        '--reference', 'file:/tmp/digest-provenance.ts',
        '--workspace', dir,
      ]);

      const result = ok(db, ['maintenance', 'digest', '--workspace', dir, '--export-doc', report]);
      expect(result['doc_path']).toBe(report);
      expect(readFileSync(report, 'utf8')).toContain('**References:** file:/tmp/digest-provenance.ts');
    } finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── edge cases ───────────────────────────────────────────────────────────────

describe('edge cases', () => {
  it('WAL mode allows 3 rapid sequential memory record calls', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      for (let i = 0; i < 3; i++) {
        ok(db, [
          'memory', 'record', '--agent-id', `a${i}`,
          '--task-context', `ctx${i}`, '--observation', `obs${i}`,
          '--importance', '5',
        ]);
      }
      const result = ok(db, ['workspace', 'status']);
      expect(result['memory_count']).toBe(3);
    } finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── signal ──────────────────────────────────────────────────────────────────

describe('signal', () => {
  it('preserves publish kind while still allowing kind filters', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--to-agent', 'agent-b',
        '--kind', 'question',
        '--subject', 'Kind check',
        '--workspace', dir,
      ]);

      const listed = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--kind', 'question',
        '--workspace', dir,
      ]);
      expect(listed['count']).toBe(1);
      const signal = (listed['signals'] as Record<string, unknown>[])[0]!;
      expect(signal['kind']).toBe('question');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('round-trips reply, ack, and resolve through the CLI', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const published = ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--to-agent', 'agent-b',
        '--kind', 'question',
        '--subject', 'Need answer',
        '--workspace', dir,
      ]);
      const parentId = published['signal_id'] as string;
      const threadId = published['thread_id'] as string;

      const reply = ok(db, [
        'signal', 'reply',
        '--agent-id', 'agent-b',
        '--to-agent', 'agent-a',
        '--subject', 'Answer',
        '--body', 'Done',
        '--in-reply-to', parentId,
        '--workspace', dir,
      ]);
      const replyId = reply['signal_id'] as string;
      expect(reply['thread_id']).toBe(threadId);
      expect(replyId).not.toBe(parentId);

      const acked = ok(db, [
        'signal', 'ack',
        '--agent-id', 'agent-a',
        '--signal-id', replyId,
        '--workspace', dir,
      ]);
      expect(acked['acknowledged']).toBe(1);
      expect(acked['signal_ids']).toEqual([replyId]);

      const resolved = ok(db, [
        'signal', 'resolve',
        '--agent-id', 'agent-a',
        '--thread-id', threadId,
        '--workspace', dir,
      ]);
      expect(resolved['resolved']).toBe(2);
      expect(resolved['signal_ids']).toEqual(expect.arrayContaining([parentId, replyId]));

      const listed = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-a',
        '--thread-id', threadId,
        '--all',
        '--workspace', dir,
      ]);
      expect(listed['count']).toBe(2);
      expect((listed['signals'] as Record<string, unknown>[]).every((s) => s['status'] === 'resolved')).toBe(true);
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
      const a = ok(db, ['lock', 'acquire', '--agent-id', 'agent-a', '--target-file', tf]);
      const taskId = (a['task'] as Record<string, unknown>)['task_id'] as string;

      const blocked = run(db, ['lock', 'acquire', '--agent-id', 'agent-b', '--target-file', tf]);
      expect(blocked.status).toBe(2);

      ok(db, ['lock', 'release', '--agent-id', 'agent-a', '--task-id', taskId, '--status', 'PENDING']);
      const reclaim = ok(db, ['lock', 'acquire', '--agent-id', 'agent-b', '--target-file', tf]);
      expect((reclaim['task'] as Record<string, unknown>)['agent_id']).toBe('agent-b');
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
      expect(docs.some((doc) => doc['name'] === 'full-flow')).toBe(true);
      expect(docs.every((doc) => doc['kind'] === 'skill-ref')).toBe(true);
      expect(String(listed['next'])).toContain('docs show');
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
