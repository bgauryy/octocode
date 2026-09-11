import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test, vi } from 'vitest';
// Presentation events and branding are separate from command/approval behavior.
vi.mock('../src/tools/execution-runtime.js', () => ({
  emitExecution: vi.fn(),
}));
vi.mock('../src/branding/renderers.js', () => ({
  withOctocodeRender: (tool: unknown) => tool,
}));
import {
  listAwarenessCommandDescriptors,
  type AwarenessCommandResult,
} from '@octocodeai/octocode-awareness';
import type { AwarenessCommandRunner } from '../src/tools/awareness-command-runner.js';
import { registerAwarenessTool } from '../src/tools/awareness-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { ToolResultError } from '../src/tools/tool-result-error.js';
import { compileMcpSchemaValidator } from '../src/tools/mcp/schema-validator.js';
import type {
  PiContext,
  PiInstance,
  ToolCallResult,
  ToolDefinition,
} from '../src/types.js';

let root: string;
let priorHome: string | undefined;
let priorMode: string | undefined;

beforeEach(() => {
  root = mkdtempSync(path.join(tmpdir(), 'pi-awareness-tool-'));
  priorHome = process.env.OCTOCODE_HOME;
  priorMode = process.env.OCTOCODE_STORAGE_MODE;
  process.env.OCTOCODE_HOME = path.join(root, 'home');
  process.env.OCTOCODE_STORAGE_MODE = 'persistent';
});

