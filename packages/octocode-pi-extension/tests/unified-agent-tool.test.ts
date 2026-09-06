/**
 * unified-agent-tool.test.ts — Phase 4 focused tests.
 *
 * Coverage:
 *   schema       — queries[] envelope, reasoning required, type/profile enums
 *   dispatch     — correct handler invoked per operation type
 *   browser      — browser profile: routeTask routing + buildSpawnConfig delegation
 *   lifecycle    — spawn → agentId, kill → success, inspect list, message, steer
 *   guard        — same-batch cross-reference rejection, preflight errors
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition, ToolCallResult } from '../src/types.js';
import {
  AGENT_OPERATIONS,
  AGENT_PROFILES,
  rejectCrossBatchReference,
  type AgentOperation,
  type AgentProfile,
} from '../src/tools/unified-agent-tool.js';
import type { QueryRecord } from '../src/tools/query-envelope.js';
import * as planReadModel from '../src/tools/plan-read-model.js';
import {
  adoptPlanModePolicy,
  clearPlanModePoliciesForTests,
  enterPlanMode,
  evaluateToolCapability,
  getToolEffect,
  planModeToolGate,
} from '../src/tools/plan-mode.js';

// ─── Helpers ─────────────────────────────────────────────────────────────────

const typeBuilder = Type as unknown as (typeof import('typebox'))['Type'];

/** Build a minimal one-query batch for the tool's execute(). */
function batch(...queries: Record<string, unknown>[]): Record<string, unknown> {
  return {
    queries: queries.map((q) => ({ reasoning: 'test operation', ...q })),
  };
}

function planContext(sessionId: string): Record<string, unknown> {
  return {
    cwd: '/tmp/unified-agent-plan-policy',
    sessionManager: { getSessionId: () => sessionId },
  };
}

afterEach(() => clearPlanModePoliciesForTests());

/** Invoke a registered tool's execute() and return text + details. */
async function run(
  tool: ToolDefinition,
  rawParams: Record<string, unknown>,
  ctx?: Record<string, unknown>,
): Promise<{ text: string; result: ToolCallResult; details: unknown }> {
  const result = await (
    tool.execute as (
      id: string,
      raw: Record<string, unknown>,
      s: undefined,
      u: undefined,
      ctx: unknown,
    ) => Promise<ToolCallResult>
  )('tool-call-1', rawParams, undefined, undefined, ctx ?? {});
  const text =
    (result.content?.find((p) => p.type === 'text') as { text?: string } | undefined)
      ?.text ?? '';
  return { text, result, details: (result as unknown as { details?: unknown }).details };
}

// ─── Module-level mock helpers ────────────────────────────────────────────────
//
// Because unified-agent-tool imports agent-tools and browser-agent-tool, we
// mock those modules before importing the SUT so tests stay fully isolated from
// real process spawning and filesystem operations.

/** The shared mock record returned by the mocked spawnRpcAgent. */
const MOCK_RECORD = {
  id: 'mock-agent-001',
  name: 'Mock Worker',
  policyWarnings: [] as string[],
};

/** Transcript returned by getWorkerTranscript for known agents. */
const MOCK_TRANSCRIPT = '[STATUS] Mock agent idle.';

vi.mock('../src/tools/agent-tools.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../src/tools/agent-tools.js')>();
  const steerWorkerById = vi.fn((_id: string, _msg: string) => true);
  const killWorkerById = vi.fn((id: string) => id === MOCK_RECORD.id);
  const getWorkerTranscript = vi.fn(
    (id: string) => (id === MOCK_RECORD.id ? MOCK_TRANSCRIPT : undefined),
  );
  const refreshAgentLedgerUi = vi.fn();
  const executeAgentLifecycle = vi.fn(async (params: Record<string, unknown>) => {
    const action = String(params['type'] ?? '');
    const agentId = String(params['agentId'] ?? '');
    if (action === 'inspect' && !agentId) {
      refreshAgentLedgerUi();
      return { content: [{ type: 'text', text: 'mock-agent-001 · idle' }] };
    }
    if (agentId !== MOCK_RECORD.id) {
      throw new Error(`No agent found with id: ${agentId}.`);
    }
    if (action === 'inspect') return { content: [{ type: 'text', text: MOCK_TRANSCRIPT }] };
    if (action === 'wait') return { content: [{ type: 'text', text: `[WAIT snapshot · agentId:${agentId}]\n${MOCK_TRANSCRIPT}` }] };
    if (action === 'message') {
      steerWorkerById(agentId, String(params['message'] ?? ''));
      return { content: [{ type: 'text', text: `[MESSAGE] delivery:${params['delivery'] ?? 'send'} → ${agentId}: ${String(params['message'] ?? '')}` }] };
    }
    if (action === 'steer') {
      steerWorkerById(agentId, String(params['message'] ?? ''));
      return { content: [{ type: 'text', text: `[STEER] → ${agentId}` }] };
    }
    if (action === 'abort') return { content: [{ type: 'text', text: `[ABORT] ${agentId}` }] };
    if (action === 'kill') {
      if (!killWorkerById(agentId)) throw new Error(`No agent found with id: ${agentId}.`);
      refreshAgentLedgerUi();
      return { content: [{ type: 'text', text: `[KILL] ${agentId}` }] };
    }
    throw new Error(`unsupported test action: ${action}`);
  });
  return {
    ...original,
    isSubagentProcess: vi.fn(() => false),
    prepareSpawnAgentParams: vi.fn(async (params: unknown) => params),
    spawnRpcAgent: vi.fn(() => ({ ...MOCK_RECORD })),
    steerWorkerById,
    killWorkerById,
    getWorkerTranscript,
    formatAgentLedger: vi.fn(() => 'Octocode agents: 1 total · 1 idle'),
    formatAgentLedgerDetails: vi.fn(() => 'mock-agent-001 · idle'),
    refreshAgentLedgerUi,
    executeAgentLifecycle,
  };
});

