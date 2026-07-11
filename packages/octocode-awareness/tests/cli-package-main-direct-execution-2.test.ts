/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
const INDEX_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/index.js');
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

describe('package entry separation', () => {
  it('keeps out/index.js import-only and exposes the CLI separately', () => {
    const result = spawnSync(NODE, [
      '--input-type=module',
      '--eval',
      `const m = await import(${JSON.stringify(INDEX_SCRIPT)}); if (typeof m.getMemory !== 'function') process.exit(1);`,
    ], { encoding: 'utf8', timeout: 10000 });
    expect(result.status, `stderr=${result.stderr} stdout=${result.stdout}`).toBe(0);
    expect(result.stdout).toBe('');
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
      'memory', 'record', '--agent-id', 'a', '--task-context', 'tag normalization', '--observation', 'tag normalization observation',
      '--importance', '5', '--tag', 'FOO', '--tag', 'bar-baz',
    ]);
    expect((result['memory'] as Record<string, unknown>)['tags']).toEqual(['foo', 'bar-baz']);
  });

  it('references are stored', () => {
    const result = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'reference storage', '--observation', 'reference storage observation',
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

  it('unknown label hard-errors', () => {
    fail(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'compact output', '--observation', 'compact output observation',
      '--importance', '5', '--label', 'NOTAREAL',
    ]);
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

  it('archives and restores a memory without reviving replaced history', () => {
    const first = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'archive lifecycle',
      '--observation', 'reversible archived memory', '--importance', '5', '--allow-similar',
    ]);
    const memoryId = (first['memory'] as Record<string, unknown>)['memory_id'] as string;

    const preview = ok(db, ['memory', 'archive', '--memory-id', memoryId, '--dry-run']);
    expect(preview).toMatchObject({ archived: 0, dry_run: true, would_archive: 1, memory_ids: [memoryId] });
    expect(ok(db, ['memory', 'archive', '--memory-id', memoryId])).toMatchObject({ archived: 1, memory_ids: [memoryId] });

    const hidden = ok(db, ['memory', 'recall', '--query', 'reversible archived memory', '--min-importance', '1']);
    expect((hidden['memories'] as Array<Record<string, unknown>>).map(memory => memory['memory_id'])).not.toContain(memoryId);
    const archived = ok(db, ['memory', 'recall', '--state', 'SUPERSEDED', '--query', 'reversible archived memory', '--min-importance', '1', '--full']);
    expect((archived['memories'] as Array<Record<string, unknown>>)[0]?.['expired_at']).toBeTruthy();

    const restorePreview = ok(db, ['memory', 'restore', '--memory-id', memoryId, '--dry-run']);
    expect(restorePreview).toMatchObject({ restored: 0, dry_run: true, would_restore: 1, memory_ids: [memoryId] });
    expect(ok(db, ['memory', 'restore', '--memory-id', memoryId])).toMatchObject({ restored: 1, memory_ids: [memoryId] });
    const restored = ok(db, ['memory', 'recall', '--query', 'reversible archived memory', '--min-importance', '1']);
    expect((restored['memories'] as Array<Record<string, unknown>>).map(memory => memory['memory_id'])).toContain(memoryId);

    const replacement = ok(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'archive replacement',
      '--observation', 'newer replacement', '--importance', '6', '--supersedes', memoryId,
    ]);
    expect(replacement['superseded']).toContain(memoryId);
    expect(ok(db, ['memory', 'restore', '--memory-id', memoryId, '--dry-run'])).toMatchObject({ would_restore: 0 });
  });

  it('memory record help exposes the correction and dedupe contract', () => {
    const help = spawnSync(NODE, [SCRIPT, 'memory', 'record', '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(help.status).toBe(0);
    expect(help.stdout).toContain('--supersedes <id>');
    expect(help.stdout).toContain('--allow-similar');
  });

  it('importance out of range exits 1', () => {
    fail(db, [
      'memory', 'record', '--agent-id', 'a', '--task-context', 'duplicate tags', '--observation', 'duplicate tag normalization observation',
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
      'memory', 'record', '--agent-id', 'a', '--task-context', 'duplicate tag fixture', '--observation', 'deduplicate repeated tag values',
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
      'memory', 'record', '--agent-id', 'a', '--task-context', 'duplicate tag assertion', '--observation', 'the tag list should contain one unique value',
      '--importance', '5', '--tag', 'dup', '--tag', 'dup', '--tag', 'dup',
    ]);
    expect((result['memory'] as Record<string, unknown>)['tags']).toEqual(['dup']);
  });
});
