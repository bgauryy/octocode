import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { openAwarenessStore } from '../../src/coordination/open.js';
import type { AwarenessStore } from '../../src/coordination/coordination-continuity.js';
import { AWARENESS_APPLICATION_ID } from '../../src/storage-scope.js';

let workspace: string;
let aw: AwarenessStore;

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-lite-'));
  aw = openAwarenessStore({ workspace, scope: 'repo' });
});

afterEach(async () => {
  aw.close();
  await rm(workspace, { recursive: true, force: true });
});

describe('agent naming', () => {
  it('generates funny host-tagged names and detects hosts from env', async () => {
    const { detectAgentHost, generateAgentName } = await import('../../src/coordination/agent-naming.js');
    expect(detectAgentHost({ CLAUDECODE: '1' })).toBe('claude');
    expect(detectAgentHost({ CURSOR_TRACE_ID: 'x', TERM_PROGRAM: 'vscode' })).toBe('cursor');
    expect(detectAgentHost({ CODEX_THREAD_ID: 'x' })).toBe('codex');
    expect(detectAgentHost({ TERM_PROGRAM: 'vscode' })).toBe('vscode');
    expect(detectAgentHost({ OCTOCODE_AGENT_HOST: 'octo', CLAUDECODE: '1' })).toBe('octo');
    expect(detectAgentHost({})).toBe('agent');
    expect(generateAgentName({ CLAUDECODE: '1' })).toMatch(/^clawde-\w+$/);
    expect(generateAgentName({ OCTOCODE_AGENT_HOST: 'octo' })).toMatch(/^octo-\w+$/);
    expect(generateAgentName({ CURSOR_AGENT: '1' })).toMatch(/^cursea-\w+$/);
  });

  it('joinAgent defaults to a generated host-tagged name and keeps it on rejoin', () => {
    const joined = aw.joinAgent({ agentId: 'anon-1' });
    expect(joined.name).toMatch(/^[a-z]+-\w+$/);
    const rejoined = aw.joinAgent({ agentId: 'anon-1' });
    expect(rejoined.name).toBe(joined.name);
    // Explicit names always win, including over a previously generated one.
    expect(aw.joinAgent({ agentId: 'anon-1', name: 'my-bot' }).name).toBe('my-bot');
    expect(aw.joinAgent({ agentId: 'named', name: 'Alice' }).name).toBe('Alice');
  });

  it('creates only Awareness tables with the OCT1 identity', () => {
    const raw = new DatabaseSync(aw.dbPath);
    try {
      const names = raw
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .all()
        .map((r) => (r as { name: string }).name);
      expect(names).toEqual(expect.arrayContaining([
        'handoffs', 'awareness_memories', 'memory_refs', 'awareness_agents', 'signals', 'signal_reads',
        'awareness_plans', 'awareness_tasks', 'awareness_locks', 'task_runs', 'run_files',
      ]));
      expect(names).not.toEqual(expect.arrayContaining([
        'plans', 'tasks', 'locks', 'work_presence', 'memories', 'agents', 'messages', 'message_receipts',
      ]));
      expect(names).not.toEqual(expect.arrayContaining([
        'octocode_meta', 'agent_sessions', 'mcp_server_overrides',
        'mcp_tool_overrides', 'skill_overrides', 'mcp_catalog_state',
      ]));
      expect(raw.prepare('PRAGMA application_id').get())
        .toEqual({ application_id: AWARENESS_APPLICATION_ID });
    } finally {
      raw.close();
    }
  });
});

describe('AwarenessStore cross-workspace isolation in an explicit Awareness file', () => {
  let root: string;
  let dbPath: string;
  let repoA: string;
  let repoB: string;
  let a: AwarenessStore;
  let b: AwarenessStore;

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'aw-lite-iso-'));
    dbPath = join(root, 'awareness.sqlite3');
    repoA = join(root, 'repo-a');
    repoB = join(root, 'repo-b');
    a = openAwarenessStore({ workspace: repoA, dbPath });
    b = openAwarenessStore({ workspace: repoB, dbPath });
  });

  afterEach(async () => {
    a.close();
    b.close();
    await rm(root, { recursive: true, force: true });
  });

  it('scopes plans, memories, and handoffs to their own workspace', () => {
    a.createPlan({ agentId: 'lead-a', title: 'plan-a', goal: 'Keep plan A scoped.' });
    b.createPlan({ agentId: 'lead-b', title: 'plan-b', goal: 'Keep plan B scoped.' });
    a.storeMemory({ label: 'OTHER', text: 'mem-a' });
    a.addHandoff({ agentId: 'x', summary: 'handoff-a' });

    expect(a.listPlans().map((p) => p.title)).toEqual(['plan-a']);
    expect(b.listPlans().map((p) => p.title)).toEqual(['plan-b']);
    expect(a.recallMemory({}).map((m) => m.text)).toEqual(['mem-a']);
    expect(b.recallMemory({})).toHaveLength(0);
    expect(b.listHandoffs()).toHaveLength(0);
    expect(a.status().plans).toBe(1);
    expect(b.status().plans).toBe(1);
  });

  it('keeps the same relative file lock independent across workspaces', () => {
    a.acquireLock({ filePath: 'src/app.ts', agentId: 'agent-a', reason: 'edit workspace A', testPlan: 'workspace A lock test' });
    // Same relative path, different repo → no conflict (absolute paths differ,
    // and rows are workspace-scoped).
    expect(() => b.acquireLock({ filePath: 'src/app.ts', agentId: 'agent-b', reason: 'edit workspace B', testPlan: 'workspace B lock test' })).not.toThrow();
    expect(a.listLocks()).toHaveLength(1);
    expect(b.listLocks()).toHaveLength(1);
    expect(a.listLocks()[0]?.agentId).toBe('agent-a');
    expect(b.listLocks()[0]?.agentId).toBe('agent-b');
  });

  it('lets the same agent id exist in two workspaces (composite key)', () => {
    a.joinAgent({ agentId: 'shared-id', name: 'in-a' });
    b.joinAgent({ agentId: 'shared-id', name: 'in-b' });
    expect(a.listAgents().map((ag) => ag.name)).toEqual(['in-a']);
    expect(b.listAgents().map((ag) => ag.name)).toEqual(['in-b']);
  });

  it('does not resolve another workspace task by id', () => {
    const plan = a.createPlan({ agentId: 'lead', title: 'p', goal: 'Keep task scope private.' });
    const task = a.addTask({ planId: plan.planId, title: 't', paths: ['src/t.ts'], agentId: 'lead', reasoning: 'Scope lookup.', acceptance: 'Task remains private.' });
    expect(a.getTask(task.taskId).title).toBe('t');
    expect(() => b.getTask(task.taskId)).toThrow(/task not found/);
  });
});
