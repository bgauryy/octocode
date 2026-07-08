import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { insertEditLog } from '../src/audit.js';
import { registerAgent } from '../src/agents.js';
import { preFlightIntent } from '../src/intents.js';
import { insertMemory } from '../src/memory.js';
import { agentSignal } from '../src/notifications.js';
import { insertRefinement } from '../src/refinements.js';
import { attendAwareness } from '../src/attend.js';
import {
  formatAwarenessQueryResult,
  injectRepoContext,
  queryAwareness,
  renderAwarenessHtml,
  writeAwarenessView,
} from '../src/repo-context.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

function seedPendingTasks(db: DatabaseSync, workspace: string, file: string): void {
  const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  for (const taskId of ['task_pending_a', 'task_pending_b']) {
    db.prepare(
      `INSERT INTO tasks (task_id, agent_id, rationale, test_plan, status, workspace_path, artifact, files_json, created_at, updated_at)
       VALUES (?, ?, ?, ?, 'PENDING', ?, ?, ?, ?, ?)`
    ).run(
      taskId,
      'agent-a',
      'verify auth file',
      'vitest auth',
      workspace,
      'svc',
      JSON.stringify([file]),
      now,
      now,
    );
  }
}

function seededDb(workspace: string): { db: DatabaseSync; file: string } {
  const db = freshDb();
  const file = join(workspace, 'src', 'auth.ts');
  registerAgent(db, {
    agentId: 'agent-a',
    agentName: 'Agent A',
    workspacePath: workspace,
    artifact: 'svc',
    context: 'repo-context test',
  });
  insertMemory(db, {
    agentId: 'agent-a',
    taskContext: 'auth gotcha',
    observation: 'Token migration order matters for auth',
    importance: 9,
    label: 'GOTCHA',
    tags: ['auth'],
    references: [`file:${file}`, 'https://example.com/auth-guide', 'repo:bgauryy/octocode-mcp', 'doc:auth-runbook'],
    workspacePath: workspace,
    artifact: 'svc',
    failureSignature: 'mechanism:auth|cause:order',
  });
  insertMemory(db, {
    agentId: 'agent-a',
    taskContext: 'auth decision',
    observation: 'Use schema before data backfill',
    importance: 8,
    label: 'DECISION',
    workspacePath: workspace,
    artifact: 'svc',
  });
  preFlightIntent(db, {
    agentId: 'agent-a',
    workspacePath: workspace,
    artifact: 'svc',
    targetFiles: [file],
    rationale: 'edit auth file',
    testPlan: 'vitest auth',
  });
  insertRefinement(db, {
    agentId: 'agent-a',
    workspacePath: workspace,
    artifact: 'svc',
    reasoning: 'Continue auth cleanup',
    remember: 'Finish middleware after router',
    quality: 'handoff',
    state: 'open',
    files: [file],
  });
  agentSignal(db, {
    action: 'publish',
    agentId: 'agent-a',
    toAgents: ['agent-b'],
    workspacePath: workspace,
    artifact: 'svc',
    kind: 'decision',
    subject: 'auth order',
    body: 'schema first',
    files: [file],
    refs: ['doc:auth'],
    importance: 7,
  });
  insertEditLog(db, {
    agentId: 'agent-a',
    workspacePath: workspace,
    filePath: file,
    operation: 'update',
    linesAdded: 9,
    linesRemoved: 3,
  });
  seedPendingTasks(db, workspace, file);
  return { db, file };
}

