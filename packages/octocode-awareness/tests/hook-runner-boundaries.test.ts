import { afterEach, describe, expect, it } from 'vitest';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { createAwarenessClient } from '../src/client.js';
import { connectDb } from '../src/host-api.js';
import { resolveDbPath } from '../src/db-runtime.js';
import { runPostEdit, runPreEdit } from '../src/hooks/edit-events.js';
import { runHookCommand } from '../src/hooks/runner.js';
import { writeWorkspacePolicy, workspacePolicyPath } from '../src/workspace-policy.js';
import { withEnabledAwarenessConfig } from './helpers/enabled-awareness-config.js';

const roots: string[] = [];
const originalHome = process.env.OCTOCODE_HOME;
const originalAgentDir = process.env.OCTOCODE_AGENT_DIR;
const originalAgentId = process.env.OCTOCODE_AGENT_ID;
const originalProfile = process.env.OCTOCODE_HOOK_PROFILE;

afterEach(() => {
  if (originalHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = originalHome;
  if (originalAgentDir === undefined) delete process.env.OCTOCODE_AGENT_DIR;
  else process.env.OCTOCODE_AGENT_DIR = originalAgentDir;
  if (originalAgentId === undefined) delete process.env.OCTOCODE_AGENT_ID;
  else process.env.OCTOCODE_AGENT_ID = originalAgentId;
  if (originalProfile === undefined) delete process.env.OCTOCODE_HOOK_PROFILE;
  else process.env.OCTOCODE_HOOK_PROFILE = originalProfile;
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture(profile: 'guard' | 'coordination' | 'full' = 'guard') {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-hook-boundaries-')));
  roots.push(root);
  const workspace = join(root, 'workspace');
  mkdirSync(workspace, { recursive: true });
  process.env.OCTOCODE_HOME = root;
  process.env.OCTOCODE_AGENT_DIR = root;
  withEnabledAwarenessConfig({ OCTOCODE_HOME: root });
  writeWorkspacePolicy(workspace, {
    version: 1,
    storage: { repository: 'repo', memory: 'repo' },
    hooks: { profile },
  });
  return { root, workspace };
}

function writePayload(workspace: string, file: string, extra: Record<string, unknown> = {}) {
  return JSON.stringify({
    cwd: workspace,
    tool_name: 'Write',
    tool_use_id: `write-${file}`,
    tool_input: { path: file },
    ...extra,
  });
}

describe('published hook runner boundaries', () => {
  it('reports help and keeps disabled, native-owned, invalid-policy, and invalid-profile hooks inert', async () => {
    expect(await runHookCommand('help', '')).toBe(0);
    expect(await runHookCommand('--help', '')).toBe(0);
    expect(await runHookCommand('-h', '')).toBe(0);
    expect(await runHookCommand('retired-hook', '{}')).toBe(1);

    const { root, workspace } = fixture('guard');
    process.env.OCTOCODE_AGENT_ID = 'boundary-agent';
    writeWorkspacePolicy(workspace, {
      version: 1,
      storage: { repository: 'repo', memory: 'repo' },
      hooks: { profile: 'guard', owners: { claude: 'native' } },
    });
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/native.ts'), { host: 'claude' })).toBe(0);
    writeWorkspacePolicy(workspace, {
      version: 1,
      storage: { repository: 'repo', memory: 'repo' },
      hooks: { profile: 'guard' },
    });
    expect(await runHookCommand('notify-deliver', JSON.stringify({ cwd: workspace }), { host: 'claude' })).toBe(0);

    process.env.OCTOCODE_HOOK_PROFILE = 'retired';
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/profile.ts'), { host: 'claude' })).toBe(0);
    delete process.env.OCTOCODE_HOOK_PROFILE;

    writeFileSync(join(root, 'awareness.json'), '{bad json');
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/config.ts'), { host: 'claude' })).toBe(0);
    withEnabledAwarenessConfig({ OCTOCODE_HOME: root });

    const policyPath = workspacePolicyPath(workspace);
    mkdirSync(dirname(policyPath), { recursive: true });
    writeFileSync(policyPath, '{bad json');
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/policy.ts'), { host: 'claude' })).toBe(0);
  });

  it('normalizes inferred host events and rejects malformed or phase-mismatched payloads without writes', async () => {
    const { workspace } = fixture('guard');
    process.env.OCTOCODE_AGENT_ID = 'dialect-agent';
    for (const host of ['claude', 'cursor', 'gemini', 'opencode'] as const) {
      const payload = JSON.stringify({ cwd: workspace, tool_name: 'Read', tool_input: { path: 'src/read.ts' } });
      expect(await runHookCommand('pre-edit', payload, { host })).toBe(0);
      expect(await runHookCommand('post-edit', payload, { host })).toBe(0);
    }
    expect(await runHookCommand('pre-edit', JSON.stringify({
      cwd: workspace, hook_event_name: 'NotAClaudeEvent', tool_name: 'Write', tool_input: { path: 'src/a.ts' },
    }), { host: 'claude' })).toBe(0);
    expect(await runHookCommand('pre-edit', JSON.stringify({
      cwd: workspace, hook_event_name: 'PostToolUse', tool_name: 'Write', tool_input: { path: 'src/a.ts' },
    }), { host: 'claude' })).toBe(0);
  });

  it('fails closed for unidentified writes and blocks harness self-edits and peer lock conflicts', async () => {
    const { workspace } = fixture('guard');
    delete process.env.OCTOCODE_AGENT_ID;
    expect(await runHookCommand('pre-edit', JSON.stringify({
      cwd: workspace, tool_name: 'Read', tool_input: { path: 'src/read.ts' },
    }), { host: 'claude' })).toBe(0);
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/unidentified.ts'), { host: 'claude' })).toBe(1);

    process.env.OCTOCODE_AGENT_ID = 'peer';
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'SKILL.md'), {
      host: 'claude', skillRoot: workspace,
    })).toBe(2);

    const owner = createAwarenessClient({ workspace, agentId: 'owner' });
    const protectedFile = await owner.execute({
      operation: 'work.protect',
      params: {
        action: 'acquire', target_file: ['src/protected.ts'], rationale: 'Exclusive owner edit',
        test_plan: 'focused hook gate test', ttl_seconds: 60,
      },
    });
    expect(protectedFile.exitCode, JSON.stringify(protectedFile.payload)).toBe(0);
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/protected.ts'), { host: 'claude' })).toBe(2);
  });

  it('settles non-aggregated hook work, refreshes a partially failed run, and touches explicit work', async () => {
    const { workspace } = fixture('guard');
    process.env.OCTOCODE_AGENT_ID = 'lifecycle-agent';
    const multi = {
      cwd: workspace,
      tool_name: 'multi_edit',
      tool_use_id: 'partial-edit',
      tool_input: { path: 'src/a.ts', filePath: 'src/b.ts' },
    };
    expect(await runHookCommand('pre-edit', JSON.stringify(multi), { host: 'claude' })).toBe(0);
    expect(await runHookCommand('post-edit', JSON.stringify({
      ...multi,
      hook_event_name: 'PostToolUseFailure',
      is_error: true,
      tool_input: { path: 'src/a.ts' },
    }), { host: 'claude' })).toBe(0);
    expect(await runHookCommand('post-edit', JSON.stringify({
      ...multi,
      hook_event_name: 'PostToolUse',
      tool_input: { path: 'src/b.ts' },
    }), { host: 'claude' })).toBe(0);

    const client = createAwarenessClient({ workspace, agentId: 'lifecycle-agent' });
    const created = await client.execute({
      operation: 'work.create',
      params: {
        kind: 'standalone', file: ['src/work.ts'], rationale: 'Explicit host work',
        test_plan: 'focused hook lifecycle test', ttl_seconds: 60,
      },
    });
    expect(created.exitCode, JSON.stringify(created.payload)).toBe(0);
    expect(await runHookCommand('post-edit', writePayload(workspace, 'src/work.ts', {
      hook_event_name: 'PostToolUse',
    }), { host: 'claude' })).toBe(0);

    const database = connectDb(resolveDbPath(null, { workspace, scope: 'repo' }));
    try {
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'HOOK' AND status = 'PENDING'").get())
        .toEqual({ count: 1 });
      expect(database.prepare("SELECT COUNT(*) AS count FROM task_runs WHERE origin = 'WORK' AND status = 'ACTIVE'").get())
        .toEqual({ count: 1 });
      expect(database.prepare("SELECT COUNT(*) AS count FROM event_outbox WHERE event_type = 'workspace.edit.update'").get())
        .toEqual({ count: 2 });
    } finally {
      database.close();
    }
  });

  it('attaches pre-edits to explicit work and active task claims while emitting peer context', async () => {
    const { workspace } = fixture('full');
    process.env.OCTOCODE_AGENT_ID = 'worker';
    const worker = createAwarenessClient({ workspace, agentId: 'worker' });
    const peer = createAwarenessClient({ workspace, agentId: 'peer' });
    expect((await worker.execute({
      operation: 'work.create',
      params: {
        kind: 'standalone', file: ['src/explicit.ts'], rationale: 'Explicit edit',
        test_plan: 'focused explicit test', ttl_seconds: 60,
      },
    })).exitCode).toBe(0);
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/explicit.ts'), { host: 'claude' })).toBe(0);

    const plan = await worker.execute({
      operation: 'work.create', params: { kind: 'plan', name: 'Hook task', objective: 'Bind hook edits to the task claim' },
    });
    expect(plan.exitCode).toBe(0);
    const planId = String((plan.payload as { plan_id: string }).plan_id);
    const task = await worker.execute({
      operation: 'work.create',
      params: {
        kind: 'task', plan_id: planId, title: 'Claimed hook edit', path: ['src/task.ts'],
        reasoning: 'Exercise task-bound hook edits.', acceptance: 'The hook uses the claim run.',
      },
    });
    expect(task.exitCode).toBe(0);
    expect((await worker.execute({
      operation: 'work.claim', params: { task_id: String((task.payload as { task_id: string }).task_id), test_plan: 'focused task test' },
    })).exitCode).toBe(0);
    expect((await peer.execute({
      operation: 'work.create',
      params: {
        kind: 'standalone', file: ['src/shared.ts'], rationale: 'Peer context',
        test_plan: 'focused peer test', ttl_seconds: 60,
      },
    })).exitCode).toBe(0);
    expect(await runHookCommand('pre-edit', writePayload(workspace, 'src/shared.ts'), { host: 'claude' })).toBe(0);
  });

  it('treats empty and uncorrelated edits as no-ops and degrades on an unusable workspace', async () => {
    const { root, workspace } = fixture('guard');
    process.env.OCTOCODE_AGENT_ID = 'boundary-agent';
    expect(await runPreEdit({ cwd: workspace, agent_id: 'boundary-agent' })).toBe(0);
    expect(await runPostEdit({ cwd: workspace, agent_id: 'boundary-agent' })).toBe(0);
    expect(await runPostEdit({
      cwd: workspace, agent_id: 'boundary-agent', tool_name: 'Write', tool_input: { path: 'src/unclaimed.ts' },
    })).toBe(0);

    const notDirectory = join(root, 'not-a-directory');
    writeFileSync(notDirectory, 'file');
    expect(await runPreEdit({
      cwd: notDirectory, agent_id: 'boundary-agent', tool_name: 'Write', tool_input: { path: 'src/fails-open.ts' },
    })).toBe(0);
  });
});