vi.mock('../src/tools/browser-agent-tool.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../src/tools/browser-agent-tool.js')>();
  return {
    ...original,
    routeTask: vi.fn((task: string) => ({
      schemes: task.includes('security') ? ['security', 'network'] : ['debug', 'network'],
      cdpDomains: task.includes('security')
        ? ['Network', 'Runtime', 'DOMDebugger']
        : ['Network', 'Runtime', 'Log', 'DOM', 'Page'],
      contextKeys: ['network'],
    })),
    buildSpawnConfig: vi.fn((params: { task: string; cdpDomains: string[]; port: number }) => ({
      systemPrompt: `Browser specialist for: ${params.task}. Domains: ${params.cdpDomains.join(', ')}.`,
      tools: ['chromeDebug'],
      task: params.task,
    })),
  };
});

vi.mock('../src/subagents.js', async () => {
  return {
    SUBAGENT_REGISTRY: {
      researcher: {
        name: 'researcher',
        label: 'Researcher',
        tools: ['web', 'MCPTool', 'write'],
        resourceMode: 'octocode',
        thinking: 'low',
        systemPromptPath: '/mock/researcher/SYSTEM_PROMPT.md',
        extraSkillPaths: [],
      },
      planner: {
        name: 'planner',
        label: 'Planner',
        tools: ['web', 'MCPTool', 'write'],
        resourceMode: 'octocode',
        thinking: 'low',
        systemPromptPath: '/mock/planner/SYSTEM_PROMPT.md',
        extraSkillPaths: [],
      },
      architect: {
        name: 'architect',
        label: 'Architect',
        tools: ['bash', 'web', 'MCPTool', 'write'],
        resourceMode: 'octocode',
        thinking: 'medium',
        systemPromptPath: '/mock/architect/SYSTEM_PROMPT.md',
        extraSkillPaths: [],
      },
      'browser-agent': {
        name: 'browser-agent',
        label: 'Browser Agent',
        tools: ['chromeDebug'],
        resourceMode: 'octocode',
        thinking: 'low',
        systemPromptPath: '/mock/browser-agent/SYSTEM_PROMPT.md',
        extraSkillPaths: [],
      },
    },
    SUBAGENT_NAMES: ['researcher', 'planner', 'architect', 'browser-agent'],
    loadSystemPrompt: vi.fn(() => '# Mock System Prompt'),
    resolveSubagentSkills: vi.fn(() => []),
    getExternalSkillDirs: vi.fn(() => []),
    OCTOCODE_SKILL_NAMES: [],
  };
});

// Import SUT AFTER mocks are registered.
async function loadSut() {
  const { registerUnifiedAgentTool } = await import('../src/tools/unified-agent-tool.js');
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
  const registerFn = (
    targetPi: { registerTool?(def: ToolDefinition): void },
    names: Set<string>,
    def: ToolDefinition,
  ) => {
    names.add(def.name);
    targetPi.registerTool?.(def);
  };
  registerUnifiedAgentTool(pi, typeBuilder, new Set<string>(), registerFn as never);
  return tools;
}

// ─── Schema tests ─────────────────────────────────────────────────────────────

