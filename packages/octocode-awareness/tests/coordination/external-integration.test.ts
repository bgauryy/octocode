import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { EXTERNAL_AGENT_AWARENESS_PROMPT, EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS, EXTERNAL_AGENT_AWARENESS_MARKER_END, EXTERNAL_AGENT_AWARENESS_MARKER_START, formatExternalAgentCoordinationContext, formatExternalAgentAwarenessInstructions, getExternalAgentAwarenessGuide } from '../../src/coordination/external-policy.js';
import { defaultDbPath } from '../../src/coordination/coordination-shared.js';
import { execCli } from '../../src/coordination/cli.js';
import { executeExternalMemoryAction, validateExternalMemoryParams } from '../../src/coordination/external-memory.js';
import { completeExternalPlanTask, finalizeExternalPlan, projectExternalPlan } from '../../src/coordination/external-plan.js';
import { openAwarenessStore } from '../../src/coordination/open.js';
import { readExternalAwarenessStatus } from '../../src/coordination/external-status.js';
import { globalAwarenessDatabasePath } from '../../src/storage-scope.js';

let workspace: string;
let previousOctocodeHome: string | undefined;

beforeEach(async () => {
  workspace = await mkdtemp(join(tmpdir(), 'aw-external-'));
  previousOctocodeHome = process.env.OCTOCODE_HOME;
  process.env.OCTOCODE_HOME = join(workspace, 'home');
});

