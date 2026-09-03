import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';
import { Type } from 'typebox';
import type { ExternalMemoryParams, ExternalMemoryResult } from '@octocodeai/octocode-awareness';
import type { ToolDefinition, PiContext, PiTheme } from '../src/types.js';
import { registerMemoryTool, setMemoryActionRunnerForTests } from '../src/tools/memory-tool.js';

function loadTool(): ToolDefinition {
  let captured: ToolDefinition | undefined;
  registerMemoryTool({ registerTool: (def) => { captured = def; } }, Type, new Set(), (_pi, _names, def) => { captured = def; });
  if (!captured) throw new Error('memory tool not registered');
  return captured;
}

const ctx = { cwd: '/tmp/mem-ws' } as unknown as PiContext;
const theme = { fg: (_c: string, text: string) => text, bold: (text: string) => text } as unknown as PiTheme;

afterEach(() => setMemoryActionRunnerForTests(null));

function envelope(...queries: Array<Partial<ExternalMemoryParams> & Pick<ExternalMemoryParams, 'action'>>) {
  return { queries: queries.map((query) => ({ reasoning: 'exercise memory behavior', ...query })) };
}

test('memory exposes one query envelope derived from the package action contract', () => {
  const schema = loadTool().parameters as {
    properties?: { queries?: { items?: { properties?: Record<string, { enum?: string[] }>; required?: string[] } } };
    required?: string[];
  };
  assert.deepEqual(Object.keys(schema.properties ?? {}), ['queries', 'queryRunType']);
  assert.ok(schema.required?.includes('queries'));
  assert.deepEqual(schema.properties?.queries?.items?.properties?.['action']?.enum, ['recall', 'record', 'forget', 'review', 'suggest']);
  assert.ok(schema.properties?.queries?.items?.required?.includes('reasoning'));
});

test('memory forwards typed requests and composes host-visible results', async () => {
  const calls: Array<{ workspace: string; params: ExternalMemoryParams }> = [];
  setMemoryActionRunnerForTests((input) => {
    calls.push(input);
    return { action: 'recall', summary: 'Recalled 1 memory.', result: [{ memoryId: 'mem_1' }], count: 1 };
  });
  const result = await loadTool().execute('m1', envelope({ action: 'recall', query: 'adapter' }), undefined, undefined, ctx);
  assert.deepEqual(calls, [{ workspace: '/tmp/mem-ws', params: { action: 'recall', query: 'adapter', reasoning: 'exercise memory behavior' } }]);
  assert.match((result.content[0] as { text: string }).text, /Recalled 1 memory/);
  assert.equal((result.details as { count: number }).count, 1);
});

test('memory preflights a batch before any mutation and preserves source order', async () => {
  const calls: string[] = [];
  setMemoryActionRunnerForTests(({ params }) => {
    calls.push(params.action);
    return { action: params.action, summary: params.action } as ExternalMemoryResult;
  });
  const tool = loadTool();
  await assert.rejects(tool.execute('m2', envelope(
    { action: 'record', label: 'GOTCHA', observation: 'A verified reusable observation.', importance: 8 },
    { action: 'forget' },
  ), undefined, undefined, ctx), /queries\[1\].*memoryId/);
  assert.deepEqual(calls, []);

  await tool.execute('m3', envelope(
    { action: 'recall', query: 'one' },
    { action: 'recall', query: 'two' },
  ), undefined, undefined, ctx);
  assert.deepEqual(calls, ['recall', 'recall']);
});

test('memory renders suggest/review payloads and runner failures consistently', async () => {
  const tool = loadTool();
  const results: ExternalMemoryResult[] = [
    { action: 'suggest', summary: 'Suggested memory candidate (not recorded).', candidate: { action: 'record', label: 'EXPERIENCE' } },
    { action: 'review', summary: 'Reviewed 1 memory; found 1 candidate.', result: [{ memoryId: 'mem_1' }], candidates: [{ memoryId: 'mem_1', label: 'GOTCHA', issues: ['missing-source'], preview: 'x' }] },
  ];
  setMemoryActionRunnerForTests(() => results.shift()!);
  const suggested = await tool.execute('m4', envelope({ action: 'suggest', observation: 'A durable candidate learning.' }), undefined, undefined, ctx);
  assert.match((suggested.content[0] as { text: string }).text, /not recorded/);
  const reviewed = await tool.execute('m5', envelope({ action: 'review' }), undefined, undefined, ctx);
  assert.match((reviewed.content[0] as { text: string }).text, /missing-source/);

  setMemoryActionRunnerForTests(() => { throw new Error('database unavailable'); });
  const failed = await tool.execute('m6', envelope({ action: 'recall', query: 'x' }), undefined, undefined, ctx);
  assert.equal(failed.isError, true);
  assert.match((failed.content[0] as { text: string }).text, /database unavailable/);
});

test('memory validates single calls before execution and renders semantic UI states', async () => {
  let called = false;
  setMemoryActionRunnerForTests(() => { called = true; return { action: 'record', summary: 'recorded' }; });
  const tool = loadTool();
  const invalid = await tool.execute('m7', envelope({ action: 'record', label: 'GOTCHA', observation: 'short', importance: 8 }), undefined, undefined, ctx);
  assert.equal(invalid.isError, true);
  assert.equal(called, false);
  const call = tool.renderCall?.(envelope({ action: 'recall', query: 'locks' }), theme) as { render(width?: number): string[] };
  const success = tool.renderResult?.({ content: [{ type: 'text', text: 'Recalled 1 memory.' }] }, {}, theme) as { render(width?: number): string[] };
  const failure = tool.renderResult?.({ content: [{ type: 'text', text: 'failed' }], isError: true }, {}, theme) as { render(width?: number): string[] };
  assert.match(call.render(80).join('\n'), /recall.*locks/);
  assert.match(success.render(80).join('\n'), /Recalled 1 memory/);
  assert.match(failure.render(80).join('\n'), /failed/);
});