describe('schema', () => {
  it('registers exactly one tool named "agent"', async () => {
    const tools = await loadSut();
    expect(tools.has('agent')).toBe(true);
    expect(tools.size).toBe(1);
  });

  it('exposes only a top-level "queries" field (universal envelope contract)', async () => {
    const tools = await loadSut();
    const schema = tools.get('agent')!.parameters as {
      properties?: Record<string, unknown>;
      required?: string[];
    };
      expect(Object.keys(schema.properties ?? {})).toEqual(['queries', 'queryRunType']);
    expect(schema.required).toContain('queries');
  });

  it('queries array has minItems:1 and each item requires reasoning', async () => {
    const tools = await loadSut();
    const schema = tools.get('agent')!.parameters as {
      properties?: {
        queries?: {
          minItems?: number;
          items?: {
            properties?: Record<string, { minLength?: number; maxLength?: number }>;
            required?: string[];
          };
        };
      };
    };
    const q = schema.properties?.queries;
    expect(q?.minItems).toBe(1);
    const reasoning = q?.items?.properties?.['reasoning'];
    expect(reasoning?.minLength).toBe(1);
    expect(reasoning?.maxLength).toBe(240);
    expect(q?.items?.required).toContain('reasoning');
  });

  it('item schema carries a "type" field with the full operation enum', async () => {
    const tools = await loadSut();
    const schema = tools.get('agent')!.parameters as {
      properties?: {
        queries?: {
          items?: {
            properties?: Record<string, { enum?: string[] }>;
          };
        };
      };
    };
    const typeField = schema.properties?.queries?.items?.properties?.['type'];
    expect(typeField?.enum).toBeDefined();
    for (const op of AGENT_OPERATIONS) {
      expect(typeField?.enum).toContain(op);
    }
  });

  it('item schema carries a "profile" field with the full profile enum', async () => {
    const tools = await loadSut();
    const schema = tools.get('agent')!.parameters as {
      properties?: {
        queries?: {
          items?: {
            properties?: Record<string, { enum?: string[] }>;
          };
        };
      };
    };
    const profileField = schema.properties?.queries?.items?.properties?.['profile'];
    expect(profileField?.enum).toBeDefined();
    for (const p of AGENT_PROFILES) {
      expect(profileField?.enum).toContain(p);
    }
  });

  it('AGENT_OPERATIONS covers all seven expected ops', () => {
    const expected: AgentOperation[] = [
      'spawn', 'inspect', 'wait', 'message', 'steer', 'abort', 'kill',
    ];
    expect([...AGENT_OPERATIONS].sort()).toEqual([...expected].sort());
  });

  it('AGENT_PROFILES covers all five expected profiles', () => {
    const expected: AgentProfile[] = [
      'researcher', 'planner', 'architect', 'browser', 'custom',
    ];
    expect([...AGENT_PROFILES].sort()).toEqual([...expected].sort());
  });
});

// ─── Same-batch cross-reference guard ────────────────────────────────────────

describe('same-batch cross-reference guard', () => {
  function q(type: string, extra: Record<string, unknown> = {}): QueryRecord {
    return { reasoning: 'test', type, ...extra } as QueryRecord;
  }

  it('allows multiple independent spawns with no lifecycle', () => {
    expect(() =>
      rejectCrossBatchReference([
        q('spawn', { task: 'a' }),
        q('spawn', { task: 'b' }),
      ]),
    ).not.toThrow();
  });

  it('allows lifecycle-only batch with existing agentIds', () => {
    expect(() =>
      rejectCrossBatchReference([
        q('inspect', { agentId: 'existing-001' }),
        q('kill', { agentId: 'existing-002' }),
      ]),
    ).not.toThrow();
  });

  it('allows spawn + inspect WITHOUT agentId (list mode) in same batch', () => {
    expect(() =>
      rejectCrossBatchReference([q('spawn', { task: 'x' }), q('inspect')]),
    ).not.toThrow();
  });

  it('rejects spawn + lifecycle with explicit agentId in same batch', () => {
    expect(() =>
      rejectCrossBatchReference([
        q('spawn', { task: 'x' }),
        q('wait', { agentId: 'unknown-new-id' }),
      ]),
    ).toThrow(/same-batch cross-reference rejected/i);
  });

  it('rejects spawn + kill with explicit agentId in same batch', () => {
    expect(() =>
      rejectCrossBatchReference([
        q('spawn', { task: 'x' }),
        q('kill', { agentId: 'abc' }),
      ]),
    ).toThrow(/same-batch cross-reference rejected/i);
  });

  it('rejects spawn + message with explicit agentId in same batch', () => {
    expect(() =>
      rejectCrossBatchReference([
        q('spawn', { task: 'x' }),
        q('message', { agentId: 'abc', message: 'hi' }),
      ]),
    ).toThrow(/same-batch cross-reference rejected/i);
  });

  it('tool execute() enforces the guard before any side effect', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    await expect(
      run(tool, batch(
        { type: 'spawn', task: 'do something' },
        { type: 'wait', agentId: 'some-future-id' },
      )),
    ).rejects.toThrow(/same-batch cross-reference rejected/i);
  });
});

// ─── Dispatch: preflight errors ───────────────────────────────────────────────

