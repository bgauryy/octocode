import { describe, expect, it } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { insertEditLog } from '../src/audit.js';
import { registerAgent } from '../src/agents.js';
import { insertMemory } from '../src/memory.js';
import { agentSignal } from '../src/notifications.js';
import { insertRefinement } from '../src/refinements.js';
import { attendAwareness } from '../src/attend.js';
import { injectRepoContext, writeAwarenessView } from '../src/repo-context.js';
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
it('writes HTML views and generated wiki files from the DB projection', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-inject-'));
    try {
      const { db, file } = seededDb(dir);
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
        includeView: false,
        check: false,
      });
      expect(injected.ok).toBe(true);
      expect(injected.files.some(file => file.endsWith('AGENTS.md'))).toBe(true);
      const knowledge = readFileSync(join(dir, '.octocode', 'KNOWLEDGE.md'), 'utf8');
      expect(knowledge).toContain('Token migration order matters');
      expect(knowledge).toContain('https://example.com/auth-guide');
      expect(knowledge).toContain('file:src/auth.ts');
      expect(knowledge).not.toContain(file);
      const agentsMd = readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8');
      expect(agentsMd).toContain('Octocode Awareness Map');
      expect(agentsMd).toContain('Projection Health');
      expect(agentsMd).toContain('Root `AGENTS.md` may point here');
      expect(agentsMd).not.toContain('append a root `AGENTS.md`');
      expect(agentsMd).not.toContain('Read GOTCHAS + LEARN');
      expect(agentsMd).not.toContain('## Files Under Work');
      expect(agentsMd).not.toContain('## Active Exclusive Locks');
      expect(agentsMd).toContain('memory recall');
      expect(agentsMd).toMatch(/ask before editing root `AGENTS\.md`/i);
      expect(agentsMd).not.toContain('or more detail is needed');
      expect(existsSync(join(dir, '.octocode', 'references', 'repo-map.md'))).toBe(false);
      expect(existsSync(join(dir, '.octocode', 'MEMORY.md'))).toBe(false);
      expect(existsSync(join(dir, '.octocode', 'BOOKMARKS.md'))).toBe(false);
      expect(existsSync(join(dir, '.octocode', 'awareness', 'csv', 'files.csv'))).toBe(false);
      expect(existsSync(view.path)).toBe(true); // explicit query export is preserved
      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as {
        files: string[];
        source: { revision: string };
        budgets: { markdown: Record<string, { max_lines: number; actual_lines: number; within_budget: boolean }> };
      };
      expect(manifest.source.revision).toMatch(/^sha256:/);
      expect(manifest.files).toContain('.octocode/awareness/manifest.json');
      const agentsBudget = manifest.budgets.markdown['AGENTS.md'];
      expect(agentsBudget).toMatchObject({ max_lines: 80, within_budget: true });
      expect(agentsBudget?.actual_lines).toBeGreaterThan(0);
      expect(manifest.budgets.markdown['KNOWLEDGE.md']).toMatchObject({ max_lines: 200, within_budget: true });
      const attend = attendAwareness(db, { workspacePath: dir, artifact: 'svc', compact: false });
      const projectionFiles = ((attend.organ_state?.senses as Record<string, unknown>).projection_health as Array<{ file: string }>).map(row => row.file);
      expect(projectionFiles).toEqual(expect.arrayContaining(['.octocode/KNOWLEDGE.md', '.octocode/awareness/manifest.json']));
      expect(attend.bloat_warnings ?? []).not.toContain('manifest older than generated projection files; regenerate repo projection');
      expect(attend.bloat_warnings ?? []).not.toContain('manifest source revision differs from live SQLite; regenerate repo projection');
      insertMemory(db, {
        agentId: 'agent-after-inject',
        taskContext: 'projection changed after inject',
        observation: 'live SQLite now differs from the generated snapshot',
        importance: 6,
        workspacePath: dir,
        artifact: 'svc',
      });
      const staleAttend = attendAwareness(db, { workspacePath: dir, artifact: 'svc', compact: false });
      expect(staleAttend.bloat_warnings ?? []).toContain('manifest source revision differs from live SQLite; regenerate repo projection');
      expect(knowledge).not.toContain(file);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('previews and prunes only retired Awareness-owned projection artifacts', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-orphans-'));
    try {
      const { db } = seededDb(dir);
      mkdirSync(join(dir, '.octocode', 'awareness'), { recursive: true });
      const html = join(dir, '.octocode', 'awareness', 'index.html');
      const legacyMemory = join(dir, '.octocode', 'MEMORY.md');
      writeFileSync(html, 'legacy generated viewer\n');
      writeFileSync(legacyMemory, 'legacy generated memory\n');
      const notes = join(dir, '.octocode', 'notes.md');
      const legacyAgentId = join(dir, '.octocode', '.agent-id');
      const plan = join(dir, '.octocode', 'plan', 'kept', 'PLAN.md');
      mkdirSync(join(dir, '.octocode', 'plan', 'kept'), { recursive: true });
      writeFileSync(notes, 'user-owned\n');
      writeFileSync(legacyAgentId, 'shared-workspace-agent\n');
      writeFileSync(plan, 'authored plan\n');

      const preview = injectRepoContext(db, { workspacePath: dir, includeView: false, check: false });
      expect(preview.orphan_candidates).toContain(html);
      expect(preview.orphan_candidates).toContain(legacyMemory);
      expect(preview.orphan_candidates).not.toContain(legacyAgentId);
      expect(preview.pruned_orphans).toEqual([]);
      expect(existsSync(html)).toBe(true);
      expect(existsSync(legacyAgentId)).toBe(false);

      const pruned = injectRepoContext(db, {
        workspacePath: dir,
        includeView: false,
        pruneOrphans: true,
        check: false,
      });
      expect(pruned.pruned_orphans).toContain(html);
      expect(pruned.pruned_orphans).toContain(legacyMemory);
      expect(existsSync(html)).toBe(false);
      expect(readFileSync(notes, 'utf8')).toBe('user-owned\n');
      expect(readFileSync(plan, 'utf8')).toBe('authored plan\n');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
it('resolves Git scope once for a complete repo injection', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-repo-inject-git-budget-'));
    const binDir = join(dir, 'bin');
    const countFile = join(dir, 'git-calls.log');
    const previousPath = process.env.PATH;
    const previousCountFile = process.env.OCTOCODE_GIT_COUNT_FILE;
    try {
      const { db } = seededDb(dir);
      mkdirSync(binDir, { recursive: true });
      const git = join(binDir, 'git');
      writeFileSync(git, '#!/bin/sh\nprintf "%s\\n" "$*" >> "$OCTOCODE_GIT_COUNT_FILE"\nexit 1\n');
      chmodSync(git, 0o755);
      process.env.PATH = `${binDir}:${previousPath ?? ''}`;
      process.env.OCTOCODE_GIT_COUNT_FILE = countFile;

      injectRepoContext(db, {
        workspacePath: dir,
        outDir: join(dir, '.octocode'),
        mode: 'local',
        includeView: true,
        check: true,
      });

      const calls = readFileSync(countFile, 'utf8').trim().split('\n').filter(Boolean);
      expect(calls.length, calls.join('\n')).toBeLessThanOrEqual(6);
      expect(calls.filter((call) => call.includes('rev-parse --show-toplevel'))).toHaveLength(1);
    } finally {
      process.env.PATH = previousPath;
      if (previousCountFile === undefined) delete process.env.OCTOCODE_GIT_COUNT_FILE;
      else process.env.OCTOCODE_GIT_COUNT_FILE = previousCountFile;
      rmSync(dir, { recursive: true, force: true });
    }
  });

});