afterEach(() => {
  if (priorHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = priorHome;
  if (priorMode === undefined) delete process.env.OCTOCODE_STORAGE_MODE;
  else process.env.OCTOCODE_STORAGE_MODE = priorMode;
  rmSync(root, { recursive: true, force: true });
});

function makeTool(runner?: AwarenessCommandRunner): ToolDefinition {
  let definition: ToolDefinition | undefined;
  const pi = {
    registerTool(value: ToolDefinition) {
      definition = value;
    },
  } as PiInstance;
  registerAwarenessTool(
    pi,
    new Set<string>(),
    (host, names, value) => registerUniqueTool(host, names, value),
    runner
  );
  assert.ok(definition);
  return definition;
}

async function runQueries(
  definition: ToolDefinition,
  queries: Record<string, unknown>[],
  ctx: PiContext = { cwd: root } as PiContext
): Promise<ToolCallResult> {
  try {
    return await definition.execute(
      'call',
      {
        queries: queries.map(query => ({
          reasoning: 'test awareness facade',
          ...query,
        })),
      },
      undefined,
      undefined,
      ctx
    );
  } catch (error) {
    if (error instanceof ToolResultError) return error.result;
    const message = error instanceof Error ? error.message : String(error);
    return { content: [{ type: 'text', text: message }], isError: true };
  }
}

async function run(
  definition: ToolDefinition,
  query: Record<string, unknown>,
  ctx?: PiContext
): Promise<ToolCallResult> {
  return runQueries(definition, [query], ctx ?? ({ cwd: root } as PiContext));
}

function details(value: ToolCallResult): Record<string, unknown> {
  return (value.details ?? {}) as Record<string, unknown>;
}

test('guidance teaches a direct canonical operation envelope', async () => {
  const tool = makeTool();
  const guidance = [tool.promptSnippet, ...(tool.promptGuidelines ?? [])].join(
    '\n'
  );
  assert.match(guidance, /operation.*context\.orient/i);
  assert.doesNotMatch(
    guidance,
    /recall before any edit|Session start:|Long operations/
  );
  const example = guidance.match(/Example: (\{.*\})\./)?.[1];
  assert.ok(example, 'native guidance provides a real queries envelope');
  const input = JSON.parse(example);
  assert.equal(
    compileMcpSchemaValidator(tool.parameters).validate(input).valid,
    true
  );
  const value = await tool.execute(
    'prompt-example',
    input,
    undefined,
    undefined,
    { cwd: root } as PiContext
  );
  assert.equal(value.isError, false);
  assert.equal(
    details(value).operation,
    'context.orient'
  );
});

test('keeps legacy discovery bounded to operator and recovery commands', async () => {
  const tool = makeTool();
  const value = await run(tool, { operation: 'legacy', action: 'list' });
  const routine = new Set(listAwarenessCommandDescriptors({ routine: true }).map(entry => entry.command));
  const legacy = listAwarenessCommandDescriptors().filter(entry => !routine.has(entry.command));
  assert.equal(value.isError, false);
  assert.equal(details(value).status, 'listed');
  assert.equal(details(value).count, legacy.length);
  const legacyEntries = details(value).entries as Array<{ command: string }>;
  assert.ok(legacyEntries.every(entry => !routine.has(entry.command)));
  assert.equal(
    (details(value).next as Record<string, unknown> | undefined)?.tool,
    'awareness'
  );
  const next = details(value).next as { queries?: unknown[] } | undefined;
  assert.equal(Array.isArray(next?.queries), true, 'pagination continuation must use the registered query envelope');
  assert.equal(
    compileMcpSchemaValidator(tool.parameters).validate({ queries: next?.queries ?? [] }).valid,
    true,
    'pagination continuation must be directly executable by the awareness tool',
  );
  const parameters = tool.parameters as {
    properties?: Record<string, { maxItems?: number }>;
  };
  assert.equal(parameters.properties?.queries?.maxItems, 100);
  const modelText = String((value.content[0] as { text?: string }).text);
  assert.equal(modelText.includes('\n'), false);
  assert.ok(
    modelText.length < 4_000,
    `list payload is ${modelText.length} chars`
  );
  const modelPayload = JSON.parse(modelText) as {
    entries: Array<Record<string, unknown>>;
  };
  assert.equal(modelPayload.entries.length, Math.min(10, legacy.length));
  assert.deepEqual(
    modelPayload.entries.map(entry => entry.command).sort(),
    legacy.slice(0, 10).map(entry => entry.command).sort(),
    'legacy list page must expose only operator/recovery commands',
  );
  assert.equal(modelPayload.entries[0]?.injected, undefined);
  assert.ok(
    modelPayload.entries.every(entry => entry.approvalClass === undefined || typeof entry.approvalClass === 'string')
  );
});

test('keeps specialist routes available through explicit all discovery', async () => {
  const tool = makeTool();
  const value = await run(tool, { operation: 'legacy', action: 'list', all: true });
  const all = listAwarenessCommandDescriptors();
  assert.equal(details(value).count, all.length);
  const entries = details(value).entries as Array<{ command: string }>;
  assert.ok(entries.length > 0);
  assert.ok(all.some(entry => entry.command === 'refinement set'));
  assert.ok(all.some(entry => entry.command === 'session capture'));
  const refinement = await run(tool, { operation: 'legacy', action: 'list', command: 'refinement' });
  assert.equal(details(refinement).count, all.filter(entry => entry.command.startsWith('refinement ')).length);
});

test('paginates every explicit legacy command exactly once and describes every native schema', async () => {
  const tool = makeTool();
  const expected = listAwarenessCommandDescriptors();
  const listed = new Set<string>();
  let request: Record<string, unknown> = { queries: [{ reasoning: 'Discover legacy Awareness commands', operation: 'legacy', action: 'list', all: true }] };
  for (let page = 0; page <= expected.length; page += 1) {
    assert.equal(compileMcpSchemaValidator(tool.parameters).validate(request).valid, true);
    const value = await tool.execute('catalog-page', request, undefined, undefined, { cwd: root } as PiContext);
    const entries = details(value).entries as Array<{ command: string }>;
    for (const entry of entries)
      assert.equal(listed.has(entry.command), false, entry.command);
    for (const entry of entries) listed.add(entry.command);
    const next = details(value).next as { queries: unknown[] } | undefined;
    if (!next) break;
    request = { queries: next.queries };
  }
  assert.deepEqual(
    [...listed].sort(),
    expected.map(entry => entry.command).sort()
  );

  for (const expectedDescriptor of expected) {
    const value = await run(tool, {
      operation: 'legacy',
      action: 'describe',
      command: expectedDescriptor.command,
    });
    assert.equal(value.isError, false, expectedDescriptor.command);
    const modelText = String((value.content[0] as { text?: string }).text);
    assert.ok(
      modelText.length < 3_000,
      `${expectedDescriptor.command}: ${modelText.length} model-visible chars`
    );
    assert.equal(modelText.includes('\n'), false, expectedDescriptor.command);
    const descriptor = details(value).descriptor as Record<string, unknown>;
    assert.equal(
      descriptor.effect,
      expectedDescriptor.effect,
      expectedDescriptor.command
    );
    assert.equal(
      descriptor.piMode,
      expectedDescriptor.piMode,
      expectedDescriptor.command
    );
    const inputSchema = descriptor.inputSchema as Record<string, unknown>;
    assert.doesNotThrow(
      () => compileMcpSchemaValidator(inputSchema),
      expectedDescriptor.command
    );
    const properties = (inputSchema.properties ?? {}) as Record<
      string,
      unknown
    >;
    for (const hostField of [
      'db',
      'database',
      'workspace',
      'agent_id',
      'lead_agent_id',
      'compact',
    ]) {
      assert.equal(
        properties[hostField],
        undefined,
        `${expectedDescriptor.command}: ${hostField}`
      );
    }
  }
});

test('describes native params without host-injected actor or workspace fields', async () => {
  const value = await run(makeTool(), {
    action: 'describe',
    command: 'signal publish',
  });
  assert.equal(value.isError, false);
  const descriptor = details(value).descriptor as Record<string, unknown>;
  assert.equal(descriptor.effect, 'coordination-write');
  const schema = descriptor.inputSchema as Record<string, unknown>;
  const properties = schema.properties as Record<string, unknown>;
  assert.equal(properties.agent_id, undefined);
  assert.equal(properties.workspace, undefined);
  assert.deepEqual(schema['x-pi-host-injected'], [
    'database',
    'workspace',
    'agent-id',
    'compact',
  ]);

  const modelText = String((value.content[0] as { text?: string }).text);
  assert.equal(modelText.includes('\n'), false);
  const modelDescriptor = JSON.parse(modelText) as Record<string, unknown>;
  assert.equal(modelDescriptor.schema, undefined);
  assert.equal(modelDescriptor.example, undefined);
  const modelSchema = modelDescriptor.inputSchema as Record<string, unknown>;
  assert.equal(modelSchema.$schema, undefined);
  assert.equal(modelSchema['x-cli-command'], undefined);
  assert.deepEqual(modelDescriptor.hostInjected, [
    'database',
    'workspace',
    'agent-id',
    'compact',
  ]);
});

test('projects every history capture branch without requesting host-owned fields', async () => {
  const tool = makeTool();
  const value = await run(tool, {
    action: 'describe',
    command: 'history capture',
  });
  assert.equal(value.isError, false);
  const schema = (
    details(value).descriptor as { inputSchema: Record<string, unknown> }
  ).inputSchema;
  const validator = compileMcpSchemaValidator(schema);
  assert.equal(
    validator.validate({ phase: 'before', file: ['a.ts'] }).valid,
    true
  );
  assert.equal(
    validator.validate({
      phase: 'after',
      operation_id: 'edit',
      outcome: 'success',
    }).valid,
    true
  );
  assert.equal(
    validator.validate({ phase: 'after', operation_id: 'edit' }).valid,
    false
  );
  assert.equal(
    validator.validate({ phase: 'before', file: ['a.ts'], agent_id: 'other' })
      .valid,
    false
  );
  for (const branch of schema.oneOf as Array<{
    properties: Record<string, unknown>;
    required: string[];
  }>) {
    for (const field of ['workspace', 'agent_id']) {
      assert.equal(branch.properties[field], undefined);
      assert.equal(branch.required.includes(field), false);
    }
  }
});

test('exports the canonical agent prompt through the native API and exposes setup with approvals', async () => {
  const tool = makeTool();
  const value = await run(tool, {
    operation: 'legacy',
    action: 'call',
    command: 'instructions export',
    params: { format: 'json' },
  });
  assert.equal(value.isError, false);
  assert.match(
    String((details(value).output as { instructions: string }).instructions),
    /Start with context\.orient once/
  );
  for (const command of ['skill install', 'hooks install', 'hooks remove']) {
    const described = await run(tool, { action: 'describe', command });
    const descriptor = details(described).descriptor as {
      piMode: string;
      approvalClass: string;
    };
    assert.equal(descriptor.piMode, 'normal');
    assert.ok(descriptor.approvalClass);
  }
});

test('runs the imported API end to end with host bindings and structured metadata', async () => {
  const tool = makeTool();
  const status = await run(tool, { action: 'call', command: 'status' });
  assert.equal(
    status.isError,
    false,
    String((status.content[0] as { text?: string }).text)
  );
  assert.equal(details(status).status, 'ok');
  assert.equal(details(status).code, 0);

  const schema = await run(tool, {
    action: 'call',
    command: 'schema command',
    params: { noun: 'signal', subcommand: 'publish' },
  });
  assert.equal(
    schema.isError,
    false,
    String((schema.content[0] as { text?: string }).text)
  );
  assert.equal(details(schema).status, 'ok');
});

test('preserves command-shaped peer data while executing native inbox continuations', async () => {
  const tool = makeTool();
  const ctx = { cwd: root, sessionManager: { getSessionId: () => 'payload-roundtrip' } } as PiContext;
  const data = { type: 'review.evidence', payload: {
    command: 'signal list', params: { workspace: '/evidence-only', limit: 7 },
    note: 'This is peer evidence, not a continuation',
    cli: { name: 'signal list', args: ['--limit', '7'] },
    action: { operation: 'agent_signal', request: { action: 'ack', signal_id: ['evidence-only'] } },
    invocation: { argv: ['signal', 'list', '--limit', '7'] },
  } };
  for (let index = 0; index < 2; index++) {
    const sent = await run(tool, { action: 'call', command: 'signal publish', params: {
      kind: 'fyi', subject: `Evidence ${index}`, to_agent: ['pi:payload-roundtrip'], data,
    } }, ctx);
    assert.equal(sent.isError, false);
  }
  let queries: Record<string, unknown>[] = [{ action: 'call', command: 'signal list', params: { limit: 1, include_bodies: true, mark_read: true } }];
  const seen = new Set<string>();
  for (let page = 0; page < 3; page++) {
    const response = await runQueries(tool, queries, ctx);
    assert.equal(response.isError, false);
    const output = details(response).output as { signals: Array<{ signal_id: string; data: unknown }>; partial: boolean; next?: { list: { command: { queries: Record<string, unknown>[] } } } };
    for (const signal of output.signals) {
      assert.deepEqual(signal.data, data);
      seen.add(signal.signal_id);
    }
    if (!output.partial) break;
    assert.ok(output.next?.list.command.queries);
    queries = output.next.list.command.queries;
    assert.equal(compileMcpSchemaValidator(tool.parameters).validate({ queries }).valid, true);
  }
  assert.equal(seen.size, 2);
});

test('calls the package API with request objects and separate trusted bindings', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async () => ({
    payload: { ok: true, signal_id: 'sig_1' },
    exitCode: 0,
  }));
  const value = await run(makeTool(runner), {
    action: 'call',
    command: 'signal publish',
    params: { kind: 'fyi', subject: 'Native parity' },
    timeoutMs: 321,
  });
  assert.equal(value.isError, false);
  assert.deepEqual(runner.mock.calls[0]?.[0], {
    command: 'signal publish',
    params: { kind: 'fyi', subject: 'Native parity' },
  });
  const bindings = runner.mock.calls[0]?.[1];
  assert.equal(bindings?.workspace, root);
  assert.equal(bindings?.timeoutMs, 321);
  assert.ok(bindings?.agentId);
  assert.match(String(bindings?.database), /awareness\.sqlite3$/);
});