describe('preflight', () => {
  it('rejects unknown type in preflight before execution', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    await expect(
      run(tool, batch({ type: 'noop' })),
    ).rejects.toThrow(/type must be one of/i);
  });

  it('rejects spawn without task in preflight', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    await expect(
      run(tool, batch({ type: 'spawn', profile: 'custom' })),
    ).rejects.toThrow(/spawn requires a non-empty task/i);
  });

  it('rejects empty queries array', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    await expect(run(tool, { queries: [] })).rejects.toThrow(/non-empty/i);
  });
});

describe('plan worker assignment', () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(() => vi.restoreAllMocks());

  function assignmentModel() {
    return planReadModel.buildPlanReadModel({
      steps: [
        { id: 'research', text: 'Research', status: 'todo', awarenessTaskId: 'shared-research' },
        { id: 'implement', text: 'Implement API', status: 'todo', awarenessTaskId: 'shared-implement', dependsOnStepIds: ['research'], paths: ['src/api.ts'], acceptance: 'API contract passes', checkCommand: 'yarn test api' },
      ],
      review: { phase: 'executing', branchSnapshotId: 'branch', generation: 1, decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'required', sourcePlanKey: 'plan-assignment', coordinationWorkspace: '/repo' },
      sharedTaskStatuses: { 'shared-research': 'DONE', 'shared-implement': 'IN_PROGRESS' },
    });
  }

  it('carries the canonical task contract using effective shared status', async () => {
    vi.spyOn(planReadModel, 'getCurrentPlanReadModel').mockReturnValue(assignmentModel());
    const agentTools = await import('../src/tools/agent-tools.js');
    const tools = await loadSut();
    await run(tools.get('agent')!, batch({ type: 'spawn', task: 'Build this', planStep: 'implement' }), planContext('assignment'));
    const params = vi.mocked(agentTools.spawnRpcAgent).mock.calls[0]![0];
    expect(params.task).toContain('plan-assignment');
    expect(params.task).toContain('implement');
    expect(params.task).toContain('src/api.ts');
    expect(params.task).toContain('API contract passes');
    expect(params.task).toContain('yarn test api');
  });

  it.each(['missing', 'todo', 'dependency', 'review', 'interaction'])(
    'rejects %s plan assignments before preparation or spawning', async (invalid) => {
      const model = assignmentModel();
      if (invalid === 'todo') model.tasks[1]!.status = 'todo';
      if (invalid === 'dependency') model.tasks[0]!.status = 'doing';
      if (invalid === 'review') model.phase = 'in_review';
      if (invalid === 'interaction') model.pendingInteractionIds = ['pending'];
      vi.spyOn(planReadModel, 'getCurrentPlanReadModel').mockReturnValue(model);
      const agentTools = await import('../src/tools/agent-tools.js');
      const tools = await loadSut();
      await expect(run(tools.get('agent')!, batch({ type: 'spawn', task: 'Build', planStep: invalid === 'missing' ? 'absent' : 'implement' }))).rejects.toThrow(/plan|depend|interaction/i);
      expect(agentTools.prepareSpawnAgentParams).not.toHaveBeenCalled();
      expect(agentTools.spawnRpcAgent).not.toHaveBeenCalled();
    },
  );

  it('revalidates plan identity after asynchronous spawn preparation', async () => {
    const current = assignmentModel();
    vi.spyOn(planReadModel, 'getCurrentPlanReadModel').mockImplementation(() => current);
    const agentTools = await import('../src/tools/agent-tools.js');
    vi.mocked(agentTools.prepareSpawnAgentParams).mockImplementationOnce(async (params) => {
      current.planId = 'replacement-plan';
      return params;
    });
    const tools = await loadSut();
    await expect(run(tools.get('agent')!, batch({ type: 'spawn', task: 'Build', planStep: 'implement' }))).rejects.toThrow(/plan.*chang/i);
    expect(agentTools.spawnRpcAgent).not.toHaveBeenCalled();
  });

  it.each(['owner', 'status', 'dependency'])('revalidates %s after asynchronous preparation', async (change) => {
    const current = assignmentModel();
    vi.spyOn(planReadModel, 'getCurrentPlanReadModel').mockImplementation(() => current);
    const agentTools = await import('../src/tools/agent-tools.js');
    const owner = vi.spyOn(agentTools, 'findLivePlanWorker').mockReturnValue(undefined);
    vi.mocked(agentTools.prepareSpawnAgentParams).mockImplementationOnce(async (params) => {
      if (change === 'owner') owner.mockReturnValue('existing-worker');
      if (change === 'status') current.tasks[1]!.status = 'done';
      if (change === 'dependency') current.tasks[0]!.status = 'doing';
      return params;
    });
    const tools = await loadSut();
    await expect(run(tools.get('agent')!, batch({ type: 'spawn', task: 'Build', planStep: 'implement' }))).rejects.toThrow(/plan|depend/i);
    expect(agentTools.spawnRpcAgent).not.toHaveBeenCalled();
  });

  it('leaves standalone spawning independent of plan state', async () => {
    const read = vi.spyOn(planReadModel, 'getCurrentPlanReadModel').mockImplementation(() => { throw new Error('should not read plan'); });
    const tools = await loadSut();
    await expect(run(tools.get('agent')!, batch({ type: 'spawn', task: 'Independent research' }))).resolves.toBeDefined();
    expect(read).not.toHaveBeenCalled();
  });

  it('does not create a process after cancellation during preparation', async () => {
    const controller = new AbortController();
    const agentTools = await import('../src/tools/agent-tools.js');
    vi.mocked(agentTools.prepareSpawnAgentParams).mockImplementationOnce(async (params) => {
      controller.abort();
      return params;
    });
    const tools = await loadSut();
    await expect(tools.get('agent')!.execute('cancel-spawn', batch({ type: 'spawn', task: 'Build' }), controller.signal)).rejects.toThrow(/abort/i);
    expect(agentTools.spawnRpcAgent).not.toHaveBeenCalled();
  });
});

