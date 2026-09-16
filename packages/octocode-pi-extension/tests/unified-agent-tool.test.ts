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
import fs from 'node:fs';
import { compileMcpSchemaValidator } from '../src/tools/mcp/schema-validator.js';
import type { ToolDefinition, ToolCallResult } from '../src/types.js';
import {
  AGENT_OPERATIONS,
  AGENT_PROFILES,
  rejectCrossBatchReference,
  type AgentOperation,
  type AgentProfile,
} from '../src/tools/agents/plan-integration.js';
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

/** Build a minimal one-query batch for the tool's execute(). */
function batch(...queries: Record<string, unknown>[]): Record<string, unknown> {
  return {
    queries: queries.map((q) => {
      const query: Record<string, unknown> = { reasoning: 'test operation', ...q };
      if (query['type'] !== 'spawn') return query;
      const complete = { ...query } as Record<string, unknown>;
      if (!Object.hasOwn(complete, 'profile')) complete['profile'] = 'researcher';
      const defaults: Record<string, string> = {
        goal: String(complete['task'] ?? 'Complete the delegated unit'),
        context: 'Observed test context',
        scope: 'Only the delegated test unit; exclude unrelated work',
        ownership: 'Read-only test ownership',
        acceptance: 'The delegated test unit reports its result',
        returnShape: 'Status, evidence, verification, confidence, and next action',
      };
      for (const [field, value] of Object.entries(defaults)) {
        if (!Object.hasOwn(complete, field)) complete[field] = value;
      }
      if (complete['profile'] === 'custom') {
        if (!Object.hasOwn(complete, 'tools')) complete['tools'] = [];
        if (!Object.hasOwn(complete, 'systemPrompt')) complete['systemPrompt'] = 'Perform the bounded custom test role.';
      }
      return complete;
    }),
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

// All module-level vi.fn() references are hoisted so vi.mock factory bodies can
// reference them without a temporal dead zone error (vi.mock is hoisted above
// regular const declarations, so factories run before const initialization).
const {
  mockRefreshAgentLedgerUi,
  mockKillWorkerById,
  mockSteerWorkerById,
} = vi.hoisted(() => ({
  mockRefreshAgentLedgerUi: vi.fn(),
  // MOCK_RECORD.id === 'mock-agent-001' \u2014 inline the string since vi.hoisted runs
  // before the module body is evaluated and MOCK_RECORD is not yet in scope.
  mockKillWorkerById: vi.fn((id: string) => id === 'mock-agent-001'),
  mockSteerWorkerById: vi.fn((_id: string, _msg: string) => true),
}));

vi.mock('../src/tools/agents/rendering.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../src/tools/agents/rendering.js')>();
  return {
    ...original,
    refreshAgentLedgerUi: mockRefreshAgentLedgerUi,
  };
});

vi.mock('../src/tools/agents/process.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../src/tools/agents/process.js')>();
  return {
    ...original,
    prepareSpawnAgentParams: vi.fn(async (params: unknown) => params),
    spawnRpcAgent: vi.fn(() => ({ ...MOCK_RECORD })),
  };
});

vi.mock('../src/tools/agents/registry.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../src/tools/agents/registry.js')>();
  return {
    ...original,
    isSubagentProcess: vi.fn(() => false),
  };
});

