import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test, vi } from 'vitest';
import { createAwarenessHost } from '@octocodeai/octocode-awareness/host';

vi.mock('../src/tools/execution-runtime.js', () => ({ emitExecution: vi.fn() }));
vi.mock('../src/branding/renderers.js', () => ({ withOctocodeRender: (tool: unknown) => tool }));

import { type AwarenessOperationRunner } from '../src/tools/awareness-operation-runner.js';
import { registerAwarenessTool } from '../src/tools/awareness-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { ToolResultError } from '../src/tools/tool-result-error.js';
import type { PiContext, PiInstance, ToolCallResult, ToolDefinition } from '../src/types.js';

const FILE_BYTES = 64 * 1024;
const MODEL_TEXT_LIMIT = 12_000;
const MAX_READ_CALLS = 32;
const OPERATION_ID = 'history-budget-operation';

let root: string;
let priorHome: string | undefined;
let priorMode: string | undefined;
let priorDatabase: string | undefined;

beforeEach(() => {
  root = mkdtempSync(path.join(tmpdir(), 'pi-awareness-history-budget-'));
  priorHome = process.env.OCTOCODE_HOME;
  priorMode = process.env.OCTOCODE_STORAGE_MODE;
  priorDatabase = process.env.OCTOCODE_AWARENESS_DB;
  process.env.OCTOCODE_HOME = path.join(root, 'home');
  process.env.OCTOCODE_STORAGE_MODE = 'persistent';
  process.env.OCTOCODE_AWARENESS_DB = path.join(root, 'awareness.sqlite3');
});

afterEach(() => {
  if (priorHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = priorHome;
  if (priorMode === undefined) delete process.env.OCTOCODE_STORAGE_MODE;
  else process.env.OCTOCODE_STORAGE_MODE = priorMode;
  if (priorDatabase === undefined) delete process.env.OCTOCODE_AWARENESS_DB;
  else process.env.OCTOCODE_AWARENESS_DB = priorDatabase;
  rmSync(root, { recursive: true, force: true });
});

function makeTool(runner?: AwarenessOperationRunner): ToolDefinition {
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
    runner,
  );
  assert.ok(definition);
  return definition;
}

async function run(
  definition: ToolDefinition,
  query: Record<string, unknown>,
  ctx: PiContext,
): Promise<ToolCallResult> {
  try {
    return await definition.execute(
      'call',
      { queries: [{ reasoning: 'test bounded Awareness history read', ...query }] },
      undefined,
      undefined,
      ctx,
    );
  } catch (error) {
    if (error instanceof ToolResultError) return error.result;
    return {
      content: [{ type: 'text', text: error instanceof Error ? error.message : String(error) }],
      isError: true,
    };
  }
}

function modelText(value: ToolCallResult): string {
  const text = value.content.find((part): part is { type: 'text'; text: string } => part.type === 'text')?.text;
  assert.ok(text, 'native Awareness must return model text');
  return text;
}

function nextQueries(payload: Record<string, unknown>): Record<string, unknown>[] | undefined {
  const next = payload.next as Record<string, unknown> | undefined;
  const retry = next?.retry as Record<string, unknown> | undefined;
  const retryQueries = retry?.queries;
  if (Array.isArray(retryQueries)) return retryQueries as Record<string, unknown>[];
  const continuation = next?.call as Record<string, unknown> | undefined;
  const continuationQueries = continuation?.queries;
  if (Array.isArray(continuationQueries)) return continuationQueries as Record<string, unknown>[];
  const directQueries = next?.queries;
  return Array.isArray(directQueries) ? directQueries as Record<string, unknown>[] : undefined;
}

function makeUtf8Fixture(): Buffer {
  const seed = Buffer.from('utf8 π ש — bounded history fixture\n');
  const result = Buffer.alloc(FILE_BYTES);
  for (let offset = 0; offset < result.length; offset += seed.length) seed.copy(result, offset, 0, Math.min(seed.length, result.length - offset));
  return result;
}