describe('plan Start enforcement', () => {
  let agentTools: typeof import('../src/tools/agent-tools.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentTools = await import('../src/tools/agent-tools.js');
    tools = await loadSut();
    vi.clearAllMocks();
  });

  afterEach(() => vi.clearAllMocks());

  it.each([
    { type: 'inspect' },
    { type: 'wait', agentId: MOCK_RECORD.id },
    { type: 'message', agentId: MOCK_RECORD.id, message: 'continue' },
    { type: 'steer', agentId: MOCK_RECORD.id, message: 'new focus' },
    { type: 'abort', agentId: MOCK_RECORD.id },
    { type: 'kill', agentId: MOCK_RECORD.id },
  ])('allows coordination-only lifecycle operation $type before Start', async (query) => {
    const ctx = planContext(`lifecycle-${query.type}`);
    enterPlanMode(ctx as never);
    await expect(run(tools.get('agent')!, batch(query), ctx)).resolves.toBeDefined();
  });

  it('allows spawn and mixed lifecycle batches before Start', async () => {
    const ctx = planContext('planning-spawn');
    enterPlanMode(ctx as never);
    await expect(
      run(tools.get('agent')!, batch({ type: 'spawn', profile: 'researcher', task: 'work' }), ctx),
    ).resolves.toBeDefined();
    expect(vi.mocked(agentTools.prepareSpawnAgentParams)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(agentTools.spawnRpcAgent)).toHaveBeenCalledTimes(1);

    vi.clearAllMocks();
    await expect(
      run(
        tools.get('agent')!,
        batch(
          { type: 'inspect' },
          { type: 'spawn', profile: 'researcher', task: 'write later' },
        ),
        ctx,
      ),
    ).resolves.toBeDefined();
    expect(vi.mocked(agentTools.refreshAgentLedgerUi)).toHaveBeenCalled();
    expect(vi.mocked(agentTools.spawnRpcAgent)).toHaveBeenCalledTimes(1);
  });

  it('keeps ordinary agent input validation active during planning', async () => {
    const ctx = planContext('malformed');
    enterPlanMode(ctx as never);
    await expect(
      run(tools.get('agent')!, batch({ type: 'unknown-operation' }), ctx),
    ).rejects.toThrow(/unknown|invalid|must be one of/i);
    expect(vi.mocked(agentTools.spawnRpcAgent)).not.toHaveBeenCalled();
  });
  it.each(['executing', 'verifying', 'complete'] as const)(
    'preserves normal spawn semantics in %s',
    async (phase) => {
      const ctx = planContext(`allowed-${phase}`);
      adoptPlanModePolicy(ctx as never, { phase, branchSnapshotId: `branch-${phase}`, generation: 1 });
      await expect(
        run(tools.get('agent')!, batch({ type: 'spawn', profile: 'researcher', task: 'work' }), ctx),
      ).resolves.toBeDefined();
      expect(vi.mocked(agentTools.prepareSpawnAgentParams)).toHaveBeenCalledTimes(1);
      expect(vi.mocked(agentTools.spawnRpcAgent)).toHaveBeenCalledTimes(1);
    },
  );

  it('uses the same resolved effect for the synchronous gate and capability receipt', () => {
    const ctx = planContext('receipt');
    enterPlanMode(ctx as never);
    const lifecycle = batch({ type: 'inspect' });
    const spawn = batch({ type: 'spawn', profile: 'researcher', task: 'work' });

    expect(getToolEffect('agent', lifecycle)).toBe('coordination-write');
    expect(planModeToolGate('agent', ctx as never, lifecycle)).toBeUndefined();
    expect(evaluateToolCapability({ toolName: 'agent', toolInput: lifecycle, phase: 'researching' }).effectiveDecision).toBe('allow');

    expect(getToolEffect('agent', spawn)).toBe('external-effect');
    expect(planModeToolGate('agent', ctx as never, spawn)).toBeUndefined();
    expect(evaluateToolCapability({ toolName: 'agent', toolInput: spawn, phase: 'researching' }).effectiveDecision).toBe('allow');
  });
});

