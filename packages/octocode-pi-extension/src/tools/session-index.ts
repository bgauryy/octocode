import fs from 'node:fs';
import { initializeSessionAudit, SESSION_AUDIT_RELATIVE_PATH } from './session-audit.js';
import { initializeSessionMemory, SESSION_MEMORY_RELATIVE_PATH } from './session-memory.js';
import type { PlanReadModelTaskV1, PlanReadModelV1 } from './plan-read-model.js';
import type { PlanPhase } from './plan-domain.js';
import { SESSION_ARTIFACT_VERSION, type SessionArtifactContext, type SessionIdentitySource } from './session-artifacts.js';

export const SESSION_INDEX_RELATIVE_PATH = 'session.json';
export const PLAN_INDEX_RELATIVE_PATH = 'plan/index.json';
export const TASK_INDEX_RELATIVE_PATH = 'tasks/index.json';
export const BACKLOG_INDEX_RELATIVE_PATH = 'backlog/index.json';

export interface SessionIndex {
  version: typeof SESSION_ARTIFACT_VERSION;
  sessionId: string;
  sessionKey: string;
  identitySource: SessionIdentitySource;
  workspace: string;
  backlogId: string;
  activePlanId: string | null;
  taskIds: string[];
  paths: {
    manifest: 'manifest.json';
    memory: typeof SESSION_MEMORY_RELATIVE_PATH;
    plan: typeof PLAN_INDEX_RELATIVE_PATH;
    tasks: typeof TASK_INDEX_RELATIVE_PATH;
    backlog: typeof BACKLOG_INDEX_RELATIVE_PATH;
    audit: typeof SESSION_AUDIT_RELATIVE_PATH;
  };
  updatedAt: string;
}

export type PlanIndexStatus = 'none' | 'draft' | 'approved' | 'active' | 'complete' | 'blocked' | 'failed';

export interface PlanIndex {
  version: typeof SESSION_ARTIFACT_VERSION;
  sessionId: string;
  planId: string | null;
  taskIds: string[];
  status: PlanIndexStatus;
  markdownPath?: 'plan/plan.md';
  htmlPath?: 'plan/plan.html';
  statePath?: 'plan/state.json';
  updatedAt: string;
}

export interface SessionTaskProjection {
  taskId: string;
  planId: string;
  sessionId: string;
  backlogId: string;
  title: string;
  status: PlanReadModelTaskV1['status'];
  dependsOn: string[];
  acceptance?: string;
  checkCommand?: string;
}

export interface TaskIndex {
  version: typeof SESSION_ARTIFACT_VERSION;
  sessionId: string;
  backlogId: string;
  planId: string | null;
  tasks: SessionTaskProjection[];
  updatedAt: string;
}

export interface BacklogIndex {
  version: typeof SESSION_ARTIFACT_VERSION;
  backlogId: string;
  sessionId: string;
  planId: string | null;
  taskIds: string[];
  openTaskIds: string[];
  updatedAt: string;
}

function readJson<T>(file: string): T | undefined {
  if (!fs.existsSync(file)) return undefined;
  return JSON.parse(fs.readFileSync(file, 'utf8')) as T;
}

function readCurrentIndex<T>(ctx: SessionArtifactContext, relativePath: string): T | undefined {
  const value = readJson<{ version?: unknown }>(ctx.resolve(relativePath));
  if (!value) return undefined;
  if (value.version !== SESSION_ARTIFACT_VERSION) {
    throw new Error(`Invalid session artifact version: ${ctx.resolve(relativePath)}`);
  }
  return value as T;
}

function writeJsonIfMissing(ctx: SessionArtifactContext, relativePath: string, value: unknown): void {
  if (!fs.existsSync(ctx.resolve(relativePath))) ctx.writeJson(relativePath, value);
}

function registerIndexPaths(ctx: SessionArtifactContext): void {
  for (const relativePath of [
    SESSION_INDEX_RELATIVE_PATH,
    PLAN_INDEX_RELATIVE_PATH,
    TASK_INDEX_RELATIVE_PATH,
    BACKLOG_INDEX_RELATIVE_PATH,
  ]) {
    const registered = ctx.inspect()?.producers['session-index']?.paths.includes(relativePath) === true;
    if (!registered) ctx.registerProducer('session-index', relativePath);
  }
}

function emptyIndexes(ctx: SessionArtifactContext, now: string): {
  session: SessionIndex;
  plan: PlanIndex;
  tasks: TaskIndex;
  backlog: BacklogIndex;
} {
  const manifest = ctx.inspect();
  if (!manifest) throw new Error('Session manifest is unavailable');
  const session: SessionIndex = {
    version: SESSION_ARTIFACT_VERSION,
    sessionId: manifest.sessionId,
    sessionKey: manifest.sessionKey,
    identitySource: manifest.identitySource,
    workspace: manifest.workspace,
    backlogId: manifest.backlogId,
    activePlanId: null,
    taskIds: [],
    paths: {
      manifest: 'manifest.json',
      memory: SESSION_MEMORY_RELATIVE_PATH,
      plan: PLAN_INDEX_RELATIVE_PATH,
      tasks: TASK_INDEX_RELATIVE_PATH,
      backlog: BACKLOG_INDEX_RELATIVE_PATH,
      audit: SESSION_AUDIT_RELATIVE_PATH,
    },
    updatedAt: now,
  };
  return {
    session,
    plan: { version: SESSION_ARTIFACT_VERSION, sessionId: session.sessionId, planId: null, taskIds: [], status: 'none', updatedAt: now },
    tasks: { version: SESSION_ARTIFACT_VERSION, sessionId: session.sessionId, backlogId: session.backlogId, planId: null, tasks: [], updatedAt: now },
    backlog: { version: SESSION_ARTIFACT_VERSION, backlogId: session.backlogId, sessionId: session.sessionId, planId: null, taskIds: [], openTaskIds: [], updatedAt: now },
  };
}

