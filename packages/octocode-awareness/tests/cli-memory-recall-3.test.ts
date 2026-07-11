/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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
