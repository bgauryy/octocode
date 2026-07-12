import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { attendAwareness } from '../src/attend.js';
import { digest } from '../src/maintenance.js';
import { getMemory, insertMemory } from '../src/memory.js';
import { agentSignal } from '../src/notifications.js';
import { insertRefinement, updateRefinement } from '../src/refinements.js';
import { reflect } from '../src/reflect.js';
import { formatAwarenessQueryResult, injectRepoContext, queryAwareness } from '../src/repo-context.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('READ -> DO -> LEARN closure fixes', () => {
  it('does not call a missing file reference verified and smart recall really broadens filters', () => {
    const dir = mkdtempSync(join(tmpdir(), 'awareness-learning-trust-'));
    try {
      const db = freshDb();
      const missing = join(dir, 'missing.ts');
      const { memoryId } = insertMemory(db, {
        agentId: 'learning-agent',
        taskContext: 'cache source evidence',
        observation: 'Validate current source before applying recalled cache rules.',
        importance: 8,
        label: 'GOTCHA',
        references: [`file:${missing}`],
        workspacePath: dir,
      });

      const packet = attendAwareness(db, {
        agentId: 'learning-agent', workspacePath: dir, query: 'cache source evidence', compact: true,
      });
      expect(packet.evidence[0]?.id).toBe(memoryId);
      expect(packet.evidence[0]?.trust).not.toBe('verified_lead');

      const broadened = getMemory(db, {
        query: 'cache source evidence', workspacePath: dir, label: ['SECURITY'], smart: true,
      });
      expect(broadened.memories.map(memory => memory.memory_id)).toContain(memoryId);
      expect((broadened as typeof broadened & { smart_expanded?: boolean }).smart_expanded).toBe(true);
      expect((broadened as typeof broadened & { smart_dropped_filters?: string[] }).smart_dropped_filters)
        .toContain('label');
      expect(broadened.judgment_reason ?? '').not.toContain('retry with --smart');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('requires an actor and a check receipt before terminal refinement closure', () => {
    const db = freshDb();
    expect(() => insertRefinement(db, {
      reasoning: 'bypass', remember: 'must not exist', state: 'done', workspacePath: '/repo',
    })).toThrow(/terminal refinement creation is not allowed/);
    const { refinementId } = insertRefinement(db, {
      agentId: 'owner-agent',
      reasoning: 'Fix the recall contract',
      remember: 'Apply and test the smart recall change',
      quality: 'instructions',
      state: 'open',
      workspacePath: '/repo',
    });

    expect(() => updateRefinement(db, { refinementId, state: 'done' }))
      .toThrow(/actor.*check receipt/i);
    const closed = updateRefinement(db, {
      refinementId,
      state: 'done',
      actorAgentId: 'instruction-author',
      checkReceipt: 'full-loop-learning-fixes.test.ts passed',
    });
    expect(closed.refinement?.state).toBe('done');
    expect(closed.refinement?.reasoning).toContain('Closure receipt');
    expect(closed.refinement?.reasoning).toContain('instruction-author');
  });

  it('keeps foreign maintenance pressure informational instead of hijacking next work', () => {
    const db = freshDb();
    db.prepare(`INSERT INTO task_runs
      (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES ('run_foreign_old', 'WORK', 'other-agent', 'old peer debt', 'peer check', 'PENDING', '/repo', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')`).run();

    const packet = attendAwareness(db, {
      agentId: 'worker', workspacePath: '/repo', query: 'implement current feature', compact: true,
    });
    expect(packet.counts?.Maintenance).toBeGreaterThan(0);
    expect(packet.next).not.toContain('verify audit');
    expect(packet.next).toContain('narrower task');
  });

  it('deduplicates repeated reflection summaries instead of growing ACTIVE memory', () => {
    const db = freshDb();
    const params = {
      agentId: 'learning-agent', task: 'stable workflow', outcome: 'worked' as const,
      lesson: 'Run the focused check before the broad suite.', workspacePath: '/repo',
    };
    const first = reflect(db, params);
    const second = reflect(db, params);
    expect(second.learning_memory_id).toBe(first.learning_memory_id);
    expect(second.learning_memory_skipped).toBe(true);
    expect(db.prepare("SELECT COUNT(*) AS count FROM memories WHERE state = 'ACTIVE'").get())
      .toEqual({ count: 1 });
  });

  it('resolves relative reflection files against the supplied workspace', () => {
    const dir = mkdtempSync(join(tmpdir(), 'awareness-reflect-scope-'));
    try {
      const db = freshDb();
      const result = reflect(db, {
        agentId: 'learning-agent',
        task: 'fix instructions',
        outcome: 'partial',
        fixInstructions: 'Clarify the recall contract.',
        files: ['docs/guide.md'],
        workspacePath: dir,
      });
      const row = db.prepare('SELECT files_json FROM refinements WHERE refinement_id = ?')
        .get(result.developer_review_refinement_id!) as { files_json: string };
      expect(JSON.parse(row.files_json)).toEqual([`file:${join(realpathSync(dir), 'docs', 'guide.md')}`]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('marks bounded explicit exports partial and publishes a complete sanitized lean snapshot atomically', () => {
    const dir = mkdtempSync(join(tmpdir(), 'awareness-share-safe-'));
    try {
      const db = freshDb();
      for (let index = 0; index < 55; index += 1) {
        insertMemory(db, {
          agentId: 'learning-agent',
          taskContext: `lesson ${index}`,
          observation: index === 0 ? '=HYPERLINK("https://example.invalid","click")' : `Distinct lesson ${index}`,
          importance: 5,
          label: 'WORKFLOW',
          workspacePath: dir,
          preComputedSimilar: [],
        });
      }
      agentSignal(db, {
        action: 'publish',
        agentId: 'learning-agent',
        workspacePath: dir,
        kind: 'fyi',
        subject: 'share probe',
        body: `PRIVATE_PLACEHOLDER ${join(dir, 'secret.ts')}`,
        files: [join(dir, 'secret.ts')],
      });

      const bounded = queryAwareness(db, { view: 'memories', workspacePath: dir, limit: 10 });
      expect(bounded.count).toBe(10);
      expect(bounded.is_partial).toBe(true);
      expect(bounded.continuation).toBeTruthy();
      const boundedCsv = formatAwarenessQueryResult(bounded, 'csv');
      expect(boundedCsv).toContain('__awareness_is_partial');
      expect(boundedCsv).toContain('true');
      const explicitCsv = formatAwarenessQueryResult(
        queryAwareness(db, { view: 'memories', workspacePath: dir, limit: 500 }),
        'csv',
      );

      const injected = injectRepoContext(db, {
        workspacePath: dir, outDir: '.octocode-share', mode: 'share', check: false, limit: 500,
      });
      const manifest = injected.manifest as {
        counts: Record<string, number>;
        completeness: Record<string, { is_partial: boolean; visible: number }>;
        workspace_path: string;
      };
      expect(manifest.counts['memories']).toBe(55);
      expect(manifest.completeness['memories']).toMatchObject({ is_partial: false, visible: 55 });
      expect(manifest.workspace_path).not.toContain(dir);

      expect(explicitCsv).toContain("'=HYPERLINK");
      expect(existsSync(join(dir, '.octocode-share', 'awareness', 'csv'))).toBe(false);
      expect(existsSync(join(dir, '.octocode-share', 'awareness', 'index.html'))).toBe(false);
      const leakingFiles = injected.files.filter(file => readFileSync(file, 'utf8').includes('PRIVATE_PLACEHOLDER'));
      expect(leakingFiles).toEqual([]);
      expect(injected.files.some(file => file.includes('.tmp-'))).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 30_000);

  it('reports only age-qualified maintenance pressure and emits selector-bearing actions', () => {
    const db = freshDb();
    const workspace = '/repo';
    const old = '2020-01-01T00:00:00Z';
    const now = new Date().toISOString();
    db.prepare(`
      INSERT INTO task_runs
        (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
      VALUES
        ('run_old', 'WORK', 'owner', 'old pending', 'test', 'PENDING', ?, ?, ?),
        ('run_fresh', 'WORK', 'owner', 'fresh pending', 'test', 'PENDING', ?, ?, ?)
    `).run(workspace, old, old, workspace, now, now);
    agentSignal(db, {
      action: 'publish', agentId: 'owner', workspacePath: workspace,
      kind: 'fyi', subject: 'old signal', body: 'review me',
    });
    db.prepare("UPDATE signals SET created_at = ? WHERE subject = 'old signal'").run(old);
    const memory = insertMemory(db, {
      agentId: 'owner', taskContext: 'stale ref', observation: 'review old path', importance: 5,
      references: ['file:/repo/missing.ts'], workspacePath: workspace,
    });
    db.prepare('UPDATE memories SET created_at = ?, updated_at = ? WHERE memory_id = ?')
      .run(old, old, memory.memoryId);

    const preview = digest(db, { workspace, dry_run: true });
    expect(preview.pressure_age_days).toBe(1);
    expect(preview.stale_pending_runs).toBe(1);
    expect(preview.stale_open_signals).toBe(1);
    expect(preview.stale_missing_refs).toBe(1);
    expect(db.prepare("SELECT status FROM task_runs WHERE run_id = 'run_old'").get())
      .toEqual({ status: 'PENDING' });

    const board = queryAwareness(db, { view: 'workboard', workspacePath: workspace, limit: 10 });
    const maintenance = board.rows.filter(row => row.column === 'Maintenance');
    expect(maintenance.length).toBeGreaterThan(0);
    expect(maintenance.every(row => String(row.action).includes('--'))).toBe(true);
    expect(maintenance.every(row => !String(row.action).includes(';'))).toBe(true);
    expect(maintenance.some(row => String(row.action).includes('--memory-id'))).toBe(true);
  });

  it('keeps terminal refinement creation behind the receipt-bearing update path', () => {
    const cliSource = readFileSync(new URL('../bin/cli-memory.ts', import.meta.url), 'utf8');
    const terminalCreateGuard = cliSource.indexOf("if (stateVal === 'done')");
    const createCall = cliSource.indexOf('const { refinement } = insertRefinement', terminalCreateGuard);
    expect(terminalCreateGuard).toBeGreaterThan(0);
    expect(createCall).toBeGreaterThan(terminalCreateGuard);
    expect(cliSource.slice(terminalCreateGuard, createCall)).toContain('terminal refinement creation is not allowed');
    expect(cliSource).toContain("'check_receipt'");
  });
});