// ─── Dispatch: lifecycle operations ──────────────────────────────────────────

describe('lifecycle dispatch', () => {
  let agentTools: typeof import('../src/tools/agent-tools.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentTools = await import('../src/tools/agent-tools.js');
    vi.mocked(agentTools.killWorkerById).mockReturnValue(true);
    vi.mocked(agentTools.steerWorkerById).mockReturnValue(true);
    vi.mocked(agentTools.getWorkerTranscript).mockImplementation(
      (id: string) => (id === MOCK_RECORD.id ? MOCK_TRANSCRIPT : undefined),
    );
    tools = await loadSut();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('inspect without agentId returns ledger list', async () => {
    const { text } = await run(tools.get('agent')!, batch({ type: 'inspect' }));
    expect(text).toMatch(/mock-agent-001|agents/i);
    expect(vi.mocked(agentTools.refreshAgentLedgerUi)).toHaveBeenCalled();
  });

  it('inspect with agentId returns transcript', async () => {
    const { text } = await run(
      tools.get('agent')!,
      batch({ type: 'inspect', agentId: MOCK_RECORD.id }),
    );
    expect(text).toBe(MOCK_TRANSCRIPT);
  });

  it('inspect with unknown agentId throws', async () => {
    await expect(
      run(tools.get('agent')!, batch({ type: 'inspect', agentId: 'ghost-id' })),
    ).rejects.toThrow(/no agent found/i);
  });

  it('wait with known agentId returns snapshot', async () => {
    const { text } = await run(
      tools.get('agent')!,
      batch({ type: 'wait', agentId: MOCK_RECORD.id }),
    );
    expect(text).toContain('[WAIT snapshot');
    expect(text).toContain(MOCK_TRANSCRIPT);
  });

  it('wait without agentId throws', async () => {
    await expect(
      run(tools.get('agent')!, batch({ type: 'wait' })),
    ).rejects.toThrow(/requires agentId/i);
  });

  it('message delegates to steerWorkerById', async () => {
    const { text } = await run(
      tools.get('agent')!,
      batch({ type: 'message', agentId: MOCK_RECORD.id, message: 'hello worker' }),
    );
    expect(vi.mocked(agentTools.steerWorkerById)).toHaveBeenCalledWith(
      MOCK_RECORD.id,
      'hello worker',
    );
    expect(text).toContain('[MESSAGE]');
    expect(text).toContain('hello worker');
  });

  it('message with delivery:followUp is reflected in output', async () => {
    const { text } = await run(
      tools.get('agent')!,
      batch({
        type: 'message',
        agentId: MOCK_RECORD.id,
        message: 'queue me',
        delivery: 'followUp',
      }),
    );
    expect(text).toContain('followUp');
  });

  it('steer delegates to steerWorkerById', async () => {
    const { text } = await run(
      tools.get('agent')!,
      batch({ type: 'steer', agentId: MOCK_RECORD.id, message: 'new focus' }),
    );
    expect(vi.mocked(agentTools.steerWorkerById)).toHaveBeenCalledWith(
      MOCK_RECORD.id,
      'new focus',
    );
    expect(text).toContain('[STEER]');
  });

  it('steer without message throws', async () => {
    await expect(
      run(tools.get('agent')!, batch({ type: 'steer', agentId: MOCK_RECORD.id })),
    ).rejects.toThrow(/requires (a )?non-empty message/i);
  });

  it('abort dispatches to the canonical lifecycle executor', async () => {
    const { text } = await run(
      tools.get('agent')!,
      batch({ type: 'abort', agentId: MOCK_RECORD.id }),
    );
    expect(text).toContain('[ABORT]');
    expect(text).toContain(MOCK_RECORD.id);
  });

  it('abort with unknown agentId throws', async () => {
    await expect(
      run(tools.get('agent')!, batch({ type: 'abort', agentId: 'ghost-id' })),
    ).rejects.toThrow(/no agent found/i);
  });

  it('kill delegates to killWorkerById', async () => {
    const { text } = await run(
      tools.get('agent')!,
      batch({ type: 'kill', agentId: MOCK_RECORD.id }),
    );
    expect(vi.mocked(agentTools.killWorkerById)).toHaveBeenCalledWith(MOCK_RECORD.id);
    expect(text).toContain('[KILL]');
    expect(vi.mocked(agentTools.refreshAgentLedgerUi)).toHaveBeenCalled();
  });

  it('kill with unknown agentId throws', async () => {
    vi.mocked(agentTools.killWorkerById).mockReturnValue(false);
    await expect(
      run(tools.get('agent')!, batch({ type: 'kill', agentId: 'ghost-id' })),
    ).rejects.toThrow(/no agent found/i);
  });
});

// ─── Spawn: typed profiles ────────────────────────────────────────────────────

describe('spawn: typed profiles', () => {
  let agentTools: typeof import('../src/tools/agent-tools.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentTools = await import('../src/tools/agent-tools.js');
    vi.mocked(agentTools.spawnRpcAgent).mockReturnValue({ ...MOCK_RECORD } as never);
    tools = await loadSut();
  });

  afterEach(() => vi.clearAllMocks());

  it.each(['researcher', 'planner', 'architect'] as const)(
    'profile:%s calls prepareSpawnAgentParams + spawnRpcAgent and returns agentId',
    async (profile) => {
      const { text, details } = await run(
        tools.get('agent')!,
        batch({ type: 'spawn', profile, task: 'gather evidence' }),
      );
      expect(vi.mocked(agentTools.prepareSpawnAgentParams)).toHaveBeenCalled();
      expect(vi.mocked(agentTools.spawnRpcAgent)).toHaveBeenCalled();
      expect(text).toContain('[SPAWNED]');
      expect(text).toContain(MOCK_RECORD.id);
      expect((details as { agentId?: string }).agentId).toBe(MOCK_RECORD.id);
      expect((details as { profile?: string }).profile).toBe(profile);
    },
  );

  it('profile:researcher passes octocode resourceMode and typed tool list', async () => {
    await run(tools.get('agent')!, batch({ type: 'spawn', profile: 'researcher', task: 'research X' }));
    const spawnCall = vi.mocked(agentTools.prepareSpawnAgentParams).mock.calls[0]![0] as {
      resourceMode?: string; tools?: string[];
    };
    expect(spawnCall.resourceMode).toBe('octocode');
    expect(spawnCall.tools).toContain('web');
  });

  it('profile:custom uses lean resourceMode with explicit tools', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'custom', task: 'custom job', tools: ['bash', 'write'] }),
    );
    const spawnCall = vi.mocked(agentTools.prepareSpawnAgentParams).mock.calls[0]![0] as {
      resourceMode?: string; tools?: string[];
    };
    expect(spawnCall.resourceMode).toBe('lean');
    expect(spawnCall.tools).toEqual(['bash', 'write']);
  });

  it('surfaces policyWarnings from the spawn record in the output', async () => {
    vi.mocked(agentTools.spawnRpcAgent).mockReturnValue({
      ...MOCK_RECORD,
      policyWarnings: ['Missing Goal: label in task packet.'],
    } as never);
    const { text } = await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'custom', task: 'missing labels' }),
    );
    expect(text).toContain('[POLICY]');
    expect(text).toContain('Missing Goal:');
  });
});

