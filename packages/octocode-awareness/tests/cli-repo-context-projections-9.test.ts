/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, existsSync, readFileSync } from 'node:fs';
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
        '--compact',
      ]);
      expect(result['counts']).toMatchObject({ Inbox: 1, Ready: 0, Claimed: 0, Verify: 0, FilesUnderWork: 0 });
      expect(result['workboard']).toMatchObject({ Inbox: expect.any(Array) });
      expect(result['evidence']).toEqual(expect.arrayContaining([
        expect.objectContaining({ kind: 'memory', trust: 'existing_file_lead' }),
      ]));
      expect(result['profile']).toBeUndefined();
      expect(result['drive_state']).toBeUndefined();
      expect(result['organ_reference']).toBeUndefined();
      expect(result['next']).toMatchObject({ action: 'inspect_inbox', command: { name: 'signal list' } });
      expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThanOrEqual(2 * 1024);
      expect(JSON.stringify(result)).not.toMatch(/fictional persistent personality/i);
      const explained = ok(db, ['attend', '--workspace', dir, '--compact', '--explain-organ']);
      expect(explained['organ_reference']).toEqual(expect.arrayContaining([
        expect.objectContaining({ organ: 'attention' }),
      ]));
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

});
