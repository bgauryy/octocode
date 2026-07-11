/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
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

  it('invalid outcome hard-errors', () => {
    fail(db, ['reflect', 'record', '--agent-id', 'a', '--task', 'task', '--outcome', 'INVALID']);
  });

  it('invalid memory label hard-errors', () => {
    fail(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 't', '--observation', 'o',
      '--importance', '5', '--label', 'BOGUSLABEL',
    ]);
  });

  it('invalid signal kind hard-errors', () => {
    fail(db, [
      'signal', 'publish', '--agent-id', 'a', '--kind', 'not-a-kind', '--subject', 's', '--body', 'b',
      '--workspace', process.cwd(),
    ]);
  });

  it('fix_harness sets harness_fix=true', () => {
    const result = ok(db, [
      'reflect', 'record', '--agent-id', 'a', '--task', 'harness', '--outcome', 'partial',
      '--fix-harness', 'improve retry logic',
    ]);
    expect(result['harness_fix']).toBe(true);
  });

  it('fix-instructions creates developer-review feedback', () => {
    const result = ok(db, [
      'reflect', 'record', '--agent-id', 'a', '--task', 'instructions', '--outcome', 'partial',
      '--fix-instructions', 'clarify hook install flow',
    ]);
    expect(result['instructions_feedback']).toBe(true);
    expect(result['developer_review_refinement_id']).toMatch(/^ref_/);
  });

  it('includes canonical next commands that close the reflection loop', () => {
    const result = ok(db, ['reflect', 'record', '--agent-id', 'a', '--task', 't', '--outcome', 'worked']);
    expect(typeof result['next']).toBe('string');
    expect(result['next']).toContain('octocode-awareness refinement get');
    expect(result['next']).toContain('octocode-awareness reflect mine-weakness');
    expect(result['next']).not.toContain('memory_refine_get');
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
