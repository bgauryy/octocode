import assert from 'node:assert/strict';
import { afterEach, test, vi } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition, ToolCallResult, PiTheme } from '../src/types.js';

const theme: PiTheme = {
  bold: (text: string) => `<b>${text}</b>`,
  fg: (color: string, text: string) => `<${color}>${text}</${color}>`,
};

async function loadRegisteredWebTool(out: Record<string, unknown>) {
  vi.resetModules();
  const runWebTool = vi.fn(async () => out);
  const renderWebResult = vi.fn((result: unknown) => {
    const r = result as { title?: string; url?: string };
    return [`Title: ${r.title ?? 'untitled'}`, `URL: ${r.url ?? 'n/a'}`].join('\n');
  });
  vi.doMock('../src/web.js', () => ({ runWebTool, renderWebResult }));

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
  return { tool: tools.get('web')!, runWebTool, renderWebResult };
}

afterEach(() => {
  vi.doUnmock('../src/web.js');
  vi.resetModules();
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
  assert.ok((tool.parameters as { properties?: Record<string, unknown> }).properties?.['url']);
  assert.ok((tool.parameters as { properties?: Record<string, unknown> }).properties?.['query']);

  const ac = new AbortController();
  const result = await tool.execute('call-1', { url: 'https://example.com', maxChars: 1000 }, ac.signal);
  const calls = runWebTool.mock.calls as unknown as Array<[Record<string, unknown>, { signal?: AbortSignal }]>;
  assert.equal(calls[0]![0].url, 'https://example.com');
  assert.equal(calls[0]![1].signal, ac.signal);
  assert.equal(renderWebResult.mock.calls.length, 1);
  assert.deepEqual(result.details, {
    title: 'Example',
    url: 'https://example.com',
    truncated: false,
  });
  assert.match(result.content?.[0]?.text ?? '', /Title: Example/);
});

test('registerWebTool execute throws provider errors so Pi marks the call failed', async () => {
  const { tool } = await loadRegisteredWebTool({ error: 'provider unavailable' });
  await assert.rejects(
    () => tool.execute('call-1', { query: 'docs' }),
    /provider unavailable/,
  );
});

test('web renderCall handles url, query, empty args, theming, and truncation', async () => {
  const { tool } = await loadRegisteredWebTool({});

  const urlLine = tool.renderCall!({ url: 'https://example.com/' }, theme).render(120)[0]!;
  assert.match(urlLine, /<toolTitle><b>web<\/b><\/toolTitle>/);
  assert.match(urlLine, /<accent>https:\/\/example\.com\//);

  const queryLine = tool.renderCall!({ query: 'what changed in vitest coverage' }, theme).render(120)[0]!;
  assert.match(queryLine, /<dim>"what changed in vitest coverage"/);

  assert.equal(tool.renderCall!({}, undefined).render(120)[0], 'web');

  const narrow = tool.renderCall!({ url: `https://example.com/${'x'.repeat(200)}` }, undefined).render(30)[0]!;
  assert.ok(narrow.includes('…'), 'long calls are truncated to terminal width');
});

test('web renderResult covers partial, search stats, page stats, expanded text, and errors', async () => {
  const { tool } = await loadRegisteredWebTool({});

  assert.equal(
    tool.renderResult!(textResult('pending'), { isPartial: true }, theme).render(80)[0],
    '<warning>Fetching…</warning>',
  );

  const search = tool.renderResult!(
    textResult('search', { results: [{}, {}] }),
    { expanded: false },
    theme,
  ).render(120)[0]!;
  assert.match(search, /<success>✓<\/success>/);
  assert.match(search, /2 results/);
  assert.match(search, /expand for full output/);

  const page = tool.renderResult!(
    textResult('page', { url: 'https://example.com', page: 3, truncated: true }),
    { expanded: false },
    theme,
  ).render(120)[0]!;
  assert.match(page, /page p3 \(more pages available\)/);

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
  assert.match(error, /<error>✗<\/error>/);
});

function textResult(text: string, details: unknown = {}, isError = false): ToolCallResult {
  return {
    isError,
    content: [{ type: 'text', text }],
    details,
  };
}
