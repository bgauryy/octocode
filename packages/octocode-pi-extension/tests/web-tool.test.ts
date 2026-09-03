import assert from 'node:assert/strict';
import { afterEach, test, vi } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition, ToolCallResult, PiTheme } from '../src/types.js';

const theme: PiTheme = {
  bold: (text: string) => `<b>${text}</b>`,
  fg: (color: string, text: string) => `<${color}>${text}</${color}>`,
};

async function loadRegisteredWebTool(
  out: Record<string, unknown>,
  options: { mockEnv?: boolean } = {},
) {
  vi.resetModules();
  const runWebTool = vi.fn(async () => out);
  const renderWebResult = vi.fn((result: unknown) => {
    const r = result as { title?: string; url?: string };
    return [`Title: ${r.title ?? 'untitled'}`, `URL: ${r.url ?? 'n/a'}`].join('\n');
  });
  const propagateOctocodeEnv = vi.fn(() => ({ applied: [], skippedExisting: [], skippedProtected: [], keys: [], sources: {} }));
  const getOctocodeHome = vi.fn(() => '/mock/home');
  vi.doMock('../src/web.js', () => ({ runWebTool, renderWebResult }));
  if (options.mockEnv) vi.doMock('../src/env.js', () => ({ propagateOctocodeEnv, getOctocodeHome }));

  const { registerWebTool } = await import('../src/tools/web-tool.js');
  const tools = new Map<string, ToolDefinition>();
  const pi = {
    registerTool(def: ToolDefinition) {
      tools.set(def.name, def);
    },
  };
  const registeredNames = new Set<string>();
  const registerFn = (
    targetPi: { registerTool?(def: ToolDefinition): void },
    names: Set<string>,
    def: ToolDefinition,
  ) => {
    assert.equal(targetPi, pi);
    assert.equal(names.has(def.name), false);
    names.add(def.name);
    targetPi.registerTool?.(def);
  };

  registerWebTool(pi, Type, registeredNames, registerFn);
  return { tool: tools.get('web')!, runWebTool, renderWebResult, propagateOctocodeEnv, getOctocodeHome };
}

afterEach(() => {
  vi.doUnmock('../src/web.js');
  vi.doUnmock('../src/env.js');
  vi.resetModules();
});

test('execute() passes process.env as env dep to runWebTool (explicit env threading)', async () => {
  const { tool, runWebTool } = await loadRegisteredWebTool({ url: 'https://x.com' }, { mockEnv: true });
  await tool.execute('c1', { queries: [{ reasoning: 'fetch url', url: 'https://x.com' }] });
  const deps = (runWebTool.mock.calls[0] as unknown as [unknown, { env?: unknown }])[1];
  assert.strictEqual(deps.env, process.env, 'env dep must be process.env snapshot, not undefined');
});

test('ensureWebEnv calls propagateOctocodeEnv exactly once across multiple execute() calls', async () => {
  const { tool, propagateOctocodeEnv } = await loadRegisteredWebTool({ url: 'https://x.com' }, { mockEnv: true });
  await tool.execute('c1', { queries: [{ reasoning: 'fetch', url: 'https://x.com' }] });
  await tool.execute('c2', { queries: [{ reasoning: 'fetch', url: 'https://x.com' }] });
  await tool.execute('c3', { queries: [{ reasoning: 'search', query: 'test' }] });
  assert.equal(propagateOctocodeEnv.mock.calls.length, 1, 'must be idempotent — called only on first execute()');
});

test('schema only exposes queries at the top level with per-query reasoning', async () => {
  const { tool } = await loadRegisteredWebTool({});
  const schema = tool.parameters as {
    properties?: Record<string, unknown>;
    required?: string[];
    additionalProperties?: boolean;
  };
    // Top-level: operations plus their execution policy.
    assert.deepEqual(Object.keys(schema.properties ?? {}), ['queries', 'queryRunType']);
    assert.deepEqual((schema.properties?.['queryRunType'] as { enum?: string[] })?.enum, ['sequential', 'parallel']);
  assert.ok((schema.required ?? []).includes('queries'));
  // Per-query items include reasoning and web fields
  const queriesSchema = schema.properties?.['queries'] as {
    items?: { properties?: Record<string, unknown>; required?: string[] };
  };
  assert.ok(queriesSchema?.items?.properties?.['reasoning']);
  assert.ok(queriesSchema?.items?.properties?.['url']);
  assert.ok(queriesSchema?.items?.properties?.['query']);
  assert.ok((queriesSchema?.items?.required ?? []).includes('reasoning'));
});