describe('repo context query and projections', () => {
  it('queries every view and renders all supported formats', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-context-'));
    try {
      const { db, file } = seededDb(dir);
      const base = { workspacePath: dir, artifact: 'svc', limit: 20 };

      for (const view of ['repo-profile', 'memories', 'gotchas', 'lessons', 'tasks', 'locks', 'agents', 'signals', 'refinements', 'files', 'activity', 'workboard'] as const) {
        const result = queryAwareness(db, { ...base, view, includeBodies: true });
        expect(result.ok).toBe(true);
        expect(result.view).toBe(view);
        expect(Array.isArray(result.rows)).toBe(true);
      }

      const workboard = queryAwareness(db, { ...base, view: 'workboard', limit: 10 });
      const verify = workboard.rows.find(row => row.column === 'Verify');
      expect(verify?.count).toBe(2);
      expect(verify?.raw_ids).toEqual(expect.arrayContaining(['task_pending_a', 'task_pending_b']));
      expect(workboard.rows.some(row => row.column === 'Inbox' && row.item_type === 'signal')).toBe(true);
      expect(workboard.rows.some(row => row.column === 'Claimed' && row.item_type === 'lock')).toBe(true);

      const all = queryAwareness(db, { ...base, view: 'all', query: 'auth', file, includeBodies: true });
      expect(all.sections?.gotchas?.count).toBeGreaterThanOrEqual(1);
      expect(all.sections?.workboard?.count).toBeGreaterThanOrEqual(1);
      expect(all.sections?.files?.rows[0]?.file_path).toBe(file);
      expect(formatAwarenessQueryResult(all, 'json')).toContain('"view": "all"');
      expect(formatAwarenessQueryResult(all, 'csv')).toContain('section,count');
      expect(formatAwarenessQueryResult(all, 'table')).toContain('section');
      expect(formatAwarenessQueryResult(all, 'markdown')).toContain('# Awareness all');
      expect(formatAwarenessQueryResult(all, 'html')).toContain('<!doctype html>');
      expect(renderAwarenessHtml(all)).toContain('Octocode Awareness: all');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('builds an attend packet with workboard, evidence, organ state, and drive state', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-'));
    try {
      const { db, file } = seededDb(dir);
      const result = attendAwareness(db, {
        workspacePath: dir,
        artifact: 'svc',
        query: 'auth',
        file,
        limit: 10,
        compact: true,
      });

      expect(result.ok).toBe(true);
      expect(result.profile.active_memories).toBeGreaterThanOrEqual(2);
      expect(result.workboard.Verify?.[0]?.raw_ids).toEqual(expect.arrayContaining(['task_pending_a', 'task_pending_b']));
      expect(result.evidence[0]?.why_selected.join(' ')).toContain('auth');
      expect(result.organ_state).toHaveProperty('attention');
      expect(result.drive_state).toMatchObject({
        goal: 'auth',
        mode: expect.stringMatching(/explore|exploit|mixed/),
        team_norms: expect.arrayContaining(['evidence-first', 'non-destructive']),
      });
      expect(JSON.stringify(result.drive_state)).not.toMatch(/permanent agent personality/i);
      expect(result.verification_targets.length).toBeGreaterThanOrEqual(1);
      expect(result.next).toContain('verify audit');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('points bloat next at forget + inject when verify is clear', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-bloat-'));
    try {
      const { db } = seededDb(dir);
      mkdirSync(join(dir, '.octocode'), { recursive: true });
      writeFileSync(join(dir, '.octocode', 'MEMORY.md'), `${'x\n'.repeat(250)}`, 'utf8');
      writeFileSync(join(dir, '.octocode', 'GOTCHAS.md'), `${'y\n'.repeat(250)}`, 'utf8');
      writeFileSync(join(dir, '.octocode', 'LEARN.md'), `${'z\n'.repeat(250)}`, 'utf8');
      // Mark pending tasks verified so bloat drives next.
      db.prepare(`UPDATE tasks SET status = 'SUCCESS' WHERE status = 'PENDING'`).run();
      const result = attendAwareness(db, {
        workspacePath: dir,
        query: 'projection bloat hygiene',
        limit: 10,
        compact: true,
      });
      expect(result.bloat_warnings.length).toBeGreaterThan(0);
      expect(result.verification_targets.length).toBe(0);
      expect(result.next).toContain('memory forget');
      expect(result.next).toContain('repo inject');
      expect(result.next).toMatch(/digest does not shrink markdown/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('routes role-dialogue queries to self-reflection-dialogue.md', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-dialogue-'));
    try {
      const { db } = seededDb(dir);
      const result = attendAwareness(db, {
        workspacePath: dir,
        query: 'role dialogue tutor student review',
        limit: 10,
        compact: true,
      });
      const leads = (result.drive_state.resource_leads ?? []) as Array<Record<string, unknown>>;
      const sources = leads.map(lead => String(lead['source'] ?? ''));
      expect(sources.some(source => source.includes('self-reflection-dialogue.md'))).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('writes HTML views and generated wiki files from the DB projection', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-inject-'));
    try {
      const { db } = seededDb(dir);
      const view = writeAwarenessView(db, {
        workspacePath: dir,
        artifact: 'svc',
        view: 'all',
        out: join(dir, '.octocode', 'awareness', 'index.html'),
      });
      expect(view.ok).toBe(true);
      expect(existsSync(view.path)).toBe(true);
      expect(readFileSync(view.path, 'utf8')).toContain('Octocode Awareness');

      const injected = injectRepoContext(db, {
        workspacePath: dir,
        artifact: 'svc',
        outDir: join(dir, '.octocode'),
        mode: 'local',
        includeView: true,
        check: false,
      });
      expect(injected.ok).toBe(true);
      expect(injected.files.some(file => file.endsWith('AGENTS.md'))).toBe(true);
      expect(readFileSync(join(dir, '.octocode', 'GOTCHAS.md'), 'utf8')).toContain('Token migration order matters');
      expect(readFileSync(join(dir, '.octocode', 'BOOKMARKS.md'), 'utf8')).toContain('https://example.com/auth-guide');
      expect(readFileSync(join(dir, '.octocode', 'BOOKMARKS.md'), 'utf8')).toContain('repo:bgauryy/octocode-mcp');
      const agentsMd = readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8');
      expect(agentsMd).toContain('Octocode Awareness Map');
      expect(agentsMd).toContain('Wiki And Memory Map');
      expect(agentsMd).toContain('Projection Health');
      expect(agentsMd).toContain('Root `AGENTS.md` should point here');
      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as {
        schema_version: number;
        budgets: { markdown: Record<string, { max_lines: number; actual_lines: number; within_budget: boolean }> };
      };
      expect(manifest.schema_version).toBe(1);
      const agentsBudget = manifest.budgets.markdown['AGENTS.md'];
      expect(agentsBudget).toMatchObject({ max_lines: 80, within_budget: true });
      expect(agentsBudget?.actual_lines).toBeGreaterThan(0);
      expect(manifest.budgets.markdown['BOOKMARKS.md']).toMatchObject({ max_lines: 200, within_budget: true });
      const attend = attendAwareness(db, { workspacePath: dir, artifact: 'svc', compact: true });
      const projectionFiles = ((attend.organ_state.senses as Record<string, unknown>).projection_health as Array<{ file: string }>).map(row => row.file);
      expect(projectionFiles).toEqual(expect.arrayContaining(['.octocode/BOOKMARKS.md', '.octocode/awareness/manifest.json']));
      expect(readFileSync(join(dir, '.octocode', 'awareness', 'csv', 'files.csv'), 'utf8')).toContain('auth.ts');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('resolves relative projection output paths against the requested workspace', () => {
    const workspaceDir = mkdtempSync(join(tmpdir(), 'oc-repo-inject-workspace-'));
    const cwdDir = mkdtempSync(join(tmpdir(), 'oc-repo-inject-cwd-'));
    const previousCwd = process.cwd();
    try {
      const { db } = seededDb(workspaceDir);
      process.chdir(cwdDir);

      const view = writeAwarenessView(db, {
        workspacePath: workspaceDir,
        view: 'all',
        out: '.octocode/awareness/index.html',
      });
      expect(view.path).toBe(join(workspaceDir, '.octocode', 'awareness', 'index.html'));
      expect(existsSync(view.path)).toBe(true);

      const injected = injectRepoContext(db, {
        workspacePath: workspaceDir,
        outDir: '.octocode',
        mode: 'local',
        includeView: false,
        check: false,
      });
      expect(injected.out_dir).toBe(join(workspaceDir, '.octocode'));
      expect(existsSync(join(workspaceDir, '.octocode', 'AGENTS.md'))).toBe(true);
      expect(existsSync(join(cwdDir, '.octocode', 'AGENTS.md'))).toBe(false);
    } finally {
      process.chdir(previousCwd);
      rmSync(workspaceDir, { recursive: true, force: true });
      rmSync(cwdDir, { recursive: true, force: true });
    }
  });

  it('keeps generated memory markdown within projection budgets', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-budget-'));
    try {
      const db = freshDb();
      for (let i = 0; i < 80; i++) {
        insertMemory(db, {
          agentId: 'agent-a',
          taskContext: `budget memory ${i}`,
          observation: `budget observation ${i}`,
          importance: 5,
          label: 'OTHER',
          workspacePath: dir,
        });
      }

      injectRepoContext(db, {
        workspacePath: dir,
        outDir: join(dir, '.octocode'),
        mode: 'local',
        includeView: false,
        check: false,
      });

      const memoryLines = readFileSync(join(dir, '.octocode', 'MEMORY.md'), 'utf8').split(/\r?\n/).length;
      expect(memoryLines).toBeLessThanOrEqual(200);
      expect(readFileSync(join(dir, '.octocode', 'MEMORY.md'), 'utf8')).toContain('Omitted by projection cap');
      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as {
        budgets: { markdown: Record<string, { within_budget: boolean }> };
      };
      expect(manifest.budgets.markdown['MEMORY.md']).toMatchObject({ within_budget: true });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('rejects unknown views, formats, and repo injection modes', () => {
    const db = freshDb();
    expect(() => queryAwareness(db, { view: 'unknown' })).toThrow('unknown octocode-awareness query view');
    expect(() => formatAwarenessQueryResult(queryAwareness(db, { view: 'all' }), 'bad')).toThrow('--format must be');
    expect(() => injectRepoContext(db, { mode: 'publish' })).toThrow('--mode must be local or share');
  });
});