// ─── Spawn: browser profile (routing + CDP delegation) ───────────────────────

describe('spawn: browser profile routing', () => {
  let agentTools: typeof import('../src/tools/agent-tools.js');
  let browserAgentTool: typeof import('../src/tools/browser-agent-tool.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentTools = await import('../src/tools/agent-tools.js');
    browserAgentTool = await import('../src/tools/browser-agent-tool.js');
    vi.mocked(agentTools.spawnRpcAgent).mockReturnValue({ ...MOCK_RECORD } as never);
    tools = await loadSut();
  });

  afterEach(() => vi.clearAllMocks());

  it('calls routeTask with the task string', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'browser', task: 'analyze security headers', runNow: false }),
    );
    expect(vi.mocked(browserAgentTool.routeTask)).toHaveBeenCalledWith('analyze security headers');
  });

  it('passes CDP domains from routeTask to buildSpawnConfig', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'browser', task: 'analyze security headers', runNow: false }),
    );
    const buildCall = vi.mocked(browserAgentTool.buildSpawnConfig).mock.calls[0]![0] as {
      cdpDomains: string[];
    };
    // Mock returns ['Network', 'Runtime', 'DOMDebugger'] for 'security' keyword
    expect(buildCall.cdpDomains).toContain('Network');
    expect(buildCall.cdpDomains).toContain('DOMDebugger');
  });

  it('passes url and port to buildSpawnConfig when provided', async () => {
    await run(
      tools.get('agent')!,
      batch({
        type: 'spawn',
        profile: 'browser',
        task: 'check performance',
        url: 'https://example.com',
        port: 9333,
        runNow: false,
      }),
    );
    const buildCall = vi.mocked(browserAgentTool.buildSpawnConfig).mock.calls[0]![0] as {
      url?: string; port: number;
    };
    expect(buildCall.url).toBe('https://example.com');
    expect(buildCall.port).toBe(9333);
  });

  it('uses systemPrompt from buildSpawnConfig result for the worker', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'browser', task: 'debug network errors', runNow: false }),
    );
    const spawnCall = vi.mocked(agentTools.prepareSpawnAgentParams).mock.calls[0]![0] as {
      systemPrompt?: string;
    };
    expect(spawnCall.systemPrompt).toContain('Browser specialist for:');
  });

  it('sets tool allowlist to [chromeDebug] from buildSpawnConfig result', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'browser', task: 'inspect DOM', runNow: false }),
    );
    const spawnCall = vi.mocked(agentTools.prepareSpawnAgentParams).mock.calls[0]![0] as {
      tools?: string[];
    };
    expect(spawnCall.tools).toEqual(['chromeDebug']);
  });

  it('defaults to port 9222 when no port provided', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'browser', task: 'check cookies', runNow: false }),
    );
    const buildCall = vi.mocked(browserAgentTool.buildSpawnConfig).mock.calls[0]![0] as {
      port: number;
    };
    expect(buildCall.port).toBe(9222);
  });

  it('returns agentId and profile:browser in details', async () => {
    const { details } = await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'browser', task: 'audit page', runNow: false }),
    );
    expect((details as { agentId?: string }).agentId).toBe(MOCK_RECORD.id);
    expect((details as { profile?: string }).profile).toBe('browser');
  });
});

