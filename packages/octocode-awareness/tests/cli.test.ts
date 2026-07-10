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
const SOURCE_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../bin/awareness.ts');
const TSX_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../../../node_modules/tsx/dist/cli.mjs');
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
    // repo inject / heavy CLI paths can exceed 10s on cold machines
    timeout: 30000,
  });
  let parsed: Record<string, unknown> | null = null;
  try { parsed = JSON.parse(result.stdout) as Record<string, unknown>; } catch { /* non-JSON */ }
  return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}

function runSource(
  args: string[],
  opts: { cwd?: string; env?: NodeJS.ProcessEnv } = {},
): RunResult {
  const result = spawnSync(NODE, [TSX_SCRIPT, SOURCE_SCRIPT, ...args], {
    cwd: opts.cwd ?? process.cwd(),
    env: opts.env ?? process.env,
    encoding: 'utf8',
    timeout: 30000,
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

describe('source CLI regressions', () => {
  it('hard-errors on a missing --db path before opening the canonical store', () => {
    const dir = mktemp();
    try {
      const result = runSource(['workspace', 'status', '--db', '--compact'], {
        env: { ...process.env, OCTOCODE_MEMORY_HOME: dir },
      });
      expect(result.status).toBe(1);
      expect(String(result.parsed?.['error'])).toContain('--db expects a path');
      expect(existsSync(join(dir, 'awareness.sqlite3'))).toBe(false);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('rejects --no-* for scalar flags while preserving boolean negation', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const scalar = runSource([
        '--db', db,
        'memory', 'record',
        '--no-task-context',
        '--observation', 'must not be recorded',
        '--importance', '5',
        '--compact',
      ]);
      expect(scalar.status).toBe(1);
      expect(String(scalar.parsed?.['error'])).toMatch(/--no-task-context.*expects a value/);

      const boolean = runSource([
        '--db', db,
        'memory', 'recall',
        '--query', 'none',
        '--no-smart',
        '--compact',
      ]);
      expect(boolean.status, boolean.stderr || boolean.stdout).toBe(0);
      expect(boolean.parsed?.['ok']).toBe(true);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('rejects missing values for lifecycle selectors and accepts boolean digest export', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      for (const args of [
        ['memory', 'record', '--task-context', 'ctx', '--observation', 'obs', '--importance', '5', '--supersedes', '--compact'],
        ['memory', 'recall', '--regex', '--compact'],
        ['memory', 'recall', '--file-regex', '--compact'],
        ['memory', 'forget', '--tags', '--compact'],
      ]) {
        const result = runSource(['--db', db, ...args]);
        expect(result.status, result.stdout).toBe(1);
        expect(String(result.parsed?.['error'])).toContain('expects a value');
      }

      const digest = runSource(['--db', db, 'maintenance', 'digest', '--dry-run', '--export-doc', '--compact']);
      expect(digest.status, digest.stderr || digest.stdout).toBe(0);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('forwards schema --examples and rejects repo inject --include-bodies', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const schema = runSource(['schema', 'commands', '--examples', '--compact']);
      expect(schema.status, schema.stderr || schema.stdout).toBe(0);
      const commands = schema.parsed?.['commands'] as Array<Record<string, unknown>>;
      expect(commands.length).toBeGreaterThan(10);
      expect(commands[0]).toHaveProperty('example');

      const inject = runSource([
        '--db', db,
        'repo', 'inject',
        '--workspace', dir,
        '--include-bodies',
        '--compact',
      ]);
      expect(inject.status).toBe(1);
      expect(String(inject.parsed?.['error'])).toContain('unknown flag(s): --include-bodies');
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('uses OCTOCODE_AGENT_ID for session capture and lock wait', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const env = { ...process.env, OCTOCODE_AGENT_ID: 'agent-from-env' };
    try {
      const started = runSource([
        '--db', db,
        'work', 'start',
        '--workspace', dir,
        '--file', 'src/session.ts',
        '--rationale', 'capture environment identity',
        '--test-plan', 'focused CLI test',
        '--compact',
      ], { env });
      expect(started.status, started.stderr || started.stdout).toBe(0);

      const capture = runSource([
        '--db', db,
        'session', 'capture',
        '--workspace', dir,
        '--compact',
      ], { env });
      expect(capture.status, capture.stderr || capture.stdout).toBe(0);
      expect(capture.parsed?.['active_runs']).toBe(1);
      expect(capture.parsed?.['captured']).toBe(true);

      const conn = new DatabaseSync(db);
      try {
        const row = conn.prepare("SELECT agent_id FROM refinements WHERE quality = 'handoff'").get() as { agent_id: string };
        expect(row.agent_id).toBe('agent-from-env');
      } finally {
        conn.close();
      }

      const acquired = runSource([
        '--db', db,
        'lock', 'acquire',
        '--workspace', dir,
        '--target-file', 'src/locked.ts',
        '--rationale', 'own lock',
        '--compact',
      ], { env });
      expect(acquired.status, acquired.stderr || acquired.stdout).toBe(0);

      const waited = runSource([
        '--db', db,
        'lock', 'wait',
        '--workspace', dir,
        '--file', 'src/locked.ts',
        '--wait-seconds', '0',
        '--compact',
      ], { env });
      expect(waited.status, waited.stderr || waited.stdout).toBe(0);
      expect(waited.parsed?.['lock_free']).toBe(true);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('requires --file for work show and validates signal importance and limit', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const show = runSource(['--db', db, 'work', 'show', '--workspace', dir, '--compact']);
      expect(show.status).toBe(1);
      expect(String(show.parsed?.['error'])).toContain('requires exactly one --file');
      const multiShow = runSource([
        '--db', db, 'work', 'show', '--workspace', dir,
        '--file', 'src/a.ts', '--file', 'src/b.ts', '--compact',
      ]);
      expect(multiShow.status).toBe(1);
      expect(String(multiShow.parsed?.['error'])).toContain('requires exactly one --file');

      for (const value of ['0', '1.5', '11']) {
        const importance = runSource([
          '--db', db,
          'signal', 'publish',
          '--agent-id', 'agent-a',
          '--to-agent', 'agent-b',
          '--kind', 'fyi',
          '--subject', 'invalid importance',
          '--importance', value,
          '--compact',
        ]);
        expect(importance.status).toBe(1);
        expect(String(importance.parsed?.['error'])).toContain('--importance must be an integer between 1 and 10');
      }

      for (const value of ['0', '-1']) {
        const limit = runSource([
          '--db', db,
          'signal', 'list',
          '--agent-id', 'agent-b',
          '--limit', value,
          '--compact',
        ]);
        expect(limit.status).toBe(1);
        expect(String(limit.parsed?.['error'])).toContain('--limit must be a positive integer');
      }

      const published = runSource([
        '--db', db,
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--to-agent', 'agent-b',
        '--kind', 'fyi',
        '--subject', 'valid bounds',
        '--importance', '10',
        '--compact',
      ]);
      expect(published.status, published.stderr || published.stdout).toBe(0);
      const listed = runSource([
        '--db', db,
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--limit', '1',
        '--compact',
      ]);
      expect(listed.status, listed.stderr || listed.stdout).toBe(0);
      expect(listed.parsed?.['count']).toBe(1);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('filters the bounded semantic pool before top-k and records one final access', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const inside = join(dir, 'inside');
    const outside = join(dir, 'outside');
    mkdirSync(inside, { recursive: true });
    mkdirSync(outside, { recursive: true });
    const embedScript = join(dir, 'embed.mjs');
    writeFileSync(embedScript, `#!/usr/bin/env node
import { readFileSync } from 'node:fs';
const text = readFileSync(0, 'utf8');
const embedding = text.includes('outside-best') ? [1, 0]
  : text.includes('inside-candidate') ? [0.8, 0.6]
  : [1, 0];
process.stdout.write(JSON.stringify({ embedding, model: 'scope-test' }));
`, 'utf8');
    const env = { ...process.env, OCTOCODE_EMBED_CMD: `${NODE} ${embedScript}` };
    try {
      const outsideRecord = runSource([
        '--db', db, 'memory', 'record', '--agent-id', 'outside-agent',
        '--task-context', 'outside-best', '--observation', 'higher global cosine match',
        '--importance', '8', '--workspace', outside, '--compact',
      ], { env });
      expect(outsideRecord.status, outsideRecord.stderr || outsideRecord.stdout).toBe(0);
      const insideRecord = runSource([
        '--db', db, 'memory', 'record', '--agent-id', 'inside-agent',
        '--task-context', 'inside-candidate', '--observation', 'lower scoped cosine match',
        '--importance', '8', '--workspace', inside, '--compact',
      ], { env });
      expect(insideRecord.status, insideRecord.stderr || insideRecord.stdout).toBe(0);
      const insideId = ((insideRecord.parsed?.['memory'] as Record<string, unknown>)['memory_id']) as string;

      const recalled = runSource([
        '--db', db, 'memory', 'recall', '--query', 'semantic-query', '--semantic',
        '--workspace', inside, '--strict-scope', '--limit', '1', '--full', '--compact',
      ], { env });
      expect(recalled.status, recalled.stderr || recalled.stdout).toBe(0);
      expect(recalled.parsed?.['mode']).toBe('semantic');
      const memories = recalled.parsed?.['memories'] as Array<Record<string, unknown>>;
      expect(memories.map(memory => memory['memory_id'])).toEqual([insideId]);

      const conn = new DatabaseSync(db);
      try {
        expect(conn.prepare('SELECT access_count FROM memories WHERE memory_id = ?').get(insideId))
          .toEqual({ access_count: 1 });
      } finally {
        conn.close();
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
});

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

  it('defaults to lean memory projection and restores full rows with --full', () => {
    const lean = ok(db, ['memory', 'recall', '--query', 'sqlite', '--limit', '1', '--compact']);
    expect(lean['projection']).toBe('lean');
    const leanMem = (lean['memories'] as Record<string, unknown>[])[0]!;
    expect(leanMem['memory_id']).toMatch(/^mem_/);
    expect(leanMem).toHaveProperty('observation');
    expect(leanMem).not.toHaveProperty('access_count');
    expect(leanMem).not.toHaveProperty('decay_half_life_days');
    expect(leanMem).not.toHaveProperty('file_tree_fingerprint');

    const full = ok(db, ['memory', 'recall', '--query', 'sqlite', '--limit', '1', '--full', '--compact']);
    expect(full).not.toHaveProperty('projection');
    const fullMem = (full['memories'] as Record<string, unknown>[])[0]!;
    expect(fullMem).toHaveProperty('access_count');
    expect(fullMem).toHaveProperty('created_at');
    expect(fullMem).toHaveProperty('workspace_path');
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
    const result = ok(db, ['memory', 'recall', '--query', 'uniqueaccesstoken77', '--min-importance', '1', '--full']);
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

// ─── workspace status ─────────────────────────────────────────────────────────

describe('work CLI', () => {
  it('runs the standalone advisory lifecycle without creating an exclusive lock', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const started = ok(db, [
        'work', 'start',
        '--agent-id', 'agent-work',
        '--workspace', dir,
        '--file', 'src/a.ts',
        '--rationale', 'refactor parser',
        '--test-plan', 'parser tests',
        '--compact',
      ]);
      const runId = (started['run'] as Record<string, unknown>)['run_id'] as string;
      expect((started['run'] as Record<string, unknown>)['status']).toBe('ACTIVE');
      expect(started['peer_count']).toBe(0);

      const listed = ok(db, ['work', 'list', '--workspace', dir, '--compact']);
      expect(listed['count']).toBe(1);
      expect((listed['files'] as Record<string, unknown>[])[0]).toMatchObject({
        run_id: runId,
        agent_id: 'agent-work',
        exclusive: false,
      });

      const shown = ok(db, ['work', 'show', '--workspace', dir, '--file', 'src/a.ts', '--compact']);
      expect(shown['count']).toBe(1);
      ok(db, ['work', 'touch', '--agent-id', 'agent-work', '--run-id', runId, '--compact']);

      const ended = ok(db, ['work', 'end', '--agent-id', 'agent-work', '--run-id', runId, '--compact']);
      expect((ended['run'] as Record<string, unknown>)['status']).toBe('PENDING');
      const checkDb = new DatabaseSync(db);
      try {
        expect(checkDb.prepare('SELECT COUNT(*) AS count FROM locks').get()).toEqual({ count: 0 });
      } finally {
        checkDb.close();
      }

      const verified = ok(db, [
        'verify', 'mark', '--agent-id', 'agent-work', '--run-id', runId,
        '--message', 'parser tests passed', '--compact',
      ]);
      expect(verified['status']).toBe('SUCCESS');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('caps compact work mutation file details while preserving exact totals', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const args = [
        'work', 'start', '--agent-id', 'agent-many-files', '--workspace', dir,
        '--rationale', 'multi-file refactor', '--test-plan', 'focused tests', '--compact',
      ];
      for (let index = 0; index < 8; index++) args.push('--file', `src/file-${index}.ts`);
      const started = ok(db, args);
      const runId = (started['run'] as Record<string, unknown>)['run_id'] as string;
      const ended = run(db, [
        'work', 'end', '--agent-id', 'agent-many-files', '--run-id', runId, '--compact',
      ]);
      expect(ended.status).toBe(0);
      expect(ended.parsed).toMatchObject({ file_count: 8, file_shown_count: 1, file_omitted_count: 7 });
      expect(ended.parsed?.['files']).toHaveLength(1);
      expect(Buffer.byteLength(ended.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);
      ok(db, ['verify', 'mark', '--agent-id', 'agent-many-files', '--run-id', runId,
        '--message', 'compact closeout fixture passed', '--compact']);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('uses OCTOCODE_AGENT_ID for a new Work run', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const result = spawnSync(NODE, [
        SCRIPT, '--db', db, 'work', 'start', '--workspace', dir,
        '--file', 'src/env.ts', '--rationale', 'env identity', '--test-plan', 'tests', '--compact',
      ], {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, OCTOCODE_AGENT_ID: 'agent-from-env' },
      });
      expect(result.status, result.stderr || result.stdout).toBe(0);
      const parsed = JSON.parse(result.stdout) as Record<string, unknown>;
      expect((parsed['run'] as Record<string, unknown>)['agent_id']).toBe('agent-from-env');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('rejects valueless identity/file flags and requires a file for show', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const missingAgent = fail(db, [
        'work', 'start', '--agent-id', '--file', 'src/a.ts',
        '--rationale', 'reason', '--test-plan', 'tests', '--compact',
      ]);
      expect(String(missingAgent?.['error'])).toMatch(/--agent-id expects a value/);

      const missingFileValue = fail(db, [
        'work', 'start', '--agent-id', 'agent-a', '--file',
        '--rationale', 'reason', '--test-plan', 'tests', '--compact',
      ]);
      expect(String(missingFileValue?.['error'])).toMatch(/--file expects a value/);

      const missingShowFile = fail(db, ['work', 'show', '--workspace', dir, '--compact']);
      expect(String(missingShowFile?.['error'])).toMatch(/requires exactly one --file/);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
});

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

  it('keeps compact lock status bounded and reports exact omissions', () => {
    const root = mktemp();
    const dbPath = join(root, 'test.sqlite3');
    try {
      for (let index = 0; index < 3; index++) {
        const file = join(root, `sensitive-${index}.txt`);
        writeFileSync(file, String(index));
        ok(dbPath, [
          'lock', 'acquire', '--agent-id', `agent-${index}`, '--workspace', root,
          '--target-file', file, '--rationale', `protect ${index}`, '--test-plan', `check ${index}`, '--compact',
        ]);
      }
      const compact = run(dbPath, ['workspace', 'status', '--workspace', root, '--compact']);
      expect(compact.status).toBe(0);
      expect(compact.parsed).toMatchObject({ lock_count: 3, lock_shown_count: 1, lock_omitted_count: 2 });
      expect(compact.parsed?.['locks']).toHaveLength(1);
      expect(Buffer.byteLength(compact.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);
    } finally { rmSync(root, { recursive: true, force: true }); }
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
      const executionRun = claimed['run'] as Record<string, unknown>;
      expect(executionRun['workspace_path']).toBe(expectedRoot);
      expect(executionRun['target_files']).toEqual([realpathSync(join(pkg, 'src/a.ts'))]);

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
      engines?: Record<string, string>;
      publishConfig?: Record<string, string>;
    };
    expect(pkg.name).toBe('@octocodeai/octocode-awareness');
    expect(pkg.publishConfig?.access).toBe('public');
    expect(pkg.bin?.['octocode-awareness']).toBe('./dist/bin/awareness.js');
    expect(pkg.engines?.['node']).toBe('>=22.13.0');
    expect(pkg.bin ?? {}).not.toHaveProperty('awareness');
  });

  it('--help exits 0', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('memory record');
    expect(r.stdout).toContain('<AGENT_INSTRUCTIONS>');
    expect(r.stdout).toContain('</AGENT_INSTRUCTIONS>');
    expect(r.stdout).toContain('BUNDLED SKILLS');
    expect(r.stdout).toContain('octocode-awareness');
    expect(r.stdout).toContain('octocode-skills');
    expect(r.stdout).toContain("Do not use a skill installer's registry/name lookup for awareness");
    expect(r.stdout).toContain('octocode-awareness schema commands --compact');
    expect(r.stdout).not.toContain('tell-memory');
    expect(r.stdout).not.toContain('get-memory');
    expect(r.stdout).not.toContain('<awareness-package>');
  });

  it('installed skill help resolves sibling bundled skills from the skill root', () => {
    const r = spawnSync(NODE, [SKILL_SCRIPT, '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    const skillsRoot = resolve(dirname(SKILL_SCRIPT), '..', '..');
    const awarenessSkill = resolve(skillsRoot, 'octocode-awareness');
    const skillsSkill = resolve(skillsRoot, 'octocode-skills');
    expect(r.stdout).toContain(awarenessSkill);
    expect(r.stdout).toContain(skillsSkill);
    expect(existsSync(awarenessSkill)).toBe(true);
    expect(existsSync(skillsSkill)).toBe(true);
  });

  it('no command prints the agent-instructions discovery guide', () => {
    const r = spawnSync(NODE, [SCRIPT], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('<AGENT_INSTRUCTIONS>');
    expect(r.stdout).toContain('BUNDLED SKILLS');
    expect(r.stdout).toContain('dist/skills/octocode-awareness');
    expect(r.stdout).toContain('dist/skills/octocode-skills');
    expect(r.stdout).toContain('FIRST COMMANDS');
    expect(r.stdout).not.toContain('<awareness-package>');
  });

  it('--help --compact returns a short agent guide', () => {
    const r = spawnSync(NODE, [SCRIPT, '--help', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
    expect(r.stdout).toContain('bundled-skills:');
    expect(r.stdout).toContain('dist/skills/octocode-awareness');
    expect(r.stdout).toContain('schema commands --compact');
    expect(r.stdout).toContain('refinement set|get|delete');
    expect(r.stdout).toMatch(/exits 0 ok/);
    expect(r.stdout).not.toContain('<awareness-package>');
    expect(r.stdout.split('\n').filter(Boolean).length).toBeLessThanOrEqual(8);
  });

  it('no command with --compact prints compact discovery instead of unknown-command JSON', () => {
    const r = spawnSync(NODE, [SCRIPT, '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain('canonical noun/verb CLI');
    expect(r.stdout).toContain('bundled-skills:');
    expect(r.stdout).toContain('dist/skills/octocode-awareness');
    expect(r.stdout).not.toContain('<awareness-package>');
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
      memory_lifecycle: 'memory archive',
      refinement: 'refinement set',
      refine_query: 'refinement get',
      refine_delete: 'refinement delete',
      agent_registry: 'agent register',
      agent_signal: 'signal publish',
      signal_prune: 'signal prune',
      workspace_status: 'workspace status',
      export_harness: 'reflect export-harness',
      developer_review: 'reflect developer-review',
      session_capture: 'session capture',
      mine_weakness: 'reflect mine-weakness',
      doc_staleness: 'docs staleness',
      docs_catalog: 'docs list',
      digest: 'maintenance digest',
      reflect: 'reflect record',
      attend: 'attend',
      query: 'query',
      repo_inject: 'repo inject',
      plan: 'plan create',
      task: 'task create',
      work: 'work start',
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
      commands: Array<{ command: string; schema: string | null; use?: string; example?: string }>;
    };
    expect(parsed.ok).toBe(true);
    expect(parsed.commands).toEqual(expect.arrayContaining([
      expect.objectContaining({ command: 'workspace status', schema: 'workspace_status' }),
      expect.objectContaining({ command: 'lock acquire', schema: 'pre_flight_intent' }),
      expect.objectContaining({ command: 'query files', schema: 'query' }),
      expect.objectContaining({ command: 'query all', schema: 'query' }),
      expect.objectContaining({ command: 'schema commands', schema: null }),
    ]));
    for (const row of parsed.commands) {
      expect(row.command).not.toMatch(/tell-memory|get-memory|notify|agent-registry|pre-flight/);
      expect(row).not.toHaveProperty('use');
      expect(row).not.toHaveProperty('example');
    }
    expect(Buffer.byteLength(result.stdout, 'utf8')).toBeLessThanOrEqual(5 * 1024);
  });

  it('schema commands --examples restores recipe lines', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    const result = spawnSync(NODE, [schemaScript, 'commands', '--examples', '--compact'], { encoding: 'utf8', timeout: 5000 });
    expect(result.status).toBe(0);
    const parsed = JSON.parse(result.stdout) as {
      commands: Array<{ command: string; use: string; example: string }>;
    };
    expect(parsed.commands.length).toBeGreaterThan(10);
    for (const row of parsed.commands) {
      expect(row.example).toContain('octocode-awareness ');
    }
    const lockRelease = parsed.commands.find((row) => row.command === 'lock release');
    expect(lockRelease?.example).toMatch(/--status PENDING/);
    expect(lockRelease?.example).not.toMatch(/--status ACTIVE/);
    const taskCreate = parsed.commands.find((row) => row.command === 'task create');
    expect(taskCreate?.example).toContain('--acceptance');
    const taskSubmit = parsed.commands.find((row) => row.command === 'task submit');
    expect(taskSubmit?.example).toContain('ready for verification');
    expect(taskSubmit?.example).not.toContain('tests pass');
    const workStart = parsed.commands.find((row) => row.command === 'work start');
    expect(workStart?.example).toContain('--workspace');
  });

  it('lock acquire help omits rejected --lock-type flag', () => {
    const help = spawnSync(NODE, [SCRIPT, 'lock', 'acquire', '--help'], { encoding: 'utf8', timeout: 5000 });
    expect(help.status).toBe(0);
    expect(help.stdout).not.toMatch(/\[--lock-type/);
    expect(help.stdout).toContain('exclusive protection');

    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const rejected = fail(db, [
        'lock', 'acquire',
        '--agent-id', 'agent-a',
        '--target-file', join(dir, 'x.ts'),
        '--rationale', 'probe',
        '--test-plan', 'none',
        '--lock-type', 'EXCLUSIVE',
        '--compact',
      ]);
      expect(String(rejected?.['error'] ?? '')).toContain('unknown flag(s): --lock-type');
    } finally {
      rmSync(dir, { recursive: true, force: true });
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

  it('attend schema exposes the agent identity used by CLI and skill guidance', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'attend'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    expect(parsed.properties).toHaveProperty('agent_id');
  });

  it('schema aligns pre-flight ttl and retry contract with CLI/runtime', () => {
    const schemaScript = resolve(dirname(fileURLToPath(import.meta.url)), '../skills/octocode-awareness/scripts/schema.mjs');
    expect(existsSync(schemaScript), 'generated schema.mjs must exist after build').toBe(true);
    const schema = spawnSync(NODE, [schemaScript, 'json-schema', 'pre_flight_intent'], { encoding: 'utf8', timeout: 5000 });
    expect(schema.status).toBe(0);
    const parsed = JSON.parse(schema.stdout) as { properties: Record<string, Record<string, unknown>> };
    const ttlSchema = parsed.properties['ttl_minutes'];
    const ttlSecondsSchema = parsed.properties['ttl_seconds'];
    const waitSchema = parsed.properties['wait_seconds'];
    const retrySchema = parsed.properties['retry_interval'];
    expect(ttlSchema).toBeDefined();
    expect(ttlSecondsSchema).toBeDefined();
    expect(waitSchema).toBeDefined();
    expect(retrySchema).toBeDefined();
    expect(ttlSchema?.['default']).toBe(10);
    expect(ttlSchema?.['maximum']).toBe(10);
    expect(ttlSecondsSchema?.['maximum']).toBe(600);
    expect(waitSchema?.['maximum']).toBe(3600);
    expect(retrySchema?.['default']).toBe(5);
    expect(retrySchema?.['maximum']).toBe(300);
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
    expect(refinementSchema.properties['quality']?.['enum']).toEqual(['good', 'bad', 'handoff', 'instructions']);
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
        '--status', 'PENDING',
      ]);
      expect(release.status).toBe(0);

      const workboard = ok(db, ['query', 'workboard', '--workspace', dir, '--limit', '10']);
      expect(workboard['view']).toBe('workboard');
      expect(workboard['rows']).toEqual(expect.arrayContaining([
        expect.objectContaining({ column: 'Verify', item_type: 'run' }),
      ]));
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('returns a compact attend packet with bounded counts and evidence', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      mkdirSync(join(dir, 'src'), { recursive: true });
      writeFileSync(join(dir, 'src/auth.ts'), 'export const auth = true;\n', 'utf8');
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
      expect(result['counts']).toMatchObject({ Inbox: 1, Ready: 0, Claimed: 0, Verify: 0, FilesUnderWork: 0 });
      expect(result['workboard']).toMatchObject({ Inbox: expect.any(Array) });
      expect(result['evidence']).toEqual(expect.arrayContaining([
        expect.objectContaining({ kind: 'memory', trust: 'verified_lead' }),
      ]));
      expect(result['profile']).toBeUndefined();
      expect(result['drive_state']).toBeUndefined();
      expect(result['organ_reference']).toBeUndefined();
      expect(String(result['next'])).toMatch(/work start|verify audit|query workboard|attend --workspace|signal list|work show|Continue claimed/);
      expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThanOrEqual(2 * 1024);
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
        '--allow-similar',
      ]);
      const oldHandoff = ok(db, [
        'refinement', 'set', '--agent-id', 'a',
        '--reasoning', 'old handoff',
        '--remember', 'old handoff',
        '--quality', 'handoff',
        '--state', 'open',
      ]);
      const oldClosedHandoff = ok(db, [
        'refinement', 'set', '--agent-id', 'a',
        '--reasoning', 'old closed handoff',
        '--remember', 'old closed handoff',
        '--quality', 'handoff',
        '--state', 'open',
      ]);
      const oldClosedHandoffId = (oldClosedHandoff['refinement'] as Record<string, unknown>)['refinement_id'] as string;
      ok(db, [
        'refinement', 'set', '--agent-id', 'a', '--refinement-id', oldClosedHandoffId,
        '--state', 'done', '--check-receipt', 'handoff consumed and verified',
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
        '--state', 'open',
      ]);
      const oldDoneId = (oldDone['refinement'] as Record<string, unknown>)['refinement_id'] as string;
      ok(db, [
        'refinement', 'set', '--agent-id', 'a', '--refinement-id', oldDoneId,
        '--state', 'done', '--check-receipt', 'old fixture verified',
      ]);
      const freshDone = ok(db, [
        'refinement', 'set', '--agent-id', 'a',
        '--reasoning', 'fresh done',
        '--remember', 'fresh done',
        '--quality', 'bad',
        '--state', 'open',
      ]);
      const freshDoneId = (freshDone['refinement'] as Record<string, unknown>)['refinement_id'] as string;
      ok(db, [
        'refinement', 'set', '--agent-id', 'a', '--refinement-id', freshDoneId,
        '--state', 'done', '--check-receipt', 'fresh fixture verified',
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
          .run(daysAgo(5), oldClosedHandoffId);
        conn.prepare('UPDATE refinements SET updated_at = ? WHERE refinement_id = ?')
          .run(daysAgo(0), (freshHandoff['refinement'] as Record<string, unknown>)['refinement_id'] as string);
        conn.prepare('UPDATE refinements SET updated_at = ? WHERE refinement_id = ?')
          .run(daysAgo(5), oldDoneId);
        conn.prepare('UPDATE refinements SET updated_at = ? WHERE refinement_id = ?')
          .run(daysAgo(0), freshDoneId);
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

      const applied = ok(db, [
        'maintenance', 'digest',
        '--retention-days', '3',
        '--refinement-handoff-retention-days', '1',
        '--refinement-done-retention-days', '2',
      ]);
      expect(applied['pruned_refinements']).toBe(2);
      const verifyConn = new DatabaseSync(db);
      try {
        const openId = (oldHandoff['refinement'] as Record<string, unknown>)['refinement_id'] as string;
        expect(verifyConn.prepare('SELECT state FROM refinements WHERE refinement_id = ?').get(openId)).toEqual({ state: 'open' });
        expect(verifyConn.prepare('SELECT state FROM refinements WHERE refinement_id = ?').get(oldClosedHandoffId)).toBeUndefined();
      } finally { verifyConn.close(); }
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('rejects every destructive retention window outside 1..3650', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      for (const flag of [
        '--retention-days',
        '--refinement-handoff-retention-days',
        '--refinement-done-retention-days',
        '--operational-retention-days',
        '--pressure-age-days',
      ]) {
        for (const value of ['0', '-1', '3651']) {
          const result = runSource(['--db', db, 'maintenance', 'digest', '--dry-run', flag, value, '--compact']);
          expect(result.status, `${flag}=${value}: ${result.stdout}`).toBe(1);
          expect(String(result.parsed?.['error'])).toContain(flag);
          expect(String(result.parsed?.['error'])).toContain('1..3650');
        }
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
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

  it('accepts signal list --format hook for host briefing shape', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--task-context', 'hook briefing memory',
        '--observation', 'selective reminder should surface in hook format',
        '--importance', '8',
        '--label', 'GOTCHA',
      ]);

      const briefing = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--workspace', dir,
        '--format', 'hook',
        '--compact',
      ]);
      expect(briefing).not.toHaveProperty('error');
      expect(
        briefing['additionalContext'] != null || Number(briefing['count'] ?? 0) === 0,
      ).toBe(true);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('summarizes signal list bodies unless --include-bodies', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const longBody = `Long signal body ${'x'.repeat(220)}`;
    try {
      ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--to-agent', 'agent-b',
        '--kind', 'fyi',
        '--subject', 'Body trim',
        '--body', longBody,
        '--workspace', dir,
      ]);

      const summarized = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--workspace', dir,
        '--compact',
      ]);
      expect(summarized['bodies']).toBe('summarized');
      const shortBody = String((summarized['signals'] as Record<string, unknown>[])[0]!['body'] ?? '');
      expect(shortBody.length).toBeLessThanOrEqual(160);
      expect(shortBody.endsWith('...')).toBe(true);

      const full = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--workspace', dir,
        '--include-bodies',
        '--compact',
      ]);
      expect(full).not.toHaveProperty('bodies');
      expect(String((full['signals'] as Record<string, unknown>[])[0]!['body'])).toBe(longBody);
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

      ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-b',
        '--to-agent', 'agent-a',
        '--kind', 'fyi',
        '--subject', 'Unrelated signal',
        '--workspace', dir,
      ]);
      const selected = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-a',
        '--signal-id', replyId,
        '--all',
        '--workspace', dir,
      ]);
      expect(selected['count']).toBe(1);
      expect((selected['signals'] as Record<string, unknown>[])[0]!['signal_id']).toBe(replyId);
    } finally { rmSync(dir, { recursive: true }); }
  });
});

// ─── integration: full round-trip ────────────────────────────────────────────

describe('integration: full round-trip', () => {
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
