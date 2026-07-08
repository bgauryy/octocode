import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { insertEditLog } from '../src/audit.js';
import { registerAgent } from '../src/agents.js';
import { preFlightIntent } from '../src/intents.js';
import { insertMemory } from '../src/memory.js';
import { agentSignal } from '../src/notifications.js';
import { insertRefinement } from '../src/refinements.js';
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
    references: [`file:${file}`],
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
  return { db, file };
}

describe('repo context query and projections', () => {
  it('queries every view and renders all supported formats', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-context-'));
    try {
      const { db, file } = seededDb(dir);
      const base = { workspacePath: dir, artifact: 'svc', limit: 20 };

      for (const view of ['repo-profile', 'memories', 'gotchas', 'lessons', 'tasks', 'locks', 'agents', 'signals', 'refinements', 'files', 'activity'] as const) {
        const result = queryAwareness(db, { ...base, view, includeBodies: true });
        expect(result.ok).toBe(true);
        expect(result.view).toBe(view);
        expect(Array.isArray(result.rows)).toBe(true);
      }

      const all = queryAwareness(db, { ...base, view: 'all', query: 'auth', file, includeBodies: true });
      expect(all.sections?.gotchas?.count).toBeGreaterThanOrEqual(1);
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
      expect(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')).toContain('"schema_version": 1');
      expect(readFileSync(join(dir, '.octocode', 'awareness', 'csv', 'files.csv'), 'utf8')).toContain('auth.ts');
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
