import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test, vi } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition } from '../src/types.js';

afterEach(() => {
  vi.doUnmock('@octocodeai/octocode-tools-core/schema');
  vi.doUnmock('@octocodeai/octocode-tools-core/config');
  vi.doUnmock('@octocodeai/octocode-tools-core/direct');
  vi.resetModules();
  vi.restoreAllMocks();
});

test('registerOctocodeTools registers schemas, skips one bad schema, executes success/error paths, and detects collisions', async () => {
  const executeDirectTool = vi.fn(async (name: string) => {
    if (name === 'npmSearch') {
      return { isError: true, content: [{ type: 'text', text: 'package provider failed' }] };
    }
    return {
      structuredContent: { ok: true, tool: name },
      content: [],
    };
  });
  const setRuntimeSurface = vi.fn();
  const invalidateConfigCache = vi.fn();

  vi.doMock('@octocodeai/octocode-tools-core/schema', () => ({
    loadToolContent: vi.fn(async () => ({ loaded: true })),
    getDirectToolCategory: (name: string) => name.startsWith('local') ? 'Local Code' : 'GitHub',
    getDirectToolDescription: (name: string) => (
      name === 'npmSearch'
        ? 'Package | Search npm packages with precise names.'
        : `${name} | Full description for ${name}.`
    ),
    formatDirectToolSchemaText: (name: string) => {
      if (name === 'ghCloneRepo') throw new Error('bad schema');
      return JSON.stringify({
        type: 'object',
        required: ['queries', 'reasoning', 'third', 'fourth', 'fifth'],
        properties: { queries: { type: 'array' } },
      });
    },
  }));
  vi.doMock('@octocodeai/octocode-tools-core/config', () => ({
    setRuntimeSurface,
    invalidateConfigCache,
  }));
  vi.doMock('@octocodeai/octocode-tools-core/direct', () => ({
    executeDirectTool,
  }));

  const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
  const { registerOctocodeTools, registerUniqueTool } = await import('../src/tools/octocode-tools.js');

  const tools = new Map<string, ToolDefinition>();
  await registerOctocodeTools(
    { registerTool: (def) => tools.set(def.name, def) },
    Type,
    new Set<string>(),
  );

  assert.equal(tools.has('localSearchCode'), true);
  assert.equal(tools.has('ghCloneRepo'), false, 'bad schema is skipped without taking down the rest');
  assert.ok(consoleError.mock.calls.some((call) => String(call[0]).includes('ghCloneRepo')));
  assert.match(tools.get('localSearchCode')!.promptGuidelines!.join('\n'), /mode:"discovery"/);
  assert.match(tools.get('localGetFileContent')!.promptGuidelines!.join('\n'), /absolute/);
  assert.match(tools.get('npmSearch')!.promptSnippet!, /Search npm packages/);
  assert.match(tools.get('localViewStructure')!.promptSnippet!, /Required: queries, reasoning, third, fourth/);

  const success = await tools.get('localViewStructure')!.execute('call-1', { queries: [] }, undefined, undefined, { cwd: process.cwd() });
  assert.deepEqual(success.details, { ok: true, tool: 'localViewStructure' });
  assert.match(success.content[0]!.text, /localViewStructure/);
  assert.equal(setRuntimeSurface.mock.calls.at(-1)?.[0], 'cli');
  assert.equal(invalidateConfigCache.mock.calls.length > 0, true);

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-tools-read-'));
  const readable = path.join(tmp, 'file.txt');
  fs.writeFileSync(readable, 'hello\n', 'utf8');
  try {
    const readResult = await tools.get('localGetFileContent')!.execute(
      'call-read',
      { queries: [{ path: readable }, {}, { path: '   ' }] },
      undefined,
      undefined,
      { cwd: tmp },
    );
    assert.deepEqual(readResult.details, { ok: true, tool: 'localGetFileContent' });
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }

  const aborted = new AbortController();
  aborted.abort();
  await assert.rejects(
    () => tools.get('localViewStructure')!.execute('call-2', { queries: [] }, aborted.signal),
    /cancelled before it started/,
  );

  await assert.rejects(
    () => tools.get('npmSearch')!.execute('call-3', { queries: [] }),
    /package provider failed/,
  );

  const names = new Set<string>();
  registerUniqueTool({}, names, {
    name: 'duplicate',
    label: 'Duplicate',
    description: 'Duplicate test tool',
    parameters: Type.Object({}),
    execute: async () => ({ content: [{ type: 'text', text: 'ok' }] }),
  });
  assert.throws(
    () => registerUniqueTool({}, names, {
      name: 'duplicate',
      label: 'Duplicate',
      description: 'Duplicate test tool',
      parameters: Type.Object({}),
      execute: async () => ({ content: [{ type: 'text', text: 'ok' }] }),
    }),
    /tool name collision/,
  );
});
