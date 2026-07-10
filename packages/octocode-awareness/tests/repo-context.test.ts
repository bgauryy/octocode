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
import { reflect } from '../src/reflect.js';
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
  for (const runId of ['run_pending_a', 'run_pending_b']) {
    db.prepare(
      `INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
       VALUES (?, 'WORK', ?, ?, ?, 'PENDING', ?, ?, ?, ?)`
    ).run(
      runId,
      'agent-a',
      'verify auth file',
      'vitest auth',
      workspace,
      'svc',
      now,
      now,
    );
    db.prepare(
      `INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
       VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`
    ).run(runId, file, now, now, new Date(Date.now() + 60_000).toISOString());
  }
}

function seedActiveFilePeers(db: DatabaseSync, workspace: string, file: string): void {
  const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  const future = new Date(Date.now() + 60_000).toISOString();
  const past = new Date(Date.now() - 60_000).toISOString();
  db.prepare(
    `INSERT INTO plans (plan_id, name, objective, lead_agent_id, status, workspace_path, artifact, doc_dir, created_at, updated_at)
     VALUES ('plan_file_work', 'Shared auth plan', 'Coordinate auth edits', 'agent-a', 'ACTIVE', ?, 'svc', '.octocode/plan/auth', ?, ?)`
  ).run(workspace, now, now);
  db.prepare(
    `INSERT INTO tasks (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
     VALUES ('task_file_work', 'plan_file_work', 'Edit auth', 'shared task reason', 'tests pass', 'IN_PROGRESS', 80, 'agent-a', ?, ?)`
  ).run(now, now);
  for (const [index, agentId] of ['agent-a', 'agent-b', 'agent-c', 'agent-d', 'agent-expired'].entries()) {
    const runId = `run_peer_${index}`;
    db.prepare(
      `INSERT INTO task_runs (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
       VALUES (?, ?, ?, ?, ?, 'focused test', 'ACTIVE', ?, 'svc', ?, ?)`
    ).run(
      runId,
      index === 0 ? 'task_file_work' : null,
      index === 0 ? 'TASK' : 'WORK',
      agentId,
      `reason ${index}`,
      workspace,
      now,
      now,
    );
    db.prepare(
      `INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
       VALUES (?, ?, 'EXPLICIT', ?, ?, ?)`
    ).run(runId, file, now, now, agentId === 'agent-expired' ? past : future);
  }
  db.prepare(
    `INSERT INTO locks (lock_id, file_path, run_id, acquired_at, expires_at)
     VALUES ('lock_peer', ?, 'run_peer_0', ?, ?)`
  ).run(file, now, future);
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
  const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  const future = new Date(Date.now() + 60_000).toISOString();
  db.prepare(
    `INSERT INTO task_runs (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
     VALUES ('run_auth', 'WORK', 'agent-a', 'edit auth file', 'vitest auth', 'ACTIVE', ?, 'svc', ?, ?)`
  ).run(workspace, now, now);
  db.prepare(
    `INSERT INTO run_files (run_id, file_path, source, started_at, heartbeat_at, expires_at)
     VALUES ('run_auth', ?, 'EXPLICIT', ?, ?, ?)`
  ).run(file, now, now, future);
  db.prepare(
    `INSERT INTO locks (lock_id, file_path, run_id, acquired_at, expires_at)
     VALUES ('lock_auth', ?, 'run_auth', ?, ?)`
  ).run(file, now, future);
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

      for (const view of ['repo-profile', 'memories', 'gotchas', 'lessons', 'plans', 'tasks', 'runs', 'locks', 'agents', 'signals', 'refinements', 'files', 'activity', 'workboard'] as const) {
        const result = queryAwareness(db, { ...base, view, includeBodies: true });
        expect(result.ok).toBe(true);
        expect(result.view).toBe(view);
        expect(Array.isArray(result.rows)).toBe(true);
      }

      const workboard = queryAwareness(db, { ...base, view: 'workboard', limit: 10 });
      const verify = workboard.rows.filter(row => row.column === 'Verify');
      expect(verify).toHaveLength(2);
      expect(verify.flatMap(row => row.raw_ids as string[]))
        .toEqual(expect.arrayContaining(['run_pending_a', 'run_pending_b']));
      expect(workboard.rows.some(row => row.column === 'Inbox' && row.item_type === 'signal')).toBe(true);
      expect(workboard.rows.some(row => row.column === 'FilesUnderWork' && row.item_type === 'file')).toBe(true);

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

  it('builds a delta-sized compact attend packet with only actionable work', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-'));
    try {
      const { db, file } = seededDb(dir);
      const accessBefore = db.prepare(
        'SELECT COALESCE(SUM(access_count), 0) AS count FROM memories'
      ).get() as { count: number };
      const result = attendAwareness(db, {
        agentId: 'agent-a',
        workspacePath: dir,
        artifact: 'svc',
        query: 'auth',
        file,
        limit: 10,
        compact: true,
      });

      expect(result.ok).toBe(true);
      expect(result.counts).toMatchObject({ Ready: expect.any(Number), Claimed: expect.any(Number), Verify: 2, FilesUnderWork: expect.any(Number) });
      expect(result.workboard.Verify?.map(row => row.id)).toContain('run_pending_a');
      expect(result.evidence[0]?.why_selected.join(' ')).toContain('auth');
      expect(result.next).toContain('verify audit');
      expect(result.next).toContain('--run-id run_pending_');
      expect(result.next).not.toContain('--all-pending');
      expect(result).not.toHaveProperty('profile');
      expect(result).not.toHaveProperty('organ_state');
      expect(result).not.toHaveProperty('drive_state');
      expect(result).not.toHaveProperty('verification_targets');
      expect(JSON.stringify(result)).not.toContain('raw_ids');
      expect(Buffer.byteLength(JSON.stringify(result), 'utf8')).toBeLessThan(2 * 1024);
      const accessAfter = db.prepare(
        'SELECT COALESCE(SUM(access_count), 0) AS count FROM memories'
      ).get() as { count: number };
      expect(accessAfter.count).toBe(accessBefore.count);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('keeps global Verify totals exact while routing only the current agent', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-owner-'));
    try {
      const { db, file } = seededDb(dir);
      const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
      const insertRun = db.prepare(`INSERT INTO task_runs
        (run_id, origin, agent_id, rationale, test_plan, status, workspace_path, artifact, created_at, updated_at)
        VALUES (?, 'WORK', 'agent-a', 'bulk pending', 'bulk test', 'PENDING', ?, 'svc', ?, ?)`);
      const insertFile = db.prepare(`INSERT INTO run_files
        (run_id, file_path, source, started_at, heartbeat_at, expires_at, ended_at)
        VALUES (?, ?, 'EXPLICIT', ?, ?, ?, ?)`);
      db.exec('BEGIN');
      for (let index = 0; index < 501; index++) {
        const runId = `run_bulk_${String(index).padStart(4, '0')}`;
        insertRun.run(runId, dir, now, now);
        insertFile.run(runId, file, now, now, now, now);
      }
      db.exec('COMMIT');

      const peer = attendAwareness(db, {
        agentId: 'agent-b', workspacePath: dir, artifact: 'svc', compact: true,
      });
      expect(peer.counts?.Verify).toBe(503);
      expect(peer.next).not.toContain('verify audit');

      const owner = attendAwareness(db, {
        agentId: 'agent-a', workspacePath: dir, artifact: 'svc', compact: true,
      });
      expect(owner.counts?.Verify).toBe(503);
      expect(owner.next).toContain('verify audit');
      expect(owner.next).not.toContain('--all-pending');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('routes a submitted task to its exact pending run owner', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-attend-task-owner-'));
    try {
      const db = freshDb();
      const now = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
      db.prepare(`INSERT INTO plans
        (plan_id, name, objective, lead_agent_id, status, workspace_path, doc_dir, created_at, updated_at)
        VALUES ('plan_verify', 'Verify', 'Route exact run', 'lead', 'ACTIVE', ?, '.octocode/plan/verify', ?, ?)`)
        .run(dir, now, now);
      db.prepare(`INSERT INTO tasks
        (task_id, plan_id, title, reasoning, acceptance_criteria, status, priority, created_by, created_at, updated_at)
        VALUES ('task_verify', 'plan_verify', 'Verify task', 'reason', 'tests pass', 'VERIFY', 1, 'lead', ?, ?)`)
        .run(now, now);
      db.prepare(`INSERT INTO task_runs
        (run_id, task_id, origin, agent_id, rationale, test_plan, status, workspace_path, created_at, updated_at)
        VALUES ('run_verify_exact', 'task_verify', 'TASK', 'worker', 'reason', 'tests pass', 'PENDING', ?, ?, ?)`)
        .run(dir, now, now);

      const worker = attendAwareness(db, { agentId: 'worker', workspacePath: dir, compact: true });
      expect(worker.next).toContain('--run-id run_verify_exact');
      const lead = attendAwareness(db, { agentId: 'lead', workspacePath: dir, compact: true });
      expect(lead.next).not.toContain('verify audit');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('groups active file work by relative path, caps peers, and shows exclusive lock state', () => {
    const dir = mkdtempSync(join(tmpdir(), 'oc-file-work-'));
    try {
      const db = freshDb();
      const file = join(dir, 'src', 'auth.ts');
      seedActiveFilePeers(db, dir, file);

      const workboard = queryAwareness(db, { workspacePath: dir, artifact: 'svc', view: 'workboard', limit: 10 });
      const row = workboard.rows.find(item => item.column === 'FilesUnderWork');
      expect(row).toMatchObject({
        item_type: 'file',
        path: 'src/auth.ts',
        peer_count: 4,
        omitted_peer_count: 1,
        locked: true,
        lock_agent_id: 'agent-a',
      });
      expect(row?.agents).toEqual(['agent-a', 'agent-b', 'agent-c']);
      expect(row?.task_ids).toEqual(['task_file_work']);
      expect(row?.plan_ids).toEqual(['plan_file_work']);
      expect(row?.plans).toEqual(['Shared auth plan']);
      expect(row?.reasons).toEqual(['shared task reason', 'reason 1', 'reason 2']);
      expect(String(row?.path)).not.toContain(dir);

      const compact = attendAwareness(db, { workspacePath: dir, artifact: 'svc', query: 'auth', compact: true });
      expect(compact.workboard.FilesUnderWork?.[0]).toMatchObject({ path: 'src/auth.ts', peer_count: 4 });
      expect(compact.workboard.FilesUnderWork?.[0]).not.toHaveProperty('agents');
      expect(compact.workboard.FilesUnderWork?.[0]).not.toHaveProperty('reasons');
      expect(compact.workboard.FilesUnderWork?.[0]).not.toHaveProperty('lock_expires_at');
      expect(Buffer.byteLength(JSON.stringify(compact), 'utf8')).toBeLessThan(2 * 1024);
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
      db.prepare(`UPDATE task_runs SET status = 'SUCCESS' WHERE status = 'PENDING'`).run();
      const result = attendAwareness(db, {
        workspacePath: dir,
        query: 'projection bloat hygiene',
        limit: 10,
        compact: true,
      });
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
      const full = attendAwareness(db, {
        workspacePath: dir,
        query: 'role dialogue tutor student review',
        limit: 10,
        compact: false,
      });
      const leads = (full.drive_state?.resource_leads ?? []) as Array<Record<string, unknown>>;
      const sources = leads.map(lead => String(lead['source'] ?? ''));
      expect(sources.some(source => source.includes('self-reflection-dialogue.md'))).toBe(true);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

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
        includeView: true,
        check: false,
      });
      expect(injected.ok).toBe(true);
      expect(injected.files.some(file => file.endsWith('AGENTS.md'))).toBe(true);
      expect(readFileSync(join(dir, '.octocode', 'GOTCHAS.md'), 'utf8')).toContain('Token migration order matters');
      expect(readFileSync(join(dir, '.octocode', 'MEMORY.md'), 'utf8')).toContain('Total: 2 · Shown: 2');
      expect(readFileSync(join(dir, '.octocode', 'BOOKMARKS.md'), 'utf8')).toContain('https://example.com/auth-guide');
      expect(readFileSync(join(dir, '.octocode', 'BOOKMARKS.md'), 'utf8')).toContain('repo:bgauryy/octocode-mcp');
      const agentsMd = readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8');
      expect(agentsMd).toContain('Octocode Awareness Map');
      expect(agentsMd).toContain('Projection Health');
      expect(agentsMd).toContain('Root `AGENTS.md` should point here');
      expect(agentsMd).not.toContain('append a root `AGENTS.md`');
      expect(agentsMd).not.toContain('Read GOTCHAS + LEARN');
      expect(agentsMd).not.toContain('## Files Under Work');
      expect(agentsMd).not.toContain('## Active Exclusive Locks');
      expect(agentsMd).toContain('memory recall');
      expect(agentsMd).toMatch(/ask before editing root `AGENTS\.md`/i);
      expect(readFileSync(join(dir, '.octocode', 'references', 'repo-map.md'), 'utf8')).not.toContain(file);
      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as {
        schema_version: number;
        files: string[];
        budgets: { markdown: Record<string, { max_lines: number; actual_lines: number; within_budget: boolean }> };
      };
      expect(manifest.schema_version).toBe(1);
      expect(manifest.files).toContain('.octocode/awareness/manifest.json');
      const agentsBudget = manifest.budgets.markdown['AGENTS.md'];
      expect(agentsBudget).toMatchObject({ max_lines: 80, within_budget: true });
      expect(agentsBudget?.actual_lines).toBeGreaterThan(0);
      expect(manifest.budgets.markdown['BOOKMARKS.md']).toMatchObject({ max_lines: 200, within_budget: true });
      const attend = attendAwareness(db, { workspacePath: dir, artifact: 'svc', compact: false });
      const projectionFiles = ((attend.organ_state?.senses as Record<string, unknown>).projection_health as Array<{ file: string }>).map(row => row.file);
      expect(projectionFiles).toEqual(expect.arrayContaining(['.octocode/BOOKMARKS.md', '.octocode/awareness/manifest.json']));
      expect(attend.bloat_warnings ?? []).not.toContain('manifest older than generated projection files; regenerate repo projection');
      expect(readFileSync(join(dir, '.octocode', 'awareness', 'csv', 'files.csv'), 'utf8')).toContain('auth.ts');
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
        includeView: true,
        check: false,
      });
      expect(readFileSync(join(dir, '.octocode', 'GOTCHAS.md'), 'utf8')).toContain(`Missing refs: file:${missing}:27`);
      expect(readFileSync(join(dir, '.octocode', 'BOOKMARKS.md'), 'utf8')).toContain('[missing file]');
      expect(readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8')).toContain('MissingFiles 1');
      const html = readFileSync(join(dir, '.octocode', 'awareness', 'index.html'), 'utf8');
      expect(html).toContain('id="global-filter"');
      expect(html).toContain('id="missing-filter"');
      expect(html).toContain('data-missing="true"');
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

  it('surfaces instruction feedback via the developer-review view and DEVELOPER_REVIEW.md projection', () => {
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

      const devReview = readFileSync(join(dir, '.octocode', 'DEVELOPER_REVIEW.md'), 'utf8');
      expect(devReview).toContain('# Developer Review');
      expect(devReview).toContain('default lock TTL');
      expect(devReview).toContain('Open: 1');

      const agentsMd = readFileSync(join(dir, '.octocode', 'AGENTS.md'), 'utf8');
      expect(agentsMd).toContain('Retro Files Map');
      expect(agentsMd).toContain('.octocode/DEVELOPER_REVIEW.md');

      const manifest = JSON.parse(readFileSync(join(dir, '.octocode', 'awareness', 'manifest.json'), 'utf8')) as {
        counts: Record<string, number>;
        budgets: { markdown: Record<string, { within_budget: boolean }> };
      };
      expect(manifest.counts['developer-review']).toBe(1);
      expect(manifest.budgets.markdown['DEVELOPER_REVIEW.md']).toMatchObject({ within_budget: true });
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
