import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { insertEditLog } from '../src/audit.js';
import { registerAgent } from '../src/agents.js';
import { insertMemory } from '../src/memory.js';
import { agentSignal } from '../src/notifications.js';
import { insertRefinement } from '../src/refinements.js';
import { reflect } from '../src/reflect.js';
import { injectRepoContext, queryAwareness, writeAwarenessView } from '../src/repo-context.js';
function freshDb(): DatabaseSync {
    const db = new DatabaseSync(':memory:');
    db.exec('PRAGMA foreign_keys = ON');
    initDb(db);
    return db;
}
function seedPendingTasks(db: DatabaseSync, workspace: string, file: string): void {
    const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
    for (const runId of ['run_pending_a', 'run_pending_b']) {
        db.prepare(`INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
       VALUES (?, 'WORK', ?, ?, ?, 'PENDING', ?, ?, ?, ?)`).run(runId, 'agent-a', 'verify auth file', 'vitest auth', workspace, 'svc', now, now);
        db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
       VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`).run(runId, file, now, now, new Date(Date.now() + 60000).toISOString());
    }
}
function seededDb(workspace: string): {
    db: DatabaseSync;
    file: string;
} {
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
    const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
    const future = new Date(Date.now() + 60000).toISOString();
    db.prepare(`INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
     VALUES ('run_auth', 'WORK', 'agent-a', 'edit auth file', 'vitest auth', 'ACTIVE', ?, 'svc', ?, ?)`).run(workspace, now, now);
    db.prepare(`INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
     VALUES ('run_auth', ?, 'EXPLICIT', ?, ?, ?)`).run(file, now, now, future);
    db.prepare(`INSERT INTO locks (lock_id, file_path, run_id, acquired_at, expires_at)
     VALUES ('lock_auth', ?, 'run_auth', ?, ?)`).run(file, now, future);
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
it('surfaces missing file references across query, workboard, projections, and HTML', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-missing-ref-'));
    try {
      const db = freshDb();
      mkdirSync(join(dir, 'src'), { recursive: true });
      const existing = join(dir, 'src', 'exists.ts');
      const missing = join(dir, 'src', 'missing.ts');
      writeFileSync(existing, 'export const ok = true;\n', 'utf8');
      insertMemory(db, {
        agentId: 'agent-a',
        taskContext: 'missing ref gotcha',
        observation: 'Do not trust old generated viewer paths without checking file refs',
        importance: 8,
        label: 'GOTCHA',
        references: [`file:${existing}:1`, `file:${missing}:27`],
        workspacePath: dir,
        failureSignature: 'mechanism:projection|cause:stale-file-ref',
      });

      const memories = queryAwareness(db, { workspacePath: dir, view: 'memories', limit: 10 });
      expect(memories.rows[0]?.['missing_reference_count']).toBe(1);
      expect(memories.rows[0]?.['missing_references']).toEqual([`file:${missing}:27`]);
      expect(memories.rows[0]?.['missing_files']).toEqual([missing]);

      const files = queryAwareness(db, { workspacePath: dir, view: 'files', limit: 10 });
      const missingRow = files.rows.find(row => row['file_path'] === missing);
      expect(missingRow).toMatchObject({ file_exists: false, missing_file: true, gotchas: 1 });

      const profile = queryAwareness(db, { workspacePath: dir, view: 'repo-profile', limit: 20 });
      expect(profile.rows).toContainEqual({ metric: 'missing_file_refs', count: 1 });

      const workboard = queryAwareness(db, { workspacePath: dir, view: 'workboard', limit: 10 });
      const review = workboard.rows.find(row => row['column'] === 'MemoryReview');
      expect(review?.['reasons']).toEqual(expect.arrayContaining(['stale_file_refs', 'failure_signature']));
      expect(review?.['missing_references']).toEqual([`file:${missing}:27`]);

      injectRepoContext(db, {
        workspacePath: dir,
        outDir: join(dir, '.octocode'),
        mode: 'local',
        includeView: false,
        check: false,
      });
      const knowledge = readFileSync(join(dir, '.octocode', 'KNOWLEDGE.md'), 'utf8');
      expect(knowledge).toContain('Missing refs: file:src/missing.ts:27');
      expect(knowledge).not.toContain(missing);
      const sourceIds = [...knowledge.matchAll(/Source id: `([^`]+)`/g)].map(match => match[1]);
      expect(sourceIds).toEqual([...new Set(sourceIds)]);
      const agents = readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8');
      expect(agents).toContain('MissingFiles 1');
      expect(agents).not.toContain('Do not trust old generated viewer paths without checking file refs');
      expect(existsSync(join(dir, '.octocode', 'awareness', 'index.html'))).toBe(false);
      expect(existsSync(join(dir, '.octocode', 'awareness', 'csv', 'files.csv'))).toBe(false);
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
// Inserts and projection writes contend with the full parallel package suite.
  it('keeps generated knowledge markdown within projection budgets', { timeout: 15_000 }, () => {
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
          preComputedSimilar: [],
        });
      }

      injectRepoContext(db, {
        workspacePath: dir,
        outDir: join(dir, '.octocode'),
        mode: 'local',
        includeView: false,
        check: false,
      });

      const memoryMarkdown = readFileSync(join(dir, '.octocode', 'KNOWLEDGE.md'), 'utf8');
      const memoryLines = memoryMarkdown.split(/\r?\n/).length;
      expect(memoryLines).toBeLessThanOrEqual(200);
      expect(memoryMarkdown).toContain('Omitted by projection cap');
      const summary = memoryMarkdown.match(/Total: (\d+) · Shown: (\d+) · Omitted: (\d+)/);
      expect(summary).not.toBeNull();
      const renderedRows = memoryMarkdown.match(/^## /gm)?.length ?? 0;
      const renderedIds = [...memoryMarkdown.matchAll(/Source id: `([^`]+)`/g)].map(match => match[1]);
      expect(new Set(renderedIds).size).toBe(renderedRows);
      expect({
        total: Number(summary?.[1]),
        shown: Number(summary?.[2]),
        omitted: Number(summary?.[3]),
      }).toEqual({
        total: 80,
        shown: renderedRows,
        omitted: 80 - renderedRows,
      });
      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as {
        budgets: { markdown: Record<string, { within_budget: boolean }> };
      };
      expect(manifest.budgets.markdown['KNOWLEDGE.md']).toMatchObject({ within_budget: true });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('surfaces instruction feedback via the developer-review view and KNOWLEDGE.md projection', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-devreview-'));
    try {
      const db = freshDb();
      reflect(db, {
        agentId: 'agent-a',
        task: 'add lock retry',
        outcome: 'partial',
        fixInstructions: 'AGENTS.md never states the default lock TTL — document it and how to extend.',
        workspacePath: dir,
      });

      const view = queryAwareness(db, { view: 'developer-review', workspacePath: dir });
      expect(view.count).toBe(1);
      expect(String(view.rows[0]!['feedback'])).toContain('default lock TTL');
      expect(view.rows[0]!['source']).toBe('refinement');
      expect(view.rows[0]!['state']).toBe('open');

      injectRepoContext(db, {
        workspacePath: dir,
        outDir: join(dir, '.octocode'),
        mode: 'local',
        includeView: false,
        check: false,
      });

      const devReview = readFileSync(join(dir, '.octocode', 'KNOWLEDGE.md'), 'utf8');
      expect(devReview).toContain('# Octocode Knowledge');
      expect(devReview).toContain('default lock TTL');

      const agentsMd = readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8');
      expect(agentsMd).toContain('## Knowledge');
      expect(agentsMd).toContain('.octocode/KNOWLEDGE.md');

      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as {
        counts: Record<string, number>;
        budgets: { markdown: Record<string, { within_budget: boolean }> };
      };
      expect(manifest.counts['developer-review']).toBe(1);
      expect(manifest.budgets.markdown['KNOWLEDGE.md']).toMatchObject({ within_budget: true });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

it('omits KNOWLEDGE.md when the workspace has no knowledge rows', () => {
  const dir = mkdtempSync(join(tmpdir(), 'oc-repo-empty-knowledge-'));
  try {
    const db = freshDb();
    const result = injectRepoContext(db, { workspacePath: dir, check: false });
    expect(result.files).toEqual(expect.arrayContaining([
      join(dir, '.octocode', 'AGENTS.md'),
      join(dir, '.octocode', 'awareness', 'manifest.json'),
    ]));
    expect(existsSync(join(dir, '.octocode', 'KNOWLEDGE.md'))).toBe(false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

});