test('injects workspace and actor for verification without building argv', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async () => ({
    payload: { ok: true },
    exitCode: 0,
  }));
  const value = await run(makeTool(runner), {
    action: 'call',
    command: 'verify mark',
    params: { all_pending: true, message: 'isolated checks passed' },
  });
  assert.equal(value.isError, false);
  assert.equal(runner.mock.calls[0]?.[1].workspace, root);
  assert.ok(runner.mock.calls[0]?.[1].agentId);
});

test('treats verify audit debt exit 1 as a successful attention report', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: { ok: true, count: 2, unverified: [{ run_id: 'r-1' }] },
    exitCode: 1,
  }));
  const value = await run(makeTool(exec), {
    action: 'call',
    command: 'verify audit',
  });
  assert.equal(value.isError, false);
  assert.equal(details(value).status, 'attention');
  assert.equal(details(value).code, 1);
  assert.deepEqual((details(value).output as { count: number }).count, 2);
});

test('does not duplicate compact child JSON in batch receipts', async () => {
  const marker = 'UNIQUE_BATCH_PAYLOAD';
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: { ok: true, count: 1, marker },
    exitCode: 0,
  }));
  const value = await runQueries(makeTool(exec), [
    { action: 'call', command: 'status' },
    { action: 'call', command: 'status' },
  ]);
  assert.notEqual(value.isError, true);
  const texts = value.content
    .filter(
      (part): part is { type: 'text'; text: string } => part.type === 'text'
    )
    .map(part => part.text);
  assert.equal(texts[0]?.includes(marker), false);
  assert.equal(texts.join('\n').split(marker).length - 1, 2);
});