vi.mock('../src/tools/agents/lifecycle.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../src/tools/agents/lifecycle.js')>();
  const getWorkerTranscript = vi.fn(
    (id: string) => (id === MOCK_RECORD.id ? MOCK_TRANSCRIPT : undefined),
  );
  const executeAgentLifecycle = vi.fn(async (params: Record<string, unknown>) => {
    const action = String(params['type'] ?? '');
    const agentId = String(params['agentId'] ?? '');
    if (action === 'inspect' && !agentId) {
      mockRefreshAgentLedgerUi();
      return { content: [{ type: 'text', text: 'mock-agent-001 \u00b7 idle' }] };
    }
    if (agentId !== MOCK_RECORD.id) {
      throw new Error(`No agent found with id: ${agentId}.`);
    }
    if (action === 'inspect') return { content: [{ type: 'text', text: MOCK_TRANSCRIPT }] };
    if (action === 'wait') return { content: [{ type: 'text', text: `[WAIT snapshot \u00b7 agentId:${agentId}]\n${MOCK_TRANSCRIPT}` }] };
    if (action === 'message') {
      mockSteerWorkerById(agentId, String(params['message'] ?? ''));
      return { content: [{ type: 'text', text: `[MESSAGE] delivery:${params['delivery'] ?? 'send'} \u2192 ${agentId}: ${String(params['message'] ?? '')}` }] };
    }
    if (action === 'steer') {
      mockSteerWorkerById(agentId, String(params['message'] ?? ''));
      return { content: [{ type: 'text', text: `[STEER] \u2192 ${agentId}` }] };
    }
    if (action === 'abort') return { content: [{ type: 'text', text: `[ABORT] ${agentId}` }] };
    if (action === 'kill') {
      if (!mockKillWorkerById(agentId)) throw new Error(`No agent found with id: ${agentId}.`);
      mockRefreshAgentLedgerUi();
      return { content: [{ type: 'text', text: `[KILL] ${agentId}` }] };
    }
    throw new Error(`unsupported test action: ${action}`);
  });
  return {
    ...original,
    steerWorkerById: mockSteerWorkerById,
    getWorkerTranscript,
    executeAgentLifecycle,
  };
});

