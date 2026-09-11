import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test, vi } from 'vitest';
import type { AwarenessCommandResult } from '@octocodeai/octocode-awareness';
import type { AwarenessCommandRunner } from '../src/tools/awareness-command-runner.js';
import { registerAwarenessTool } from '../src/tools/awareness-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { ToolResultError } from '../src/tools/tool-result-error.js';
import type { PiContext, PiInstance, ToolCallResult, ToolDefinition } from '../src/types.js';

let root: string;
let priorHome: string | undefined;
let priorStorageMode: string | undefined;
beforeEach(() => {
  root = mkdtempSync(path.join(tmpdir(), 'pi-awareness-canonical-'));
  priorHome = process.env.OCTOCODE_HOME;
  priorStorageMode = process.env.OCTOCODE_STORAGE_MODE;
  process.env.OCTOCODE_HOME = path.join(root, 'home');
  process.env.OCTOCODE_STORAGE_MODE = 'persistent';
});
afterEach(() => {
  if (priorHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = priorHome;
  if (priorStorageMode === undefined) delete process.env.OCTOCODE_STORAGE_MODE;
  else process.env.OCTOCODE_STORAGE_MODE = priorStorageMode;
  rmSync(root, { recursive: true, force: true });
});

function makeTool(runner: AwarenessCommandRunner): ToolDefinition {
  let definition: ToolDefinition | undefined;
  const pi = { registerTool(value: ToolDefinition) { definition = value; } } as PiInstance;
  registerAwarenessTool(pi, new Set(), (host, names, value) => registerUniqueTool(host, names, value), runner);
  assert.ok(definition);
  return definition;
}

async function run(tool: ToolDefinition, queries: Record<string, unknown>[], ctx: PiContext = { cwd: root } as PiContext): Promise<ToolCallResult> {
  try {
    return await tool.execute('call', { queries: queries.map(query => ({ reasoning: 'canonical contract', ...query })) }, undefined, undefined, ctx);
  } catch (error) {
    if (error instanceof ToolResultError) return error.result;
    return { content: [{ type: 'text', text: error instanceof Error ? error.message : String(error) }], isError: true };
  }
}

test('exposes direct routine operations without list-describe-call ceremony', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async () => ({ exitCode: 0, payload: { revision: 'o1.fixture', unchanged: true } }));
  const tool = makeTool(runner);
  const schemaText = JSON.stringify(tool.parameters);
  const promptText = [tool.description, tool.promptSnippet, ...(tool.promptGuidelines ?? [])].join('\n');
  assert.ok(schemaText.includes('context.orient'), `baseline schema=${Buffer.byteLength(schemaText)} prompt=${Buffer.byteLength(promptText)}`);
  assert.ok(schemaText.includes('message.send'));
  assert.ok(schemaText.includes('legacy'));
  assert.ok(Buffer.byteLength(schemaText) < 2_000);
  assert.ok(Buffer.byteLength(promptText) < 1_200);

  const value = await run(tool, [{ operation: 'context.orient', params: { if_revision: 'o1.fixture' } }]);
  assert.equal(value.isError, false);
  assert.deepEqual(runner.mock.calls[0]?.[0], { operation: 'context.orient', params: { if_revision: 'o1.fixture' } });
  const bindings = runner.mock.calls[0]?.[1];
  assert.equal(bindings?.workspace, root);
  assert.ok(bindings?.agentId);
  assert.match(String(bindings?.database), /awareness\.sqlite3$/);
});

test('rejects legacy-only fields on routine operation branches', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async (): Promise<AwarenessCommandResult> => ({ exitCode: 0, payload: { ok: true } }));
  const value = await run(makeTool(runner), [{ operation: 'context.orient', action: 'list' }]);
  assert.equal(value.isError, true);
  assert.match(String((value.content[0] as { text?: string }).text), /action is only valid with operation:"legacy"/);
  assert.equal(runner.mock.calls.length, 0);
});

test('preflights a routine batch before allowing more than one possible mutation', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async (): Promise<AwarenessCommandResult> => ({ exitCode: 0, payload: { ok: true } }));
  const value = await run(makeTool(runner), [
    { operation: 'message.send', params: { kind: 'fyi', subject: 'one' } },
    { operation: 'message.send', params: { kind: 'fyi', subject: 'two' } },
  ]);
  assert.equal(value.isError, true);
  assert.match(String((value.content[0] as { text?: string }).text), /at most one state-changing Awareness operation/i);
  assert.equal(runner.mock.calls.length, 0);
});

test('distinguishes history restore preview from approval-protected apply', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async (): Promise<AwarenessCommandResult> => ({ exitCode: 0, payload: { ok: true } }));
  let approvals = 0;
  const ctx = { cwd: root, hasUI: true, ui: { select: async (_prompt: string, choices: string[]) => { approvals += 1; return choices[0]; } } } as unknown as PiContext;
  assert.equal((await run(makeTool(runner), [{ operation: 'history.restore', params: { action: 'preview', version_id: 'v1' } }], ctx)).isError, false);
  assert.equal(approvals, 0);
  assert.equal((await run(makeTool(runner), [{ operation: 'history.restore', params: { action: 'apply', preview_id: 'p1' } }], ctx)).isError, false);
  assert.equal(approvals, 1);
});

test('wraps canonical operation continuations in executable Pi envelopes', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async (): Promise<AwarenessCommandResult> => ({
    exitCode: 0,
    payload: { next: [{ operation: 'context.orient', params: { limit: 2, offset: 2 } }] },
  }));
  const value = await run(makeTool(runner), [{ operation: 'context.orient' }]);
  assert.equal(value.isError, false);
  const packet = JSON.parse(String((value.content[0] as { text?: string }).text));
  assert.deepEqual(packet.next[0].queries[0], {
    reasoning: 'Continue the requested Awareness results',
    operation: 'context.orient',
    params: { limit: 2, offset: 2 },
  });
});

test('filters the legacy administration catalog by command noun', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async () => ({ exitCode: 0, payload: {} }));
  const value = await run(makeTool(runner), [
    { operation: 'legacy', action: 'list', command: 'database', pageSize: 5 },
  ]);
  assert.equal(value.isError, false);
  const payload = JSON.parse(String((value.content[0] as { text?: string }).text)) as {
    entries: Array<{ command: string }>;
  };
  assert.ok(payload.entries.length > 0);
  assert.ok(payload.entries.every(entry => entry.command.startsWith('database')));
  assert.equal(runner.mock.calls.length, 0);
});

test('rejects legacy-only paging fields on routine operations', async () => {
  const runner = vi.fn<AwarenessCommandRunner>(async () => ({ exitCode: 0, payload: {} }));
  const value = await run(makeTool(runner), [
    { operation: 'context.orient', pageSize: 5 },
  ]);
  assert.equal(value.isError, true);
  assert.match(String((value.content[0] as { text?: string }).text), /pageSize is only valid with operation:"legacy"/);
  assert.equal(runner.mock.calls.length, 0);
});