test('bounds model-visible command output and reports truncation', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    text: 'x'.repeat(13_000),
    payload: null,
    exitCode: 0,
  }));
  const value = await run(makeTool(exec), {
    action: 'call',
    command: 'status',
  });
  assert.equal(value.isError, false);
  assert.equal(details(value).truncated, true);
  assert.equal(details(value).totalChars, 13_000);
  assert.ok(
    String((value.content[0] as { text?: string }).text).length < 12_000
  );
  const packet = JSON.parse(
    String((value.content[0] as { text?: string }).text)
  );
  assert.equal(packet.partial, true);
  assert.equal(packet.diagnostic.code, 'AWARENESS_OUTPUT_LIMIT');
  assert.equal(packet.next.retry.queries[0].params.limit, 1);
  assert.equal(
    compileMcpSchemaValidator(makeTool().parameters).validate({
      queries: packet.next.retry.queries,
    }).valid,
    true
  );
});

test('does not offer to replay a completed mutation when its output is oversized', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({ payload: { receipt: 'x'.repeat(13_000) }, exitCode: 0 }));
  const value = await run(makeTool(exec), { action: 'call', command: 'agent register', params: { limit: 10 } });
  assert.equal(value.isError, false);
  const packet = JSON.parse(String((value.content[0] as { text?: string }).text));
  assert.equal(packet.next, undefined);
  assert.equal(packet.diagnostic.kind, 'terminal-limit');
  assert.equal(packet.commandCompleted, true);
  assert.match(packet.hint, /Do not repeat/);
  assert.equal(exec.mock.calls.length, 1);
});

