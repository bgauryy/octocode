import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { initDb } from '../src/db-init.js';
import { tsxCli } from './helpers/tsx-cli.js';
const SOURCE_SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../bin/awareness.ts');
const TSX_SCRIPT = tsxCli;
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
describe('source CLI regressions', () => {
  it('uses the source root bin for status and rejects the removed coordination prefix', () => {
    const dir = mktemp();
    const dbPath = join(dir, 'scratch.sqlite3');
    try {
      const root = runSource(['--db', dbPath, 'status', '--workspace', dir]);
      expect(root.status, root.stderr || root.stdout).toBe(0);
      expect(root.parsed).toMatchObject({ db_path: dbPath });
      const removed = runSource(['coordination', 'status', '--workspace', dir]);
      expect(removed.status).toBe(1);
      expect(removed.parsed).toMatchObject({ error_code: 'REMOVED_COORDINATION_ROUTE' });
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
  it('accepts equals-form global flags through the root bin', () => {
    const dir = mktemp();
    const leading = join(dir, 'leading.sqlite3');
    const trailing = join(dir, 'trailing.sqlite3');
    try {
      const before = runSource([`--db=${leading}`, 'status', '--workspace', dir]);
      const after = runSource(['status', `--db=${trailing}`, '--workspace', dir]);
      expect(before.status, before.stderr || before.stdout).toBe(0);
      expect(after.status, after.stderr || after.stdout).toBe(0);
      expect(before.parsed).toMatchObject({ db_path: leading });
      expect(after.parsed).toMatchObject({ db_path: trailing });
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
  it('supports compact output on root-bin status', () => {
    const dir = mktemp();
    try {
      const result = runSource(['status', '--workspace', dir, '--compact']);
      expect(result.status, result.stderr || result.stdout).toBe(0);
      expect(result.stdout.trim()).not.toContain('\n  ');
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('rejects missing equals-form global values', () => {
    const db = runSource(['--db=']);
    const scope = runSource(['--db-scope=']);
    expect(db.status).toBe(1);
    expect(scope.status).toBe(1);
    expect(`${db.stdout}${db.stderr}`).toContain('--db expects a path');
    expect(`${scope.stdout}${scope.stderr}`).toContain('--db-scope expects a value');
  });

  it('advertises every supported schema discovery route', () => {
    const result = runSource(['schema', '--help']);
    expect(result.status, result.stderr || result.stdout).toBe(0);
    // schemas are served dynamically — there is no `schema path` route and the
    // help must not advertise one.
    expect(result.stdout).not.toContain('path <name>');
    expect(result.stdout).toContain('command <noun> [action]');
    expect(result.stdout).toContain('json-schema <name>');
    expect(result.stdout).toContain('example <name>');
    expect(result.stdout).toContain('validate <name>');
    expect(result.stdout).toContain('list');
    expect(result.stdout).toContain('entities');
  });

  it('rejects a routine reflect (task + outcome only) with no reusable signal', () => {
    const dir = mktemp();
    const db = join(dir, 'aw.db');
    try {
      runSource(['--db', db, 'maintenance', 'init', '--compact']);
      const routine = runSource([
        '--db', db, 'reflect', 'record', '--task', 'routine status', '--outcome', 'worked', '--compact',
      ]);
      expect(routine.status).toBe(1);
      expect(String(routine.parsed?.['error'] ?? routine.stdout)).toMatch(/reusable lesson, failure, or fix/);

      const withLesson = runSource([
        '--db', db, 'reflect', 'record', '--task', 'real', '--outcome', 'worked',
        '--lesson', 'run build before tests', '--compact',
      ]);
      expect(withLesson.status, withLesson.stderr || withLesson.stdout).toBe(0);
      expect(String(withLesson.parsed?.['learning_memory_id'] ?? '')).toMatch(/^mem_/);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('hard-errors on a missing --db path before opening the canonical store', () => {
    const dir = mktemp();
    try {
      const result = runSource(['workspace', 'status', '--db', '--compact'], {
        env: { ...process.env, OCTOCODE_AGENT_DIR: dir },
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

  it('preserves false values for digest boolean flags', () => {
    const dir = mktemp();
    const dbPath = join(dir, 'test.sqlite3');
    const old = new Date(Date.now() - 30 * 86400000).toISOString();
    try {
      const db = new DatabaseSync(dbPath);
      initDb(db);
      db.prepare(`INSERT INTO task_runs (
        run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at
      ) VALUES ('run_cli_stale_active', 'WORK', 'old-agent', 'stale active session', 'maintenance digest', 'ACTIVE', ?, ?, ?)`).run(dir, old, old);
      db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
        VALUES ('run_cli_stale_active', ?, 'EXPLICIT', ?, ?, ?)`).run(join(dir, 'stale.ts'), old, old, old);
      db.close();

      const negated = runSource([
        '--db', dbPath,
        'maintenance', 'digest',
        '--workspace', dir,
        '--no-fail-stale-active-runs',
        '--compact',
      ]);
      expect(negated.status, negated.stderr || negated.stdout).toBe(0);
      expect(negated.parsed?.['failed_stale_active_runs']).toBe(0);

      const explicitFalse = runSource([
        '--db', dbPath,
        'maintenance', 'digest',
        '--workspace', dir,
        '--fail-stale-active-runs', 'false',
        '--compact',
      ]);
      expect(explicitFalse.status, explicitFalse.stderr || explicitFalse.stdout).toBe(0);
      expect(explicitFalse.parsed?.['failed_stale_active_runs']).toBe(0);

      const check = new DatabaseSync(dbPath);
      expect(check.prepare("SELECT status FROM task_runs WHERE run_id = 'run_cli_stale_active'").get())
        .toEqual({ status: 'ACTIVE' });
      check.close();
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  it('forwards schema --examples', () => {
    const dir = mktemp();
    try {
        const schema = runSource(['schema', 'commands', '--all', '--examples', '--compact']);
      expect(schema.status, schema.stderr || schema.stdout).toBe(0);
      const commands = schema.parsed?.['commands'] as Array<Record<string, unknown>>;
      expect(commands.length).toBeGreaterThan(10);
      expect(commands[0]).toHaveProperty('example');
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
        const row = conn.prepare("SELECT from_agent AS agent_id FROM signals WHERE kind = 'handoff'").get() as { agent_id: string };
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
        '--target-file', 'src/locked.ts',
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
        expect(conn.prepare('SELECT access_count FROM awareness_memories WHERE memory_id = ?').get(insideId))
          .toEqual({ access_count: 1 });
      } finally {
        conn.close();
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
});