afterEach(async () => {
  if (previousOctocodeHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = previousOctocodeHome;
  await rm(workspace, { recursive: true, force: true });
});

describe('external-agent integration boundary', () => {
  it('owns the prompt fragment and shared database path', () => {
    expect(EXTERNAL_AGENT_AWARENESS_PROMPT).toContain('<awareness>');
    expect(EXTERNAL_AGENT_AWARENESS_PROMPT).toContain('coordination evidence');
    expect(defaultDbPath(workspace)).toBe(globalAwarenessDatabasePath());
  });

  it('serves the same agent guidance through the CLI and advertises it in help', () => {
    const guide = execCli(['guide']);
    expect(guide).toEqual({ code: 0, stdout: `${EXTERNAL_AGENT_AWARENESS_PROMPT}\n`, stderr: '' });
    expect(execCli(['--help']).stdout).toContain('guide');
    const dynamic = getExternalAgentAwarenessGuide();
    expect(dynamic.prompt).toBe(EXTERNAL_AGENT_AWARENESS_PROMPT);
    expect(dynamic.commands).not.toHaveLength(0);
    expect(dynamic.commands.every((entry) => entry.cli.startsWith('npx @octocodeai/octocode-awareness '))).toBe(true);
    expect(JSON.parse(execCli(['guide', '--json']).stdout)).toEqual(dynamic);
  });

  it('exports reusable prompt and AGENTS.md instruction blocks without touching files', () => {
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).toContain('attend`');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).toContain('work start`');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).toContain('work end`');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).toContain('verify mark`');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).toContain('verify audit`');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).toContain('reflect record`');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).not.toContain('Use `next`, `inspect`, `verify`, and `close`');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).toContain('separate Awareness database under Octocode home and workspace-scoped columns');
    expect(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS).not.toContain('compatibility default is global');
    expect(execCli(['instructions', 'export'])).toEqual({
      code: 0,
      stdout: `${EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS}\n`,
      stderr: '',
    });

    const agentsMd = formatExternalAgentAwarenessInstructions('agents-md');
    expect(agentsMd).toContain(EXTERNAL_AGENT_AWARENESS_MARKER_START);
    expect(agentsMd).toContain(EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS);
    expect(agentsMd).toContain(EXTERNAL_AGENT_AWARENESS_MARKER_END);
    expect(execCli(['instructions', 'export', '--format', 'agents-md']).stdout).toBe(`${agentsMd}\n`);
    expect(JSON.parse(execCli(['instructions', 'export', '--format', 'json']).stdout)).toEqual({
      format: 'prompt',
      instructions: EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS,
    });
    expect(execCli(['instructions', 'export', '--format', 'bad'])).toMatchObject({
      code: 1,
      stderr: 'instructions export --format must be prompt, agents-md, or json',
    });
    expect(execCli(['--help']).stdout).toContain('instructions export');
  });

  it('projects, completes, verifies, and finalizes a shared plan through package adapters', () => {
    expect(projectExternalPlan({
      requestedScope: 'session', workspace, sourcePlanKey: 'local', title: 'local', agentId: 'agent-a', steps: [],
    })).toEqual({ scope: 'session', adopted: false });

    const projected = projectExternalPlan({
      requestedScope: 'shared',
      workspace,
      sourcePlanKey: 'shared-plan',
      title: 'Shared adapter plan',
      goal: 'Finish the shared adapter plan with verified task runs.',
      agentId: 'agent-a',
      steps: [
        { id: 'one', text: 'Implement adapter', status: 'doing', paths: ['src/adapter.ts'], reasoning: 'The adapter must use canonical task runs.', acceptance: 'Adapter behavior is verified.', checkCommand: 'yarn test' },
        { id: 'two', text: 'Document adapter', status: 'todo', dependsOnStepIds: ['one'], paths: ['docs/adapter.md'], reasoning: 'The durable flow needs host documentation.', acceptance: 'Documentation describes the completed flow.' },
      ],
    });
    expect(projected).toMatchObject({ scope: 'shared', adopted: false });
    const planId = projected.awarenessPlanId!;
    const firstId = projected.taskIdsByStepId!.one!;
    const secondId = projected.taskIdsByStepId!.two!;
    expect(finalizeExternalPlan({ workspace, planId, agentId: 'agent-a' })).toBe(false);

    expect(completeExternalPlanTask({
      workspace,
      taskId: firstId,
      agentId: 'agent-a',
      receipt: { command: 'yarn test', status: 'SUCCESS', message: 'focused tests passed' },
    })).toMatchObject({ verified: true });
    expect(completeExternalPlanTask({ workspace, taskId: firstId, agentId: 'agent-a' }))
      .toMatchObject({ verified: true });

    const aw = openAwarenessStore({ workspace });
    expect(aw.getPlan(planId)).toMatchObject({ sourceKind: 'external-agent', sourceKey: 'shared-plan' });
    aw.claimTask({ taskId: secondId, agentId: 'agent-a' });
    aw.close();
    expect(completeExternalPlanTask({ workspace, taskId: secondId, agentId: 'agent-a' }))
      .toMatchObject({ verified: true });
    expect(finalizeExternalPlan({ workspace, planId, agentId: 'agent-a' })).toBe(true);
  });

  it('formats host-neutral dynamic peer identity without duplicating commands', () => {
    const context = formatExternalAgentCoordinationContext({ selfId: 'worker-a', parentId: 'lead', peerIds: ['worker-a', 'worker-b'] });
    expect(context).toContain('your agent id: worker-a');
    expect(context).toContain('parent agent id: lead');
    expect(context).toContain('peers: worker-b');
    expect(context).toContain('npx @octocodeai/octocode-awareness guide');
    expect(context).not.toContain('message send');
  });

  it('projects typed status, task activity, and peer messages in one read', () => {
    const aw = openAwarenessStore({ workspace });
    const plan = aw.createPlan({ title: 'shared change', goal: 'Coordinate the shared change.', agentId: 'agent-a' });
    const doing = aw.addTask({ planId: plan.planId, title: 'owned task', paths: ['src/owned.ts'], reasoning: 'Agent A owns the change.', acceptance: 'Owned behavior is verified.', agentId: 'agent-a' });
    aw.addTask({ planId: plan.planId, title: 'ready task', paths: ['src/ready.ts'], reasoning: 'A follow-up remains ready.', acceptance: 'Follow-up behavior is verified.', agentId: 'agent-a' });
    aw.claimTask({ taskId: doing.taskId, agentId: 'agent-a' });
    aw.joinAgent({ agentId: 'agent-a' });
    aw.joinAgent({ agentId: 'agent-b' });
    aw.sendMessage({ fromAgentId: 'agent-b', toAgentId: 'agent-a', text: 'Please preserve the adapter.' });
    aw.close();

    expect(readExternalAwarenessStatus({ workspace, agentId: 'agent-a' })).toMatchObject({
      activePlans: 1,
      readyTasks: 1,
      inProgressTasks: 1,
      agentCount: 2,
      messageCount: 1,
      unreadInbox: 1,
      taskActivities: [
        { taskId: doing.taskId, title: 'owned task', state: 'doing', agentId: 'agent-a' },
        { title: 'ready task', state: 'ready' },
      ],
      lastMessage: { from: 'agent-b', to: 'agent-a', preview: 'Please preserve the adapter.' },
      lastInbound: { from: 'agent-b', preview: 'Please preserve the adapter.' },
    });
  });

  it('validates and executes the complete memory workflow without CLI serialization', () => {
    expect(() => validateExternalMemoryParams({ action: 'record', label: 'GOTCHA', observation: 'short', importance: 8 }))
      .toThrow(/too short/);

    const recorded = executeExternalMemoryAction({
      workspace,
      params: {
        action: 'record',
        label: 'GOTCHA',
        observation: 'The shared adapter must own memory validation and storage.',
        importance: 8,
        source: 'tests/coordination/external-integration.test.ts',
        tags: ['awareness'],
      },
    });
    expect(recorded).toMatchObject({ action: 'record', count: 1 });

    const recalled = executeExternalMemoryAction({ workspace, params: { action: 'recall', query: 'shared adapter' } });
    expect(recalled).toMatchObject({ action: 'recall', count: 1 });

    const review = executeExternalMemoryAction({ workspace, params: { action: 'review' } });
    expect(review).toMatchObject({ action: 'review', count: 1, candidates: [] });

    const suggestion = executeExternalMemoryAction({
      workspace,
      params: {
        action: 'suggest',
        observation: 'Use the package-owned adapter for durable memory operations.',
        changedFiles: ['packages/octocode-awareness/src/coordination/external-memory.ts'],
      },
    });
    expect(suggestion).toMatchObject({
      action: 'suggest',
      candidate: { action: 'record', label: 'EXPERIENCE', tags: ['octocode-awareness', 'external-memory'] },
    });

    const forgotten = executeExternalMemoryAction({
      workspace,
      params: { action: 'forget', memoryId: recorded.memoryId },
    });
    expect(forgotten).toMatchObject({ action: 'forget', deleted: 1 });
  });
});