function makeBinaryFixture(): Buffer {
  const result = Buffer.allocUnsafe(FILE_BYTES);
  for (let index = 0; index < result.length; index++) result[index] = (index * 73 + 19) & 0xff;
  return result;
}

async function capture(
  ctx: PiContext,
  phase: 'before' | 'after',
  files: string[],
  outcome?: string,
): Promise<void> {
  const host = createAwarenessHost({
    workspace: ctx.cwd,
    agentId: 'pi:history-budget',
    sessionId: ctx.sessionManager?.getSessionId?.(),
    database: process.env.OCTOCODE_AWARENESS_DB,
  });
  await host.captureHistory({
    phase,
    file: files,
    operation_id: OPERATION_ID,
    ...(outcome ? { outcome } : {}),
  });
}

test('bounds native history reads while preserving exact operation/file/side continuations', async () => {
  const tool = makeTool();
  const ctx = { cwd: root, sessionManager: { getSessionId: () => 'history-budget-session' } } as PiContext;
  const fixtures = [
    { name: 'utf8.txt', before: makeUtf8Fixture(), after: Buffer.from(makeUtf8Fixture()).reverse() },
    { name: 'binary.bin', before: makeBinaryFixture(), after: Buffer.from(makeBinaryFixture()).map((value, index) => value ^ (index & 0xff)) },
  ];
  for (const fixture of fixtures) writeFileSync(path.join(root, fixture.name), fixture.before);
  await capture(ctx, 'before', fixtures.map(fixture => fixture.name));
  for (const fixture of fixtures) writeFileSync(path.join(root, fixture.name), fixture.after);
  await capture(ctx, 'after', fixtures.map(fixture => fixture.name), 'success');

  for (const fixture of fixtures) {
    for (const side of ['before', 'after'] as const) {
      const expected = fixture[side];
      let queries: Record<string, unknown>[] | undefined = [{
        operation: 'history.read',
        // Deliberately omit limit: this exercises the native output-limit retry.
        params: { operation_id: OPERATION_ID, file: fixture.name, side },
      }];
      const chunks: Buffer[] = [];
      let calls = 0;
      let retrySeen = false;
      while (queries) {
        calls += 1;
        assert.ok(calls <= MAX_READ_CALLS, `${fixture.name} ${side} exceeded ${MAX_READ_CALLS} native calls`);
        const query: Record<string, unknown> = queries[0];
        assert.equal(query.operation, 'history.read');
        const params = query.params as Record<string, unknown>;
        assert.equal(params.operation_id, OPERATION_ID);
        assert.equal(params.file, fixture.name);
        assert.equal(params.side, side);
        const response = await run(tool, query, ctx);
        assert.equal(response.isError, false, modelText(response));
        const text = modelText(response);
        assert.ok(text.length <= MODEL_TEXT_LIMIT, `${fixture.name} ${side} model text exceeded ${MODEL_TEXT_LIMIT}`);
        const payload = JSON.parse(text) as Record<string, unknown>;
        if (typeof payload.content === 'string') {
          const offset = Number(payload.offset ?? chunks.reduce((sum, chunk) => sum + chunk.length, 0));
          assert.equal(offset, chunks.reduce((sum, chunk) => sum + chunk.length, 0));
          chunks.push(Buffer.from(payload.content, 'base64'));
        }
        if (payload.next && typeof payload.next === 'object' && 'retry' in payload.next) retrySeen = true;
        const continued = nextQueries(payload);
        queries = continued && continued.length > 0 ? continued : undefined;
      }
      assert.equal(retrySeen, true, `${fixture.name} ${side} must exercise the native output-limit retry`);
      assert.deepEqual(Buffer.concat(chunks), expected, `${fixture.name} ${side} history bytes must be exact`);
      assert.equal(Buffer.concat(chunks).length, FILE_BYTES);
    }
  }

  assert.deepEqual(readFileSync(path.join(root, 'utf8.txt')), fixtures[0].after);
  assert.deepEqual(readFileSync(path.join(root, 'binary.bin')), fixtures[1].after);
});
