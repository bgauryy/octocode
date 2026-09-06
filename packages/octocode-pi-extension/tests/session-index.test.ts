import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { createSessionArtifactContext, SESSION_ARTIFACT_VERSION } from '../src/tools/session-artifacts.js';
import {
  initializeSessionIndexes,
  projectSessionPlan,
  readSessionIndex,
} from '../src/tools/session-index.js';
import type { PlanReadModelV1 } from '../src/tools/plan-read-model.js';

const roots: string[] = [];

function tempRoot(prefix: string): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  roots.push(root);
  return root;
}

function readJson<T>(file: string): T {
  return JSON.parse(fs.readFileSync(file, 'utf8')) as T;
}

afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

test('initializes safe session, plan, task, backlog, memory, and audit projections with explicit ids', () => {
  const workspace = tempRoot('octocode-session-index-workspace-');
  const octocodeHome = tempRoot('octocode-session-index-home-');
  const sessionId = '../unsafe/private/session-id';
  const ctx = createSessionArtifactContext({
    cwd: workspace,
    octocodeHome,
    sessionManager: { getSessionId: () => sessionId },
  });

  const session = initializeSessionIndexes(ctx);

  assert.equal(session.version, SESSION_ARTIFACT_VERSION);
  assert.equal(session.sessionId, sessionId);
  assert.equal(session.sessionKey, ctx.identity.sessionKey);
  assert.match(session.backlogId, /^pi-backlog-[a-f0-9]{24}$/);
  assert.equal(session.activePlanId, null);
  assert.deepEqual(session.taskIds, []);
  const sessionsRoot = path.join(octocodeHome, 'extension', 'sessions');
  assert.equal(path.dirname(ctx.root), sessionsRoot);
  assert.equal(path.relative(sessionsRoot, ctx.root).includes(path.sep), false);
  assert.notEqual(path.basename(ctx.root), sessionId);

  for (const relative of [
    'session.json',
    'memory.md',
    'plan/index.json',
    'tasks/index.json',
    'backlog/index.json',
    'audit.md',
  ]) {
    assert.equal(fs.existsSync(ctx.resolve(relative)), true, `${relative} must exist`);
  }

  const manifest = ctx.inspect();
  const documentVersions = [
    manifest?.version,
    session.version,
    readJson<{ version: number }>(ctx.resolve('plan/index.json')).version,
    readJson<{ version: number }>(ctx.resolve('tasks/index.json')).version,
    readJson<{ version: number }>(ctx.resolve('backlog/index.json')).version,
  ];
  assert.deepEqual([...new Set(documentVersions)], [SESSION_ARTIFACT_VERSION]);
  assert.equal(manifest?.sessionId, sessionId);
  assert.equal(manifest?.backlogId, session.backlogId);
  assert.ok(manifest?.producers['session-index']?.paths.includes('session.json'));
  assert.deepEqual(readSessionIndex(ctx), session);
});

test('uses an opaque document id when session identity falls back to a private session-file path', () => {
  const workspace = tempRoot('octocode-session-file-workspace-');
  const octocodeHome = tempRoot('octocode-session-file-home-');
  const sessionFile = path.join(workspace, 'private', 'session.jsonl');
  const ctx = createSessionArtifactContext({
    cwd: workspace,
    octocodeHome,
    sessionManager: { getSessionFile: () => sessionFile },
  });

  const session = initializeSessionIndexes(ctx);
  const manifestText = fs.readFileSync(ctx.manifestPath, 'utf8');
  const sessionText = fs.readFileSync(ctx.resolve('session.json'), 'utf8');

  assert.match(session.sessionId, /^pi-session-[a-f0-9]{24}$/);
  assert.equal(manifestText.includes(sessionFile), false);
  assert.equal(sessionText.includes(sessionFile), false);
});