test('preserves recovery handles and application details for an oversized completed write', async () => {
  const payload = {
    ok: true, preview_id: 'restore_example', status: 'ready',
    expires_at: '2026-09-10T12:00:00.000Z',
    operation: { operation_id: 'history_example', details: 'x'.repeat(13_000) },
    files: ['a.ts'],
  };
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({ payload, exitCode: 0 }));
  const value = await run(makeTool(exec), { action: 'call', command: 'agent register', params: {} });
  const packet = JSON.parse(String((value.content[0] as { text?: string }).text));
  assert.equal(packet.commandCompleted, true);
  assert.equal(packet.receipt.preview_id, payload.preview_id);
  assert.equal(packet.receipt.operation.operation_id, payload.operation.operation_id);
  assert.equal(packet.receipt.expires_at, payload.expires_at);
  assert.equal(packet.next, undefined);
  assert.ok(JSON.stringify(packet).length < 12_000);
  assert.deepEqual(details(value).output, payload);
  assert.equal(exec.mock.calls.length, 1);
});

test('returns native history navigation from a verified-memory call', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    exitCode: 0,
    payload: { memories: [{ memoryId: 'mem_example', historyEvidence: {
      state: 'recorded', reason: 'Captured source', next: { call: {
        command: 'history inspect', params: { operation_id: 'history_example', source_workspace: root },
      } },
    } }] },
  }));
  const value = await run(makeTool(exec), { action: 'call', command: 'memory recall-verified', params: { memory_id: 'mem_example' } });
  assert.equal(value.isError, false);
  const packet = JSON.parse(String((value.content[0] as { text?: string }).text));
  const next = packet.memories[0].historyEvidence.next.call;
  assert.equal(next.tool, 'awareness');
  assert.equal(next.queries[0].command, 'history inspect');
  assert.equal(next.queries[0].params.source_workspace, root);
});

test('preserves exit code 2 as blocked instead of success', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: { blocked: true },
    exitCode: 2,
  }));
  const value = await run(makeTool(exec), {
    action: 'call',
    command: 'lock wait',
    params: {
      artifact: 'pkg',
      target_file: ['src/file.ts'],
      wait_seconds: 120,
      retry_interval: 5,
    },
  });
  assert.equal(value.isError, true);
  assert.equal(details(value).status, 'blocked');
  assert.equal(details(value).code, 2);
});