export function initializeSessionIndexes(ctx: SessionArtifactContext): SessionIndex {
  initializeSessionMemory(ctx);
  initializeSessionAudit(ctx);
  const initial = emptyIndexes(ctx, new Date().toISOString());
  writeJsonIfMissing(ctx, SESSION_INDEX_RELATIVE_PATH, initial.session);
  writeJsonIfMissing(ctx, PLAN_INDEX_RELATIVE_PATH, initial.plan);
  writeJsonIfMissing(ctx, TASK_INDEX_RELATIVE_PATH, initial.tasks);
  writeJsonIfMissing(ctx, BACKLOG_INDEX_RELATIVE_PATH, initial.backlog);

  const session = readSessionIndex(ctx);
  if (!session) throw new Error('Session index initialization failed');
  for (const relativePath of [PLAN_INDEX_RELATIVE_PATH, TASK_INDEX_RELATIVE_PATH, BACKLOG_INDEX_RELATIVE_PATH]) {
    if (!readCurrentIndex(ctx, relativePath)) throw new Error(`Session index initialization failed: ${relativePath}`);
  }
  registerIndexPaths(ctx);
  return session;
}

export function readSessionIndex(ctx: SessionArtifactContext): SessionIndex | undefined {
  const value = readCurrentIndex<SessionIndex>(ctx, SESSION_INDEX_RELATIVE_PATH);
  if (!value) return undefined;
  const manifest = ctx.inspect();
  if (value.sessionId !== manifest?.sessionId || value.sessionKey !== ctx.identity.sessionKey) {
    throw new Error(`Invalid session index: ${ctx.resolve(SESSION_INDEX_RELATIVE_PATH)}`);
  }
  return value;
}

function planStatus(phase: PlanPhase): PlanIndexStatus {
  if (phase === 'complete' || phase === 'abandoned') return 'complete';
  if (phase === 'blocked') return 'blocked';
  if (phase === 'failed') return 'failed';
  if (phase === 'accepted') return 'approved';
  if (phase === 'executing' || phase === 'verifying') return 'active';
  return 'draft';
}

function dependencyTaskIds(task: PlanReadModelTaskV1, tasks: PlanReadModelTaskV1[]): string[] {
  return task.dependsOn.map((index) => tasks[index - 1]?.id).filter((id): id is string => typeof id === 'string');
}

export function projectSessionPlan(ctx: SessionArtifactContext, model: PlanReadModelV1 | undefined): void {
  const current = initializeSessionIndexes(ctx);
  const now = new Date().toISOString();
  if (!model) {
    const empty = emptyIndexes(ctx, now);
    ctx.writeJson(SESSION_INDEX_RELATIVE_PATH, empty.session);
    ctx.writeJson(PLAN_INDEX_RELATIVE_PATH, empty.plan);
    ctx.writeJson(TASK_INDEX_RELATIVE_PATH, empty.tasks);
    ctx.writeJson(BACKLOG_INDEX_RELATIVE_PATH, empty.backlog);
    return;
  }

  const taskIds = model.tasks.map((task) => task.id);
  const tasks: SessionTaskProjection[] = model.tasks.map((task) => ({
    taskId: task.id,
    planId: model.planId,
    sessionId: current.sessionId,
    backlogId: current.backlogId,
    title: task.text,
    status: task.status,
    dependsOn: dependencyTaskIds(task, model.tasks),
    ...(task.acceptance ? { acceptance: task.acceptance } : {}),
    ...(task.checkCommand ? { checkCommand: task.checkCommand } : {}),
  }));
  const session: SessionIndex = {
    ...current,
    activePlanId: model.planId,
    taskIds,
    updatedAt: now,
  };
  const plan: PlanIndex = {
    version: SESSION_ARTIFACT_VERSION,
    sessionId: current.sessionId,
    planId: model.planId,
    taskIds,
    status: planStatus(model.phase),
    ...(fs.existsSync(ctx.resolve('plan/plan.md')) ? { markdownPath: 'plan/plan.md' as const } : {}),
    ...(fs.existsSync(ctx.resolve('plan/plan.html')) ? { htmlPath: 'plan/plan.html' as const } : {}),
    ...(fs.existsSync(ctx.resolve('plan/state.json')) ? { statePath: 'plan/state.json' as const } : {}),
    updatedAt: now,
  };
  const taskIndex: TaskIndex = {
    version: SESSION_ARTIFACT_VERSION,
    sessionId: current.sessionId,
    backlogId: current.backlogId,
    planId: model.planId,
    tasks,
    updatedAt: now,
  };
  const backlog: BacklogIndex = {
    version: SESSION_ARTIFACT_VERSION,
    backlogId: current.backlogId,
    sessionId: current.sessionId,
    planId: model.planId,
    taskIds,
    openTaskIds: tasks.filter((task) => task.status !== 'done').map((task) => task.taskId),
    updatedAt: now,
  };

  ctx.writeJson(SESSION_INDEX_RELATIVE_PATH, session);
  ctx.writeJson(PLAN_INDEX_RELATIVE_PATH, plan);
  ctx.writeJson(TASK_INDEX_RELATIVE_PATH, taskIndex);
  ctx.writeJson(BACKLOG_INDEX_RELATIVE_PATH, backlog);
}
