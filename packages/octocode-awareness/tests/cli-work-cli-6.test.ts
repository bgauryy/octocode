/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
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
      const runId = started['run_id'] as string;
      expect(started['status']).toBe('ACTIVE');
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
      expect(ended['status']).toBe('PENDING');
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
      const runId = started['run_id'] as string;
      const ended = run(db, [
        'work', 'end', '--agent-id', 'agent-many-files', '--run-id', runId, '--compact',
      ]);
      expect(ended.status).toBe(0);
      expect(ended.parsed).toMatchObject({ ok: true, run_id: runId, status: 'PENDING', file_count: 8, peer_count: 0 });
      expect(ended.parsed).not.toHaveProperty('files');
      expect(Buffer.byteLength(ended.stdout, 'utf8')).toBeLessThanOrEqual(512);
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
      expect(parsed['agent_id']).toBe('agent-from-env');
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