test('preserves host cancellation as a distinct terminal status', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: null,
    diagnostics: ['aborted'],
    exitCode: 1,
    cancelled: true,
  }));
  const value = await run(makeTool(exec), {
    action: 'call',
    command: 'status',
  });
  assert.equal(value.isError, true);
  assert.equal(details(value).status, 'cancelled');
  assert.equal(details(value).killed, true);
});

test('rejects params that do not match the canonical command schema before execution', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: {},
    exitCode: 0,
  }));
  const value = await run(makeTool(exec), {
    action: 'call',
    command: 'status',
    params: { not_a_status_flag: true },
  });
  assert.equal(value.isError, true);
  assert.match(
    String((value.content[0] as { text?: string }).text),
    /Invalid parameters/
  );
  assert.equal(exec.mock.calls.length, 0);
});

test('rejects caller overrides for host-owned context', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: {},
    exitCode: 0,
  }));
  const value = await run(makeTool(exec), {
    action: 'call',
    command: 'status',
    params: { workspace: '/other' },
  });
  assert.equal(value.isError, true);
  assert.match(
    String((value.content[0] as { text?: string }).text),
    /host-injected/
  );
  assert.equal(exec.mock.calls.length, 0);
});

test('executes approval-protected commands only after an interactive grant', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: {},
    exitCode: 0,
  }));
  let approvalPrompt = '';
  const ctx = {
    cwd: root,
    hasUI: true,
    ui: {
      select: async (prompt: string, choices: string[]) => {
        approvalPrompt = prompt;
        return choices[0];
      },
    },
  } as unknown as PiContext;
  const value = await run(
    makeTool(exec),
    { action: 'call', command: 'memory prune', params: { older_than: '7d' } },
    ctx
  );
  assert.equal(value.isError, false);
  assert.equal(details(value).status, 'ok');
  assert.match(approvalPrompt, /older_than.*7d/);
  assert.equal(exec.mock.calls.length, 1);
});

test('denies external-host and approval-protected commands without execution', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: {},
    exitCode: 0,
  }));
  const external = await run(makeTool(exec), {
    action: 'call',
    command: 'hook run',
    params: { event: 'post-edit', payload: {} },
  });
  assert.equal(external.isError, true);
  assert.match(
    String((external.content[0] as { text?: string }).text),
    /external-host-only/
  );

  const protectedCall = await run(makeTool(exec), {
    action: 'call',
    command: 'memory prune',
    params: { older_than: '7d' },
  });
  assert.equal(protectedCall.isError, true);
  assert.equal(details(protectedCall).status, 'denied');
  assert.equal(exec.mock.calls.length, 0);
});

test('memory storage mode rejects durable calls before execution', async () => {
  process.env.OCTOCODE_STORAGE_MODE = 'memory';
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: {},
    exitCode: 0,
  }));
  const value = await run(makeTool(exec), {
    action: 'call',
    command: 'status',
  });
  assert.equal(value.isError, true);
  assert.match(
    String((value.content[0] as { text?: string }).text),
    /Persistent storage is disabled/
  );
  assert.equal(exec.mock.calls.length, 0);
});

test('preflights the full batch before executing an earlier valid command', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: {},
    exitCode: 0,
  }));
  const value = await runQueries(makeTool(exec), [
    { action: 'call', command: 'status' },
    { action: 'call', command: 'status', params: { invalid_flag: true } },
  ]);
  assert.equal(value.isError, true);
  assert.match(
    String((value.content[0] as { text?: string }).text),
    /queries\[1\] failed preflight/
  );
  assert.equal(exec.mock.calls.length, 0);
});

test('rejects multiple state-changing commands before any batch item executes', async () => {
  const exec = vi.fn(async (): Promise<AwarenessCommandResult> => ({
    payload: {},
    exitCode: 0,
  }));
  const value = await runQueries(makeTool(exec), [
    { action: 'call', command: 'work start', params: { file: ['src/a.ts'] } },
    { action: 'call', command: 'work start', params: { file: ['src/b.ts'] } },
  ]);
  assert.equal(value.isError, true);
  assert.match(
    String((value.content[0] as { text?: string }).text),
    /at most one state-changing Awareness command/i
  );
  assert.equal(exec.mock.calls.length, 0);
});