test('registerWebTool registers schema and executes through runWebTool', async () => {
  const { tool, runWebTool, renderWebResult } = await loadRegisteredWebTool({
    title: 'Example',
    url: 'https://example.com',
    truncated: false,
  });

  assert.equal(tool.name, 'web');
  assert.equal(tool.label, 'Web');
  assert.match(tool.description!, /Browse the live web/);

  const ac = new AbortController();
  const result = await tool.execute('call-1', {
    queries: [{ reasoning: 'fetch example', url: 'https://example.com', maxChars: 1000 }],
  }, ac.signal);
  const calls = runWebTool.mock.calls as unknown as Array<[Record<string, unknown>, { signal?: AbortSignal }]>;
  assert.equal(calls[0]![0].url, 'https://example.com');
  assert.equal(calls[0]![1].signal, ac.signal);
  assert.equal(renderWebResult.mock.calls.length, 1);
  assert.deepEqual(result.details, {
    title: 'Example',
    url: 'https://example.com',
    truncated: false,
  });
  assert.match((result.content?.[0] as { text?: string } | undefined)?.text ?? '', /Title: Example/);
});

test('registerWebTool execute throws provider errors so Pi marks the call failed', async () => {
  const { tool } = await loadRegisteredWebTool({ error: 'provider unavailable' });
  await assert.rejects(
    () => tool.execute('call-1', { queries: [{ reasoning: 'search docs', query: 'docs' }] }),
    /provider unavailable/,
  );
});

test('multi-query: two queries execute in order and return batch summary', async () => {
  let callCount = 0;
  const outs = [
    { title: 'First', url: 'https://first.com', truncated: false },
    { results: [{ title: 'Hit', url: 'https://hit.com', snippet: 'x' }] },
  ];
  vi.resetModules();
  const runWebTool = vi.fn(async () => outs[callCount++] ?? {});
  vi.doMock('../src/web.js', () => ({ runWebTool, renderWebResult: (r: unknown) => JSON.stringify(r) }));
  const { registerWebTool } = await import('../src/tools/web-tool.js');
  const tools = new Map<string, ToolDefinition>();
  registerWebTool(
    { registerTool: (d) => tools.set(d.name, d) },
    Type,
    new Set<string>(),
    (_pi, _n, d) => _pi.registerTool?.(d),
  );
  const tool = tools.get('web')!;
  const result = await tool.execute('multi', {
    queries: [
      { reasoning: 'fetch first page', url: 'https://first.com' },
      { reasoning: 'search for hits', query: 'hits' },
    ],
  });
  assert.equal(runWebTool.mock.calls.length, 2);
  assert.match((result.content[0] as { text: string }).text, /2 quer/);
  const details = result.details as { results: Array<{ summary: string }> };
  assert.equal(details.results.length, 2);
  vi.doUnmock('../src/web.js');
  vi.resetModules();
});

test('web renderCall handles url, query, empty args, theming, and truncation', async () => {
  const { tool } = await loadRegisteredWebTool({});

  const urlLine = tool.renderCall!({ queries: [{ reasoning: 'read example', url: 'https://example.com/' }] }, theme).render(120)[0]!;
  assert.match(urlLine, /<toolTitle><b>web<\/b><\/toolTitle>/);
  assert.match(urlLine, /<mdLink>https:\/\/example\.com\//);

  const queryLine = tool.renderCall!({ queries: [{ reasoning: 'search changes', query: 'what changed in vitest coverage' }] }, theme).render(180)[0]!;
  assert.match(queryLine, /<text>search<\/text>/);
  assert.match(queryLine, /<dim>"what changed in vitest coverage"/);

  assert.equal(tool.renderCall!({}, undefined).render(120)[0], '◇ web');

  const narrow = tool.renderCall!({ queries: [{ reasoning: 'read long URL', url: `https://example.com/${'x'.repeat(200)}` }] }, undefined).render(30)[0]!;
  assert.ok(narrow.includes('\u2026'), 'long calls are truncated to terminal width');
});

test('web renderResult covers partial, search stats, page stats, expanded text, and errors', async () => {
  const { tool } = await loadRegisteredWebTool({});

  const partial = tool.renderResult!(textResult('pending'), { isPartial: true }, theme).render(120)[0]!;
  assert.match(partial, /<accent>[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]<\/accent>/);
  assert.match(partial, /<toolTitle>web<\/toolTitle>/);
  assert.match(partial, /<dim>Fetching\u2026<\/dim>/);

  const search = tool.renderResult!(
    textResult('search', { results: [{}, {}] }),
    { expanded: false },
    theme,
  ).render(120)[0]!;
  assert.match(search, /<success>\u2713<\/success>/);
  assert.match(search, /2 results/);

  const page = tool.renderResult!(
    textResult('page', { url: 'https://example.com', page: 3, truncated: true }),
    { expanded: false },
    theme,
  ).render(120)[0]!;
  assert.match(page, /page p3.*more pages available/);

  const expanded = tool.renderResult!(
    textResult(Array.from({ length: 25 }, (_, i) => `line ${i + 1}`).join('\n'), { url: 'https://example.com' }),
    { expanded: true },
    theme,
  ).render(120);
  assert.equal(expanded.length, 22);
  assert.match(expanded.at(-1)!, /5 more lines/);

  const error = tool.renderResult!(
    textResult('bad', {}, true),
    { expanded: false },
    theme,
  ).render(120)[0]!;
  assert.match(error, /<error>\u2717<\/error>/);
});

function textResult(text: string, details: unknown = {}, isError = false): ToolCallResult {
  return {
    isError,
    content: [{ type: 'text', text }],
    details,
  };
}