// ─── Multi-query batch semantics ─────────────────────────────────────────────

describe('multi-query batch', () => {
  let agentTools: typeof import('../src/tools/agent-tools.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentTools = await import('../src/tools/agent-tools.js');
    tools = await loadSut();
  });

  afterEach(() => vi.clearAllMocks());

  it('executes multiple spawn queries in order and returns count in summary', async () => {
    vi.mocked(agentTools.spawnRpcAgent)
      .mockReturnValueOnce({ ...MOCK_RECORD, id: 'agent-a', name: 'Worker A' } as never)
      .mockReturnValueOnce({ ...MOCK_RECORD, id: 'agent-b', name: 'Worker B' } as never);

    const { text } = await run(
      tools.get('agent')!,
      batch(
        { type: 'spawn', profile: 'custom', task: 'task A' },
        { type: 'spawn', profile: 'custom', task: 'task B' },
      ),
    );
    expect(vi.mocked(agentTools.spawnRpcAgent)).toHaveBeenCalledTimes(2);
    expect(text).toContain('2 quer');
  });

  it('stops on first failure and reports the failing index', async () => {
    vi.mocked(agentTools.killWorkerById)
      .mockReturnValueOnce(true)
      .mockReturnValueOnce(false); // second kill fails

    await expect(
      run(
        tools.get('agent')!,
        batch(
          { type: 'kill', agentId: MOCK_RECORD.id },
          { type: 'kill', agentId: 'nonexistent-id' },
          { type: 'inspect' }, // should not execute
        ),
      ),
    ).rejects.toThrow(/queries\[1\].*no agent found/i);

    // inspect (index 2) must not have been called
    expect(vi.mocked(agentTools.formatAgentLedgerDetails)).not.toHaveBeenCalled();
    expect(vi.mocked(agentTools.formatAgentLedger)).not.toHaveBeenCalled();
  });
});

// ─── Renderer smoke tests ─────────────────────────────────────────────────────

describe('renderCall / renderResult', () => {
  it('renderCall with no args produces a non-empty string', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    const renderer = (tool.renderCall as (r: unknown) => { render: (w: number) => string[] })?.({});
    const lines = renderer?.render(80) ?? [];
    expect(lines.length).toBeGreaterThan(0);
    expect(typeof lines[0]).toBe('string');
  });

  it('renderCall shows operation type and profile when present', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    const renderer = (
      tool.renderCall as (r: unknown) => { render: (w: number) => string[] }
    )?.({ queries: [{ type: 'spawn', profile: 'researcher', reasoning: 'test' }] });
    const line = renderer?.render(120)?.[0] ?? '';
    expect(line).toMatch(/spawn/);
    expect(line).toMatch(/researcher/);
  });

  it('renderResult shows success glyph on non-error result', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    const mockResult: ToolCallResult = {
      content: [{ type: 'text', text: '[SPAWNED] profile:custom · agentId:abc' }],
    };
    const renderer = (
      tool.renderResult as (
        r: unknown,
        opts: unknown,
        theme: undefined,
      ) => { render: (w: number) => string[] }
    )?.(mockResult, {}, undefined);
    const line = renderer?.render(120)?.[0] ?? '';
    expect(line).toContain('agent');
  });
});
