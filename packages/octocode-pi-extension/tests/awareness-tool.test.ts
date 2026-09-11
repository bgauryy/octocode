import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test, vi } from 'vitest';
import type { AwarenessOperationResult } from '@octocodeai/octocode-awareness';
import type { AwarenessOperationRunner } from '../src/tools/awareness-operation-runner.js';
import { registerAwarenessTool } from '../src/tools/awareness-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { ToolResultError } from '../src/tools/tool-result-error.js';
import { compileMcpSchemaValidator } from '../src/tools/mcp/schema-validator.js';
import type { PiContext, PiInstance, ToolCallResult, ToolDefinition } from '../src/types.js';

vi.mock('../src/tools/execution-runtime.js', () => ({ emitExecution: vi.fn() }));
vi.mock('../src/branding/renderers.js', () => ({ withOctocodeRender: (tool: unknown) => tool }));

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

function makeTool(runner?: AwarenessOperationRunner): ToolDefinition {
  let definition: ToolDefinition | undefined;
  const pi = { registerTool(value: ToolDefinition) { definition = value; } } as PiInstance;
  registerAwarenessTool(pi, new Set(), (host, names, value) => registerUniqueTool(host, names, value), runner);
  assert.ok(definition);
  return definition;
}

async function runQueries(
  definition: ToolDefinition,
  queries: Record<string, unknown>[],
  ctx: PiContext = { cwd: root } as PiContext,
): Promise<ToolCallResult> {
  try {
    return await definition.execute(
      'call',
      { queries: queries.map(query => ({ reasoning: 'test canonical Awareness', ...query })) },
      undefined,
      undefined,
      ctx,
    );
  } catch (error) {
    if (error instanceof ToolResultError) return error.result;
    return { content: [{ type: 'text', text: error instanceof Error ? error.message : String(error) }], isError: true };
  }
}

function text(value: ToolCallResult): string {
  return String((value.content[0] as { text?: string }).text);
}

function details(value: ToolCallResult): Record<string, unknown> {
  return (value.details ?? {}) as Record<string, unknown>;
}

test('publishes only the canonical operation envelope', () => {
  const tool = makeTool();
  const schema = JSON.stringify(tool.parameters);
  const guidance = [tool.description, tool.promptSnippet, ...(tool.promptGuidelines ?? [])].join('\n');
  assert.match(schema, /context\.orient/);
  assert.match(schema, /history\.restore/);
  assert.doesNotMatch(schema, /legacy|"command"|"action"|pageSize/);
  assert.doesNotMatch(guidance, /legacy|list-describe-call/i);
  assert.ok(Buffer.byteLength(schema) < 2_000);
  assert.ok(Buffer.byteLength(guidance) < 1_200);
});

test('executes an operation with trusted host bindings and timeout', async () => {
  const runner = vi.fn<AwarenessOperationRunner>(async () => ({ exitCode: 0, payload: { revision: 'r1', unchanged: true } }));
  const result = await runQueries(makeTool(runner), [
    { operation: 'context.orient', params: { if_revision: 'r0' }, timeoutMs: 321 },
  ]);
  assert.equal(result.isError, false);
  assert.deepEqual(runner.mock.calls[0]?.[0], { operation: 'context.orient', params: { if_revision: 'r0' } });
  assert.equal(runner.mock.calls[0]?.[1].workspace, root);
  assert.equal(runner.mock.calls[0]?.[1].timeoutMs, 321);
  assert.ok(runner.mock.calls[0]?.[1].agentId);
  assert.match(String(runner.mock.calls[0]?.[1].database), /awareness\.sqlite3$/);
});

test('rejects removed dispatch fields before execution', async () => {
  const runner = vi.fn<AwarenessOperationRunner>();
  const result = await runQueries(makeTool(runner), [
    { operation: 'message.send', command: 'removed route', params: { kind: 'fyi', subject: 'x' } },
  ]);
  assert.equal(result.isError, true);
  assert.match(text(result), /command is not part of the canonical Awareness surface/);
  assert.equal(runner.mock.calls.length, 0);
});

