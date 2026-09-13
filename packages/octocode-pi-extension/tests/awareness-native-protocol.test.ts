import { afterEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { getAwarenessOperationDescriptor, ROUTINE_AWARENESS_OPERATIONS } from '@octocodeai/octocode-awareness';
import { createAwarenessHost } from '@octocodeai/octocode-awareness/host';
import {
  nativeOperationContinuations,
  routineApproval,
  validateRoutineParams,
} from '../src/tools/awareness-tool-protocol.js';
import { registerAwarenessTool } from '../src/tools/awareness-tool.js';
import { compileMcpSchemaValidator } from '../src/tools/mcp/schema-validator.js';
import type { AwarenessOperationRunner } from '../src/tools/awareness-operation-runner.js';
import { runAwarenessOperation } from '../src/tools/awareness-operation-runner.js';
import type { PiContext, PiInstance, ToolDefinition } from '../src/types.js';

afterEach(() => vi.unstubAllEnvs());

function tool(runner: AwarenessOperationRunner = vi.fn<AwarenessOperationRunner>()): ToolDefinition {
  let definition: ToolDefinition | undefined;
  registerAwarenessTool({} as PiInstance, new Set(), (_pi, _names, value) => {
    definition = value;
  }, runner);
  return definition!;
}

async function describeExactSchema(definition: ToolDefinition, operation: string): Promise<string> {
  const parts: string[] = [];
  let part: number | undefined;
  let total = 1;
  do {
    const query = { reasoning: 'Inspect the exact operation contract', operation, describe: true, ...(part === undefined ? {} : { part }) };
    expect(compileMcpSchemaValidator(definition.parameters).validate({ queries: [query] }).valid).toBe(true);
    const result = await definition.execute('describe', { queries: [query] }, undefined, undefined, {} as PiContext);
    expect(result.isError).not.toBe(true);
    const text = (result.content[0] as { text: string }).text;
    expect(Buffer.byteLength(text, 'utf8')).toBeLessThanOrEqual(2_000);
    const payload = JSON.parse(text) as {
      operation: string;
      inputSchemaText?: string;
      inputSchemaTextPart?: string;
      schemaPart?: { index: number; total: number };
      next?: { queries?: Array<{ part?: number }> };
    };
    expect(payload.operation).toBe(operation);
    if (payload.inputSchemaText !== undefined) return payload.inputSchemaText;
    expect(payload.schemaPart).toBeDefined();
    expect(typeof payload.inputSchemaTextPart).toBe('string');
    parts[payload.schemaPart!.index] = payload.inputSchemaTextPart!;
    total = payload.schemaPart!.total;
    part = payload.next?.queries?.[0]?.part;
  } while (part !== undefined);
  expect(parts).toHaveLength(total);
  expect(parts.every(value => typeof value === 'string')).toBe(true);
  return parts.join('');
}

type NativeCall = { tool: string; queries: { reasoning: string; operation: string; params?: Record<string, unknown> }[] };

describe('native Awareness continuations', () => {
  it.each(['next', 'actions', 'continuations', 'undo_preview'])('translates nested %s calls without changing their parameters', key => {
    const call = { operation: 'context.orient', params: { limit: 1, offset: 2 } };
    const result = nativeOperationContinuations({ [key]: { retry: call } }) as Record<string, { retry: NativeCall }>;
    const retry = result[key]!.retry;
    expect(retry.tool).toBe('awareness');
    expect(retry.queries[0]).toEqual({ reasoning: 'Continue the requested Awareness results', ...call });
    expect(compileMcpSchemaValidator(tool().parameters).validate({ queries: retry.queries }).valid).toBe(true);
    expect(getAwarenessOperationDescriptor(call.operation)!.validate(retry.queries[0]!.params)).toEqual(call.params);
  });

  it('leaves operation-shaped user data and unknown operations untouched', () => {
    const data = { operation: 'context.orient', params: { limit: 1 } };
    const payload = { data, next: { operation: 'unknown.operation', params: {} } };
    expect(nativeOperationContinuations(data)).toEqual(data);
    expect(nativeOperationContinuations(payload)).toEqual(payload);
  });

  it('delegates parameter validation and approvals to the canonical descriptor', () => {
    const original = getAwarenessOperationDescriptor('work.update')!;
    const descriptor = { ...original, validate: vi.fn(() => ({ transition: 'new-canonical-transition' })), approval: vi.fn(() => 'fs-delete' as const) };
    const params = { transition: 'new-canonical-transition' };
    expect(() => validateRoutineParams(descriptor, params)).not.toThrow();
    expect(descriptor.validate).toHaveBeenCalledWith(params);
    expect(routineApproval(descriptor, params)?.actionClass).toBe('fs-delete');
    expect(descriptor.approval).toHaveBeenCalledWith(params);
  });
});

it('executes a canonical nested retry and an actual History undo preview through the native tool', async () => {
  const workspace = mkdtempSync(path.join(tmpdir(), 'awareness-native-continuation-'));
  vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
  vi.stubEnv('OCTOCODE_HOME', path.join(workspace, 'home'));
  vi.stubEnv('OCTOCODE_AWARENESS_DB', path.join(workspace, 'awareness.sqlite3'));
  vi.stubEnv('OCTOCODE_AGENT_ID', 'native-continuation-agent');
  const ctx = { cwd: workspace, sessionManager: { getSessionId: () => 'native-continuation-session' }, hasUI: true,
    ui: { select: async (_prompt: string, choices: string[]) => choices[0] },
  } as unknown as PiContext;
  const definition = tool(runAwarenessOperation);
  const execute = async (queries: NativeCall['queries']) => {
    expect(compileMcpSchemaValidator(definition.parameters).validate({ queries }).valid).toBe(true);
    const result = await definition.execute('continuation', { queries }, undefined, undefined, ctx);
    expect(result.isError).not.toBe(true);
    return JSON.parse((result.content[0] as { text: string }).text);
  };
  try {
    const canonical = getAwarenessOperationDescriptor('context.orient')!.continuations({ next: { retry: { command: 'attend', params: { limit: 1 } } } });
    const translated = nativeOperationContinuations(canonical) as { next: { retry: NativeCall } };
    expect((await execute(translated.next.retry.queries)).self.actorId).toBe('native-continuation-agent');

    const file = path.join(workspace, 'example.txt');
    const host = createAwarenessHost({ workspace, database: process.env.OCTOCODE_AWARENESS_DB, agentId: 'native-continuation-agent', sessionId: 'native-continuation-session' });
    writeFileSync(file, 'before');
    await host.captureHistory({ phase: 'before', file: ['example.txt'], operation_id: 'native-history-operation' });
    writeFileSync(file, 'after');
    await host.captureHistory({ phase: 'after', file: ['example.txt'], operation_id: 'native-history-operation', outcome: 'success' });
    const preview = await execute([{ reasoning: 'Preview restore', operation: 'history.restore', params: { operation_id: 'native-history-operation', side: 'before' } }]);
    const restored = await execute([{ reasoning: 'Restore fixture', operation: 'history.restore', params: { action: 'apply', preview_id: preview.preview_id } }]);
    expect(readFileSync(file, 'utf8')).toBe('before');
    expect(restored.undo_preview.tool).toBe('awareness');
    const undo = await execute(restored.undo_preview.queries);
    expect(undo.status).toBe('ready');
    expect(undo.changed_files).toBe(1);
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
});

describe('native Awareness schema discovery', () => {
  it.each(ROUTINE_AWARENESS_OPERATIONS)('describes %s losslessly without storage, identity, or runner access', async operation => {
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'memory');
    const runner = vi.fn<AwarenessOperationRunner>();
    const definition = tool(runner);
    const schemaText = await describeExactSchema(definition, operation);
    expect(schemaText).toBe(getAwarenessOperationDescriptor(operation)!.inputSchemaText);
    expect(JSON.parse(schemaText)).toEqual(getAwarenessOperationDescriptor(operation)!.inputSchema);
    expect(runner).not.toHaveBeenCalled();
  });

  it('reconstructed executable schemas accept canonical calls with multiple optional fields', async () => {
    const definition = tool();
    const schema = JSON.parse(await describeExactSchema(definition, 'memory.recall'));
    const params = { query: 'schema fidelity', limit: 7, smart: true, explain: true };
    expect(() => getAwarenessOperationDescriptor('memory.recall')!.validate(params)).not.toThrow();
    expect(compileMcpSchemaValidator(schema).validate(params).valid).toBe(true);
  });

  it('rejects describe with execution parameters instead of silently ignoring them', async () => {
    const runner = vi.fn<AwarenessOperationRunner>();
    const definition = tool(runner);
    await expect(definition.execute('describe', { queries: [{ reasoning: 'Inspect', operation: 'context.orient', describe: true, params: { limit: 1 } }] }, undefined, undefined, {} as PiContext)).rejects.toThrow(/describe.*params|params.*describe/i);
    expect(runner).not.toHaveBeenCalled();
  });
});

it('offers only schema-valid retries for oversized native reads', async () => {
  vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
  const runner = vi.fn<AwarenessOperationRunner>(async () => ({ exitCode: 0, payload: { body: 'x'.repeat(13_000) } }));
  const definition = tool(runner);
  const run = async (operation: string) => {
    const result = await definition.execute('oversized', { queries: [{ reasoning: 'Read', operation }] }, undefined, undefined, {} as PiContext);
    return JSON.parse((result.content[0] as { text: string }).text);
  };
  const packet = await run('message.list');
  const retry = packet.next.retry.queries[0];
  expect(() => getAwarenessOperationDescriptor(retry.operation)!.validate(retry.params)).not.toThrow();
  expect(retry.params.limit).toBe(1);
  const terminal = await run('history.status');
  expect(terminal.diagnostic.kind).toBe('terminal-limit');
  expect(terminal.next).toBeUndefined();
});

it('recovers every complete Message body through native budget retries and page continuations', async () => {
  const workspace = mkdtempSync(path.join(tmpdir(), 'awareness-native-message-pages-'));
  const agentId = 'message-pagination-agent';
  vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
  vi.stubEnv('OCTOCODE_HOME', path.join(workspace, 'home'));
  vi.stubEnv('OCTOCODE_AWARENESS_DB', path.join(workspace, 'awareness.sqlite3'));
  vi.stubEnv('OCTOCODE_AGENT_ID', agentId);
  const definition = tool(runAwarenessOperation);
  const ctx = { cwd: workspace, sessionManager: { getSessionId: () => 'message-pagination-session' } } as PiContext;
  const execute = async (queries: NativeCall['queries']) => {
    expect(compileMcpSchemaValidator(definition.parameters).validate({ queries }).valid).toBe(true);
    for (const query of queries) expect(() => getAwarenessOperationDescriptor(query.operation)!.validate(query.params)).not.toThrow();
    const result = await definition.execute('messages', { queries }, undefined, undefined, ctx);
    expect(result.isError).not.toBe(true);
    return JSON.parse((result.content[0] as { text: string }).text);
  };
  const expected = new Map<string, string>();
  try {
    for (let index = 0; index < 5; index += 1) {
      const subject = `Message ${index}`;
      const body = `${index}:` + 'x'.repeat(2_998);
      expected.set(subject, body);
      await execute([{ reasoning: 'Seed addressed fixture', operation: 'message.send', params: { kind: 'fyi', subject, body, to_agent: [agentId] } }]);
    }
    let queries: NativeCall['queries'] | undefined = [{ reasoning: 'Read fixture bodies', operation: 'message.list', params: { all: true, include_bodies: true, limit: 200 } }];
    const observed = new Map<string, string>();
    let retried = false;
    let paged = false;
    for (let calls = 0; queries && calls < 20; calls += 1) {
      const packet = await execute(queries);
      for (const signal of packet.signals ?? []) observed.set(signal.subject, signal.body);
      const retry = packet.next?.retry as NativeCall | undefined;
      const page = packet.next?.list as NativeCall | undefined;
      if (retry) retried = true;
      if (page) paged = true;
      const continuation = retry ?? page;
      if (continuation) expect(continuation.tool).toBe('awareness');
      queries = continuation?.queries;
    }
    expect(queries).toBeUndefined();
    expect(retried).toBe(true);
    expect(paged).toBe(true);
    expect(observed).toEqual(expected);
  } finally {
    rmSync(workspace, { recursive: true, force: true });
  }
});
