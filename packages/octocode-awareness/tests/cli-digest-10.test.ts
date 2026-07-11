/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
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
function daysAgo(days: number): string {
    return new Date(Date.now() - days * 86400000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

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