test('rejects invalid params and host-owned overrides before execution', async () => {
  const runner = vi.fn<AwarenessOperationRunner>();
  const invalid = await runQueries(makeTool(runner), [
    { operation: 'context.orient', params: { unknown: true } },
  ]);
  assert.equal(invalid.isError, true);
  assert.match(text(invalid), /Invalid parameters/);
  const override = await runQueries(makeTool(runner), [
    { operation: 'context.orient', params: { workspace: '/other' } },
  ]);
  assert.equal(override.isError, true);
  assert.match(text(override), /host-injected/);
  assert.equal(runner.mock.calls.length, 0);
});

test('allows batched reads but refuses every mixed or repeated mutation batch', async () => {
  const runner = vi.fn<AwarenessOperationRunner>(async () => ({ exitCode: 0, payload: { ok: true } }));
  const reads = await runQueries(makeTool(runner), [
    { operation: 'context.orient' },
    { operation: 'message.list' },
  ]);
  assert.notEqual(reads.isError, true);
  const mutations = await runQueries(makeTool(runner), [
    { operation: 'message.send', params: { kind: 'fyi', subject: 'one' } },
    { operation: 'message.send', params: { kind: 'fyi', subject: 'two' } },
  ]);
  assert.equal(mutations.isError, true);
  assert.match(text(mutations), /at most one state-changing Awareness operation/i);
});

test('treats verification debt as attention instead of a tool failure', async () => {
  const runner = vi.fn<AwarenessOperationRunner>(async (): Promise<AwarenessOperationResult> => ({
    payload: { ok: true, count: 2, unverified: [{ run_id: 'r-1' }] },
    exitCode: 1,
  }));
  const value = await runQueries(makeTool(runner), [
    { operation: 'work.verify', params: { action: 'audit' } },
  ]);
  assert.equal(value.isError, false);
  assert.equal(details(value).status, 'attention');
  assert.equal(details(value).code, 1);
});

test('bounds oversized reads with an executable canonical retry', async () => {
  const runner = vi.fn<AwarenessOperationRunner>(async () => ({
    text: 'x'.repeat(13_000), payload: { ok: true }, exitCode: 0,
  }));
  const value = await runQueries(makeTool(runner), [
    { operation: 'message.list', params: { limit: 20 } },
  ]);
  assert.equal(value.isError, false);
  assert.equal(details(value).truncated, true);
  const packet = JSON.parse(text(value));
  assert.equal(packet.next.retry.queries[0].operation, 'message.list');
  assert.ok(packet.next.retry.queries[0].params.limit < 20);
  assert.equal(compileMcpSchemaValidator(makeTool().parameters).validate({ queries: packet.next.retry.queries }).valid, true);
});

test('does not offer to replay an oversized completed mutation', async () => {
  const runner = vi.fn<AwarenessOperationRunner>(async () => ({
    payload: { ok: true, signal_id: 's1', body: 'x'.repeat(13_000) }, exitCode: 0,
  }));
  const value = await runQueries(makeTool(runner), [
    { operation: 'message.send', params: { kind: 'fyi', subject: 'one' } },
  ]);
  const packet = JSON.parse(text(value));
  assert.equal(packet.next, undefined);
  assert.equal(packet.commandCompleted, true);
  assert.equal(packet.receipt.signal_id, 's1');
  assert.equal(runner.mock.calls.length, 1);
});

test('reports cancelled and thrown executions without claiming success', async () => {
  const cancelled = vi.fn<AwarenessOperationRunner>(async () => ({ exitCode: 130, payload: {}, cancelled: true }));
  const cancelledValue = await runQueries(makeTool(cancelled), [{ operation: 'context.orient' }]);
  assert.equal(cancelledValue.isError, true);
  assert.equal(details(cancelledValue).status, 'cancelled');

  const failed = vi.fn<AwarenessOperationRunner>(async () => { throw new Error('boom'); });
  const failedValue = await runQueries(makeTool(failed), [{ operation: 'context.orient' }]);
  assert.equal(failedValue.isError, true);
  assert.match(text(failedValue), /boom/);
});

test('memory storage mode rejects durable operations before execution', async () => {
  process.env.OCTOCODE_STORAGE_MODE = 'memory';
  const runner = vi.fn<AwarenessOperationRunner>();
  const value = await runQueries(makeTool(runner), [{ operation: 'context.orient' }]);
  assert.equal(value.isError, true);
  assert.match(text(value), /persistent/i);
  assert.equal(runner.mock.calls.length, 0);
});