vi.mock('../src/tools/agents/kill.js', async (importOriginal) => {
  const original = await importOriginal<typeof import('../src/tools/agents/kill.js')>();
  return {
    ...original,
    killWorkerById: mockKillWorkerById,
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
      implementer: {
        name: 'implementer',
        label: 'Implementer',
        tools: ['MCPTool', 'file', 'skill', 'bash'],
        resourceMode: 'octocode',
        thinking: 'medium',
        systemPromptPath: '/mock/implementer/SYSTEM_PROMPT.md',
        extraSkillPaths: [],
      },
      reviewer: {
        name: 'reviewer',
        label: 'Reviewer',
        tools: ['MCPTool', 'skill', 'awareness'],
        resourceMode: 'octocode',
        thinking: 'medium',
        systemPromptPath: '/mock/reviewer/SYSTEM_PROMPT.md',
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
    SUBAGENT_NAMES: ['researcher', 'planner', 'architect', 'implementer', 'reviewer', 'browser-agent'],
    loadSystemPrompt: vi.fn(() => '# Mock System Prompt'),
    resolveSubagentSkills: vi.fn(() => []),
    getExternalSkillDirs: vi.fn(() => []),
    OCTOCODE_SKILL_NAMES: [],
  };
});

// Import SUT AFTER mocks are registered.
async function loadSut() {
  const { registerUnifiedAgentTool } = await import('../src/tools/agents/tool.js');
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
  registerUnifiedAgentTool(pi, new Set<string>(), registerFn as never);
  return tools;
}

// ─── Schema tests ─────────────────────────────────────────────────────────────

describe('schema', () => {
  it('accepts the documented browser packet and rejects a packet without ownership', async () => {
    const tools = await loadSut();
    const documentation = fs.readFileSync(new URL('../subagents/browser-agent/BROWSER_AGENT.md', import.meta.url), 'utf8');
    const example = JSON.parse(documentation.match(/```json\n([\s\S]*?)\n```/)![1]!);
    const validator = compileMcpSchemaValidator(tools.get('agent')!.parameters);
    expect(validator.validate(example).valid).toBe(true);
    delete example.queries[0].ownership;
    expect(validator.validate(example).valid).toBe(false);
  });
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

  type SchemaProperty = {
    const?: string;
    enum?: string[];
    minLength?: number;
    maxLength?: number;
    description?: string;
    $ref?: string;
  };

  function schemaBranches(schema: ToolDefinition['parameters']): Array<{ properties?: Record<string, SchemaProperty>; required?: string[] }> {
    const root = schema as { properties?: { queries?: { minItems?: number; items?: { anyOf?: unknown[]; oneOf?: unknown[] } } } };
    const items = root.properties?.queries?.items;
    return (items?.anyOf ?? items?.oneOf ?? []) as Array<{ properties?: Record<string, SchemaProperty>; required?: string[] }>;
  }

  it('queries array has minItems:1 and every discriminated branch requires bounded reasoning', async () => {
    const tools = await loadSut();
    const schema = tools.get('agent')!.parameters as { properties?: { queries?: { minItems?: number } } };
    expect(schema.properties?.queries?.minItems).toBe(1);
    const branches = schemaBranches(tools.get('agent')!.parameters);
    expect(branches.length).toBeGreaterThan(AGENT_OPERATIONS.length);
    for (const branch of branches) {
      expect(branch.required).toContain('reasoning');
      expect(branch.properties?.['reasoning']?.minLength).toBe(1);
      expect(branch.properties?.['reasoning']?.maxLength).toBe(400);
    }
  });

  it('schema discriminates every operation and spawn profile', async () => {
    const tools = await loadSut();
    const branches = schemaBranches(tools.get('agent')!.parameters);
    const operations = new Set(branches.map((branch) => branch.properties?.['type']?.enum?.[0]));
    for (const op of AGENT_OPERATIONS) expect(operations).toContain(op);
    const profiles = new Set(branches.flatMap((branch) => [
      ...(branch.properties?.['profile']?.enum ?? []),
      ...(branch.properties?.['profile']?.const ? [branch.properties['profile'].const!] : []),
    ]));
    for (const profile of AGENT_PROFILES) expect(profiles).toContain(profile);
    const custom = branches.find((branch) => branch.properties?.['profile']?.enum?.includes('custom'));
    expect(custom?.required).toEqual(expect.arrayContaining(['goal', 'context', 'scope', 'ownership', 'acceptance', 'returnShape', 'tools', 'systemPrompt']));
  });

  it('publishes profile-specific capability contracts that agree with custom preflight', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    const schema = tool.parameters as {
      definitions?: Record<string, {
        additionalProperties?: boolean;
        properties?: Record<string, unknown>;
      }>;
    };
    const branches = schemaBranches(tool.parameters);
    const custom = branches.find((branch) => branch.properties?.['profile']?.enum?.includes('custom'));
    const typed = branches.find((branch) => branch.properties?.['profile']?.enum?.includes('researcher'));
    const configure = branches.find((branch) => branch.properties?.['type']?.enum?.includes('configure'));

    expect(custom?.properties?.['capabilities']?.$ref).toBe('#/definitions/customWorkerCapabilities');
    expect(typed?.properties?.['capabilities']?.$ref).toBe('#/definitions/workerCapabilities');
    expect(configure?.properties?.['capabilities']?.$ref).toBe('#/definitions/workerCapabilities');
    expect(Object.keys(schema.definitions?.['customWorkerCapabilities']?.properties ?? {}).sort()).toEqual(['mcpTools', 'skills']);
    expect(schema.definitions?.['customWorkerCapabilities']?.additionalProperties).toBe(false);
    expect(Object.keys(schema.definitions?.['workerCapabilities']?.properties ?? {}).sort()).toEqual(['mcpTools', 'nativeTools', 'skills']);

    const validator = compileMcpSchemaValidator(tool.parameters);
    expect(validator.validate(batch({
      type: 'spawn', profile: 'custom', task: 'custom capabilities',
      tools: ['MCPTool', 'skill'], capabilities: { skills: [], mcpTools: [] },
    })).valid).toBe(true);
    expect(validator.validate(batch({
      type: 'spawn', profile: 'custom', task: 'duplicate native selector',
      tools: ['MCPTool'], capabilities: { nativeTools: [] },
    })).valid).toBe(false);
    expect(validator.validate(batch({
      type: 'spawn', profile: 'researcher', task: 'typed capabilities',
      capabilities: { nativeTools: [], skills: [], mcpTools: [] },
    })).valid).toBe(true);
    expect(validator.validate(batch({
      type: 'configure', agentId: 'existing-agent', snapshotRevision: 'snapshot-one',
      capabilities: { nativeTools: [], skills: [], mcpTools: [] },
    })).valid).toBe(true);
  });

  it('accepts every profile and operation contract while rejecting branch-only field leakage', async () => {
    const tools = await loadSut();
    const validator = compileMcpSchemaValidator(tools.get('agent')!.parameters);
    const validPayloads = [
      ...AGENT_PROFILES.map((profile) => batch({
        type: 'spawn',
        profile,
        task: `${profile} task`,
        ...(profile === 'custom' ? { tools: [], systemPrompt: 'Bounded custom role.' } : {}),
        ...(profile === 'browser' ? { runNow: false } : {}),
      })),
      batch({ type: 'inspect' }),
      batch({ type: 'inspect', agentId: 'existing-agent', full: true }),
      batch({
        type: 'configure', agentId: 'existing-agent', snapshotRevision: 'snapshot-one', grantRevision: 1,
        capabilities: { nativeTools: [], skills: [], mcpTools: [] },
      }),
      batch({ type: 'wait', agentId: 'existing-agent', timeoutMs: 1, remove: false, full: true }),
      batch({ type: 'message', agentId: 'existing-agent', message: 'continue', delivery: 'send' }),
      batch({ type: 'steer', agentId: 'existing-agent', message: 'focus here' }),
      batch({ type: 'abort', agentId: 'existing-agent', full: true }),
      batch({ type: 'kill', agentId: 'existing-agent', remove: true, full: true }),
    ];
    for (const payload of validPayloads) expect(validator.validate(payload).valid).toBe(true);

    const invalidPayloads = [
      batch({ type: 'spawn', profile: 'researcher', task: 'typed', tools: [] }),
      batch({ type: 'spawn', profile: 'browser', task: 'browser', systemPrompt: 'not allowed' }),
      batch({ type: 'spawn', profile: 'custom', task: 'custom', tools: [], systemPrompt: 'role', url: 'https://example.com' }),
      batch({ type: 'inspect', message: 'not allowed' }),
      batch({ type: 'configure', agentId: 'existing-agent', snapshotRevision: 'snapshot-one' }),
      batch({ type: 'wait' }),
      batch({ type: 'message', agentId: 'existing-agent' }),
      batch({ type: 'steer', agentId: 'existing-agent', message: 'focus', delivery: 'send' }),
      batch({ type: 'abort', agentId: 'existing-agent', remove: true }),
      batch({ type: 'kill', agentId: 'existing-agent', durationMs: 1 }),
    ];
    for (const payload of invalidPayloads) expect(validator.validate(payload).valid).toBe(false);
  });

  it('marks planStep as optional and exclusive to an existing executing-plan assignment', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    const branches = schemaBranches(tool.parameters);
    const spawns = branches.filter((branch) => branch.properties?.['type']?.enum?.includes('spawn'));
    expect(spawns.length).toBeGreaterThan(0);
    expect(spawns.every((branch) => !branch.required?.includes('planStep'))).toBe(true);
    expect(spawns.every((branch) => /plan task ID.*omit for standalone/is.test(String(branch.properties?.['planStep']?.description)))).toBe(true);
  });

  it('guides the parent through bounded delegation, verification, integration, and continuation', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    const guidance = [tool.description, tool.promptSnippet, ...(tool.promptGuidelines ?? [])].join('\n');
    expect(guidance).toMatch(/two or more bounded lanes.*independent.*disjoint ownership/is);
    expect(guidance).toMatch(/continue non-overlapping parent work.*type:wait.*verify.*reconcile.*kill.*continue the user request/is);
    expect(guidance).toMatch(/rejects incomplete packets before creating a worker/is);
    expect(guidance).toMatch(/never trust or persist a raw handback/is);
  });

  it('AGENT_OPERATIONS covers spawn, parent configuration, and lifecycle operations', () => {
    const expected: AgentOperation[] = [
      'spawn', 'inspect', 'configure', 'wait', 'message', 'steer', 'abort', 'kill',
    ];
    expect([...AGENT_OPERATIONS].sort()).toEqual([...expected].sort());
  });

  it('AGENT_PROFILES covers all seven expected profiles', () => {
    const expected: AgentProfile[] = [
      'researcher', 'planner', 'architect', 'implementer', 'reviewer', 'browser', 'custom',
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

  it('rejects an incomplete spawn packet in preflight before execution', async () => {
    const tools = await loadSut();
    const tool = tools.get('agent')!;
    await expect(
      run(tool, batch({ type: 'spawn', profile: 'custom', goal: ' ' })),
    ).rejects.toThrow(/spawn requires non-empty goal/i);
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

  it('passes planStep through as metadata without validation', async () => {
    const agentProcess = await import('../src/tools/agents/process.js');
    const tools = await loadSut();
    const result = await run(
      tools.get('agent')!,
      batch({ type: 'spawn', task: 'Independent architecture review', planStep: 'architecture-review-integration' }),
    );
    expect(vi.mocked(agentProcess.spawnRpcAgent).mock.calls[0]![0].planStep).toBe('architecture-review-integration');
    expect(result.text).toMatch(/SPAWNED/i);
  });

  it('spawns without planStep and does not read plan state', async () => {
    const read = vi.spyOn(planReadModel, 'getCurrentPlanReadModel').mockImplementation(() => { throw new Error('should not read plan'); });
    const tools = await loadSut();
    await expect(run(tools.get('agent')!, batch({ type: 'spawn', task: 'Independent research' }))).resolves.toBeDefined();
    expect(read).not.toHaveBeenCalled();
  });

  it('does not create a process after cancellation during preparation', async () => {
    const controller = new AbortController();
    const agentProcess = await import('../src/tools/agents/process.js');
    vi.mocked(agentProcess.prepareSpawnAgentParams).mockImplementationOnce(async (params) => {
      controller.abort();
      return params;
    });
    const tools = await loadSut();
    await expect(tools.get('agent')!.execute('cancel-spawn', batch({ type: 'spawn', task: 'Build' }), controller.signal)).rejects.toThrow(/abort/i);
    expect(agentProcess.spawnRpcAgent).not.toHaveBeenCalled();
  });
});

describe('plan Start enforcement', () => {
  let agentProcess: typeof import('../src/tools/agents/process.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentProcess = await import('../src/tools/agents/process.js');
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
    expect(vi.mocked(agentProcess.prepareSpawnAgentParams)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(agentProcess.spawnRpcAgent)).toHaveBeenCalledTimes(1);

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
    expect(mockRefreshAgentLedgerUi).toHaveBeenCalled();
    expect(vi.mocked(agentProcess.spawnRpcAgent)).toHaveBeenCalledTimes(1);
  });

  it('keeps ordinary agent input validation active during planning', async () => {
    const ctx = planContext('malformed');
    enterPlanMode(ctx as never);
    await expect(
      run(tools.get('agent')!, batch({ type: 'unknown-operation' }), ctx),
    ).rejects.toThrow(/unknown|invalid|must be one of/i);
    expect(vi.mocked(agentProcess.spawnRpcAgent)).not.toHaveBeenCalled();
  });
  it.each(['executing', 'verifying', 'complete'] as const)(
    'preserves normal spawn semantics in %s',
    async (phase) => {
      const ctx = planContext(`allowed-${phase}`);
      adoptPlanModePolicy(ctx as never, { phase, branchSnapshotId: `branch-${phase}`, generation: 1 });
      await expect(
        run(tools.get('agent')!, batch({ type: 'spawn', profile: 'researcher', task: 'work' }), ctx),
      ).resolves.toBeDefined();
      expect(vi.mocked(agentProcess.prepareSpawnAgentParams)).toHaveBeenCalledTimes(1);
      expect(vi.mocked(agentProcess.spawnRpcAgent)).toHaveBeenCalledTimes(1);
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
  let agentLifecycle: typeof import('../src/tools/agents/lifecycle.js');
  let agentKill: typeof import('../src/tools/agents/kill.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentLifecycle = await import('../src/tools/agents/lifecycle.js');
    agentKill = await import('../src/tools/agents/kill.js');
    vi.mocked(agentKill.killWorkerById).mockReturnValue(true);
    vi.mocked(agentLifecycle.steerWorkerById).mockReturnValue(true);
    vi.mocked(agentLifecycle.getWorkerTranscript).mockImplementation(
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
    expect(mockRefreshAgentLedgerUi).toHaveBeenCalled();
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
    expect(vi.mocked(agentLifecycle.steerWorkerById)).toHaveBeenCalledWith(
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
    expect(vi.mocked(agentLifecycle.steerWorkerById)).toHaveBeenCalledWith(
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
    expect(vi.mocked(agentKill.killWorkerById)).toHaveBeenCalledWith(MOCK_RECORD.id);
    expect(text).toContain('[KILL]');
    expect(mockRefreshAgentLedgerUi).toHaveBeenCalled();
  });

  it('kill with unknown agentId throws', async () => {
    vi.mocked(agentKill.killWorkerById).mockReturnValue(false);
    await expect(
      run(tools.get('agent')!, batch({ type: 'kill', agentId: 'ghost-id' })),
    ).rejects.toThrow(/no agent found/i);
  });
});

// ─── Spawn: typed profiles ────────────────────────────────────────────────────

describe('spawn: typed profiles', () => {
  let agentProcess: typeof import('../src/tools/agents/process.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentProcess = await import('../src/tools/agents/process.js');
    vi.mocked(agentProcess.spawnRpcAgent).mockReturnValue({ ...MOCK_RECORD } as never);
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
      expect(vi.mocked(agentProcess.prepareSpawnAgentParams)).toHaveBeenCalled();
      expect(vi.mocked(agentProcess.spawnRpcAgent)).toHaveBeenCalled();
      expect(text).toContain('[SPAWNED]');
      expect(text).toContain(MOCK_RECORD.id);
      expect((details as { agentId?: string }).agentId).toBe(MOCK_RECORD.id);
      expect((details as { profile?: string }).profile).toBe(profile);
    },
  );

  it('profile:researcher passes octocode resourceMode and typed tool list', async () => {
    await run(tools.get('agent')!, batch({ type: 'spawn', profile: 'researcher', task: 'research X' }));
    const spawnCall = vi.mocked(agentProcess.prepareSpawnAgentParams).mock.calls[0]![0] as {
      resourceMode?: string; tools?: string[];
    };
    expect(spawnCall.resourceMode).toBe('octocode');
    expect(spawnCall.tools).toContain('web');
  });

  it('profile:custom uses the explicit least-capability tool list and shared worker contract', async () => {
    await run(tools.get('agent')!, batch({
      type: 'spawn', profile: 'custom', task: 'custom job',
      capabilities: { skills: [], mcpTools: [] },
    }));
    const spawnCall = vi.mocked(agentProcess.prepareSpawnAgentParams).mock.calls[0]![0] as {
      resourceMode?: string;
      tools?: string[];
      skills?: string[];
      systemPrompt?: string;
      capabilities?: { skills?: string[]; mcpTools?: Array<{ server: string; tool: string }> };
    };
    expect(spawnCall.resourceMode).toBe('octocode');
    expect(spawnCall.tools).toEqual([]);
    expect(spawnCall.capabilities).toEqual({ skills: [], mcpTools: [] });
    expect(spawnCall.skills).toBeDefined();
    expect(spawnCall.systemPrompt).toMatch(/The parent owns scope, synthesis, dependent decisions, and user contact/i);
    expect(spawnCall.systemPrompt).toMatch(/Perform the bounded custom test role/);
  });

  it('profile:custom preserves explicit lean mode and explicit tool scoping', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'custom', task: 'custom job', tools: [], resourceMode: 'lean' }),
    );
    const spawnCall = vi.mocked(agentProcess.prepareSpawnAgentParams).mock.calls[0]![0] as {
      resourceMode?: string; tools?: string[];
    };
    expect(spawnCall.resourceMode).toBe('lean');
    expect(spawnCall.tools).toEqual([]);
  });

  it('profile:custom rejects a second native-tool selector before spawning', async () => {
    await expect(run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'custom', task: 'custom job', tools: ['MCPTool'], capabilities: { nativeTools: [] } }),
    )).rejects.toThrow(/custom profile uses tools\[\] as its native-tool selector/i);
    expect(vi.mocked(agentProcess.prepareSpawnAgentParams)).not.toHaveBeenCalled();
    expect(vi.mocked(agentProcess.spawnRpcAgent)).not.toHaveBeenCalled();
  });

  it('surfaces policyWarnings from the spawn record in the output', async () => {
    vi.mocked(agentProcess.spawnRpcAgent).mockReturnValue({
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
  let agentProcess: typeof import('../src/tools/agents/process.js');
  let browserAgentTool: typeof import('../src/tools/browser-agent-tool.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentProcess = await import('../src/tools/agents/process.js');
    browserAgentTool = await import('../src/tools/browser-agent-tool.js');
    vi.mocked(agentProcess.spawnRpcAgent).mockReturnValue({ ...MOCK_RECORD } as never);
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
    const spawnCall = vi.mocked(agentProcess.prepareSpawnAgentParams).mock.calls[0]![0] as {
      systemPrompt?: string;
    };
    expect(spawnCall.systemPrompt).toContain('Browser specialist for:');
  });

  it('keeps browser control plus Octocode research, skills, and Awareness access', async () => {
    await run(
      tools.get('agent')!,
      batch({ type: 'spawn', profile: 'browser', task: 'inspect DOM', runNow: false }),
    );
    const spawnCall = vi.mocked(agentProcess.prepareSpawnAgentParams).mock.calls[0]![0] as {
      tools?: string[];
    };
    expect(spawnCall.tools).toEqual(['chromeDebug', 'MCPTool', 'skill', 'awareness', 'bash']);
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
  let agentProcess: typeof import('../src/tools/agents/process.js');
  let agentKill: typeof import('../src/tools/agents/kill.js');
  let tools: Map<string, ToolDefinition>;

  beforeEach(async () => {
    agentProcess = await import('../src/tools/agents/process.js');
    agentKill = await import('../src/tools/agents/kill.js');
    tools = await loadSut();
  });

  afterEach(() => vi.clearAllMocks());

  it('executes multiple spawn queries in order and returns count in summary', async () => {
    vi.mocked(agentProcess.spawnRpcAgent)
      .mockReturnValueOnce({ ...MOCK_RECORD, id: 'agent-a', name: 'Worker A' } as never)
      .mockReturnValueOnce({ ...MOCK_RECORD, id: 'agent-b', name: 'Worker B' } as never);

    const { text } = await run(
      tools.get('agent')!,
      batch(
        { type: 'spawn', profile: 'custom', task: 'task A' },
        { type: 'spawn', profile: 'custom', task: 'task B' },
      ),
    );
    expect(vi.mocked(agentProcess.spawnRpcAgent)).toHaveBeenCalledTimes(2);
    expect(text).toContain('2 quer');
  });

  it('stops on first failure and reports the failing index', async () => {
    vi.mocked(agentKill.killWorkerById)
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

    const lifecycle = await import('../src/tools/agents/lifecycle.js');
    expect(vi.mocked(lifecycle.executeAgentLifecycle).mock.calls.map(([params]) => params['type'])).toEqual(['kill', 'kill']);
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