test('projects existing plan and task ids coherently while backlog tracks unfinished tasks in plan order', () => {
  const workspace = tempRoot('octocode-session-plan-workspace-');
  const octocodeHome = tempRoot('octocode-session-plan-home-');
  const ctx = createSessionArtifactContext({
    cwd: workspace,
    octocodeHome,
    sessionManager: { getSessionId: () => 'session-123' },
  });
  const initial = initializeSessionIndexes(ctx);
  const model: PlanReadModelV1 = {
    version: 1,
    planId: 'pi-plan-123',
    phase: 'executing',
    summary: { total: 3, done: 1, running: 1, blocked: 0 },
    tasks: [
      { id: 'task-a', index: 1, text: 'First', status: 'done', dependsOn: [] },
      { id: 'task-b', index: 2, text: 'Second', status: 'doing', dependsOn: [1], acceptance: 'It works' },
      { id: 'task-c', index: 3, text: 'Third', status: 'todo', dependsOn: [2], checkCommand: 'true' },
    ],
    review: {
      branchSnapshotId: 'snapshot-1',
      generation: 1,
      blockingQuestions: 0,
      unresolvedComments: 0,
      decisions: [],
      questions: [],
      comments: [],
    },
    coordination: { mode: 'local', sourcePlanKey: 'pi-plan-123', workspace },
    authorization: {},
    pendingInteractionIds: [],
    runtime: { turnsSinceUpdate: 0 },
  };

  projectSessionPlan(ctx, model);

  const session = readJson<{
    sessionId: string;
    backlogId: string;
    activePlanId: string | null;
    taskIds: string[];
  }>(ctx.resolve('session.json'));
  const plan = readJson<{ sessionId: string; planId: string | null; taskIds: string[]; status: string }>(ctx.resolve('plan/index.json'));
  const tasks = readJson<{
    sessionId: string;
    backlogId: string;
    planId: string | null;
    tasks: Array<{ taskId: string; planId: string; sessionId: string; backlogId: string; status: string; dependsOn: string[] }>;
  }>(ctx.resolve('tasks/index.json'));
  const backlog = readJson<{ backlogId: string; sessionId: string; planId: string | null; taskIds: string[]; openTaskIds: string[] }>(ctx.resolve('backlog/index.json'));

  assert.equal(session.sessionId, 'session-123');
  assert.equal(session.backlogId, initial.backlogId);
  assert.equal(session.activePlanId, model.planId);
  assert.deepEqual(session.taskIds, ['task-a', 'task-b', 'task-c']);
  assert.equal(plan.sessionId, session.sessionId);
  assert.equal(plan.planId, model.planId);
  assert.equal(plan.status, 'active');
  assert.deepEqual(plan.taskIds, session.taskIds);
  assert.equal(tasks.planId, model.planId);
  assert.deepEqual(tasks.tasks.map((task) => task.taskId), session.taskIds);
  assert.deepEqual(tasks.tasks[1]?.dependsOn, ['task-a']);
  assert.ok(tasks.tasks.every((task) => task.sessionId === session.sessionId));
  assert.ok(tasks.tasks.every((task) => task.backlogId === session.backlogId));
  assert.equal(backlog.backlogId, session.backlogId);
  assert.equal(backlog.sessionId, session.sessionId);
  assert.equal(backlog.planId, model.planId);
  assert.deepEqual(backlog.taskIds, session.taskIds);
  assert.deepEqual(backlog.openTaskIds, ['task-b', 'task-c']);
});

test('rejects legacy index versions instead of mixing schema generations', () => {
  const workspace = tempRoot('octocode-session-legacy-index-workspace-');
  const octocodeHome = tempRoot('octocode-session-legacy-index-home-');
  const ctx = createSessionArtifactContext({
    cwd: workspace,
    octocodeHome,
    sessionManager: { getSessionId: () => 'legacy-index-session' },
  });
  initializeSessionIndexes(ctx);
  fs.writeFileSync(ctx.resolve('plan/index.json'), `${JSON.stringify({
    version: 1,
    sessionId: 'legacy-index-session',
    planId: null,
    taskIds: [],
    status: 'none',
    updatedAt: '2026-01-01T00:00:00.000Z',
  }, null, 2)}\n`);

  assert.throws(() => initializeSessionIndexes(ctx), /Invalid session artifact version/);
});

test('rejects legacy manifest versions instead of upgrading them', () => {
  const workspace = tempRoot('octocode-session-legacy-workspace-');
  const octocodeHome = tempRoot('octocode-session-legacy-home-');
  const input = {
    cwd: workspace,
    octocodeHome,
    sessionManager: { getSessionId: () => 'legacy-session' },
  };
  const first = createSessionArtifactContext(input);
  fs.writeFileSync(first.manifestPath, `${JSON.stringify({
    version: 1,
    sessionKey: first.identity.sessionKey,
    identitySource: first.identity.source,
    workspace: first.identity.workspace,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    producers: {},
  }, null, 2)}\n`);

  assert.throws(() => createSessionArtifactContext(input), /Invalid session artifact manifest/);
});
