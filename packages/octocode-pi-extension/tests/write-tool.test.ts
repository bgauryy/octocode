/**
 * Public file-tool write behavior tests.
 *
 * Tests validate the path-guard, param validation, directory creation,
 * overwrite semantics, and read-state recording.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test, beforeEach, afterEach } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition } from '../src/types.js';
import { registerFileTool } from '../src/tools/file-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import {
  checkReadState,
  clearReadStatesForTests,
} from '../src/tools/file-state.js';

let tmpDir: string;
let fileTool: ToolDefinition;

beforeEach(() => {
  tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'write-tool-test-'));
  clearReadStatesForTests();

  const tools = new Map<string, ToolDefinition>();
  registerFileTool({ registerTool: (def) => tools.set(def.name, def) }, Type, new Set<string>(), registerUniqueTool);
  fileTool = tools.get('file')!;
  assert.ok(fileTool, 'public file tool must be registered');
});

afterEach(() => {
  fs.rmSync(tmpDir, { recursive: true, force: true });
  clearReadStatesForTests();
});

function run(
  params: Record<string, unknown>,
  cwd = tmpDir,
  signal?: AbortSignal,
): ReturnType<ToolDefinition['execute']> {
  const withReasoning = Object.hasOwn(params, 'reasoning')
    ? params
    : { ...params, reasoning: 'test write operation' };
  const envelope = { queries: [{ type: 'write', ...withReasoning }] };
  const prepared = fileTool.prepareArguments?.(envelope) as Record<string, unknown> | undefined;
  return fileTool.execute('call-1', prepared ?? envelope, signal, undefined, { cwd });
}

// ─── Registration ─────────────────────────────────────────────────────────────

test('public file tool exposes guarded write queries', () => {
  assert.equal(fileTool.name, 'file');
  assert.match(fileTool.description ?? '', /guarded/);
  const schema = fileTool.parameters as {
    properties?: Record<string, unknown>;
  };
  assert.deepEqual(Object.keys(schema.properties ?? {}), ['queries', 'queryRunType']);
  assert.deepEqual((schema.properties?.['queryRunType'] as { enum?: string[] })?.enum, ['sequential']);
  const queries = schema.properties?.['queries'] as { items?: { properties?: Record<string, unknown> } };
  assert.ok(queries.items?.properties?.['path'], 'query must have a path property');
  assert.ok(queries.items?.properties?.['content'], 'query must have a content property');
  assert.ok(queries.items?.properties?.['reasoning'], 'query must have a reasoning property');
});

// ─── Successful writes ────────────────────────────────────────────────────────

test('creates a new file and returns byte count', async () => {
  const content = 'hello world';
  const result = await run({ path: 'new-file.txt', content });
  assert.equal(result.isError, undefined);
  assert.match((result.content[0] as { text: string }).text, /Successfully wrote/);
  assert.match((result.content[0] as { text: string }).text, new RegExp(`${content.length} bytes`));
  assert.equal(fs.readFileSync(path.join(tmpDir, 'new-file.txt'), 'utf8'), content);
});

test('creates nested directories when they do not exist', async () => {
  const relPath = 'nested/deep/file.ts';
  await run({ path: relPath, content: 'export {}' });
  assert.equal(
    fs.readFileSync(path.join(tmpDir, relPath), 'utf8'),
    'export {}',
  );
});

test('overwrites an existing file', async () => {
  const target = path.join(tmpDir, 'overwrite.txt');
  fs.writeFileSync(target, 'original');
  await run({ path: target, content: 'replaced' });
  assert.equal(fs.readFileSync(target, 'utf8'), 'replaced');
});

test('rejects file_path instead of path', async () => {
  await assert.rejects(
    () => run({ file_path: 'unsupported.txt', content: 'x' }),
    /requires a non-empty path/,
  );
});

test('writes empty content without error', async () => {
  const result = await run({ path: 'empty.txt', content: '' });
  assert.equal(result.isError, undefined);
  assert.equal(fs.readFileSync(path.join(tmpDir, 'empty.txt'), 'utf8'), '');
});

// ─── details field ────────────────────────────────────────────────────────────

test('result.details contains path, absolutePath, and bytes', async () => {
  const content = 'abc';
  const result = await run({ path: 'details-test.txt', content });
  const details = result.details as { path?: string; absolutePath?: string; bytes?: number };
  assert.equal(details.path, 'details-test.txt');
  assert.equal(details.absolutePath, path.join(tmpDir, 'details-test.txt'));
  assert.equal(details.bytes, 3);
});

// ─── Read-state recording ─────────────────────────────────────────────────────

test('records read state after writing so a subsequent edit does not see the file as stale', async () => {
  const filePath = path.join(tmpDir, 'after-write.txt');
  await run({ path: filePath, content: 'written content' });
  // checkReadState should return "fresh" — write tool must have recorded state
  const check = await checkReadState(filePath, false);
  assert.equal(check.state, 'fresh');
});

test('aborts before writing without creating the target file', async () => {
  const filePath = path.join(tmpDir, 'abort-before-write.txt');
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(
    () => run({ path: filePath, content: 'not written' }, tmpDir, controller.signal),
    /query batch aborted/,
  );
  assert.equal(fs.existsSync(filePath), false);
});

// ─── Param validation ────────────────────────────────────────────────────────

test('rejects when path is missing', async () => {
  await assert.rejects(() => run({ content: 'no path' }), /requires a non-empty path/);
});

test('rejects when path is empty string', async () => {
  await assert.rejects(() => run({ path: '', content: 'empty path' }), /requires a non-empty path/);
});

test('rejects when content is not a string', async () => {
  await assert.rejects(
    () => run({ path: 'file.txt', content: 42 }),
    /content must be a string/,
  );
});

test('rejects when reasoning is missing', async () => {
  await assert.rejects(
    () => fileTool.execute('call-missing-reasoning', { queries: [{ type: 'write', path: 'file.txt', content: 'hi' }] }, undefined, undefined, { cwd: tmpDir }),
    /requires non-empty reasoning/,
  );
});

// ─── Path guard ───────────────────────────────────────────────────────────────

test('blocks writes to a path outside all allowed roots', async () => {
  const prev = process.env['ALLOWED_PATHS'];
  try {
    delete process.env['ALLOWED_PATHS'];
    await assert.rejects(
      () => run({ path: '/usr/local/evil.txt', content: 'pwned' }),
      /outside the allowed roots/,
    );
  } finally {
    if (prev === undefined) delete process.env['ALLOWED_PATHS'];
    else process.env['ALLOWED_PATHS'] = prev;
  }
});

test('prepareArguments does not convert flat input or path aliases', () => {
  assert.ok(fileTool.prepareArguments, 'prepareArguments must be defined');
  const input = { file_path: 'x.txt', content: 'hi' };
  assert.deepEqual(fileTool.prepareArguments!(input), input);
});

test('prepareArguments fills reasoning only inside queries[]', () => {
  const input = { queries: [{ type: 'write', path: 'x.txt', content: 'hi' }] };
  const result = fileTool.prepareArguments!(input) as { queries: Array<Record<string, unknown>> };
  const query = (result['queries'] as Array<Record<string, unknown>>)[0]!;
  assert.equal(query['path'], 'x.txt');
  assert.equal(query['reasoning'], 'file operation');
});

// ─── renderCall ──────────────────────────────────────────────────────

const theme = {
  bold: (t: string) => `**${t}**`,
  fg: (_color: string, t: string) => t,
};

test('renderCall identifies the public file operation, path, and reasoning', () => {
  assert.ok(fileTool.renderCall, 'renderCall must be defined');
  const renderer = fileTool.renderCall!({
    queries: [{ type: 'write', path: 'src/foo.ts', content: 'line1\nline2\nline3', reasoning: 'create fixture file' }],
  }, theme);
  assert.ok(renderer, 'renderCall must return a renderer');
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.ok(lines.join('\n').includes('file (Octocode)'), 'label must identify the public tool');
  assert.ok(lines.join('\n').includes('src/foo.ts'), 'label must contain file path');
  assert.ok(lines.join('\n').includes('write'), 'label must show the operation');
  assert.ok(lines.join('\n').includes('create fixture file'), 'label must show reasoning');
});

test('renderCall handles missing path gracefully', () => {
  assert.ok(fileTool.renderCall);
  const renderer = fileTool.renderCall!({ queries: [{ type: 'write', reasoning: 'test missing path', content: 'hello' }] });
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.ok(lines.join('\n').includes('missing path'), 'must note missing path');
});

test('renderCall with no theme still renders', () => {
  const renderer = fileTool.renderCall!({ queries: [{ type: 'write', reasoning: 'write output', path: 'out.txt', content: 'hi' }] });
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.ok(lines.length > 0);
});

// ─── renderResult ────────────────────────────────────────────────────

test('renderResult for isPartial=true renders a progress indicator', () => {
  assert.ok(fileTool.renderResult);
  const result: import('../src/types.js').ToolCallResult = { content: [], isError: undefined };
  const renderer = fileTool.renderResult!(result, { isPartial: true }, theme);
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.ok(lines.join('\n').includes('editing'), 'partial result must show mutation progress');
  assert.ok(lines.join('\n').includes('file (Octocode)'), 'partial result must identify the public tool');
});

test('renderResult for successful write renders the path and size', () => {
  const result: import('../src/types.js').ToolCallResult = {
    content: [{ type: 'text' as const, text: 'ok' }], isError: undefined, details: { operation: 'write', path: 'src/a.ts', bytes: 42 },
  };
  const renderer = fileTool.renderResult!(result, {}, theme);
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.match(lines.join('\n'), /✓/);
  assert.match(lines.join('\n'), /file \(Octocode\)/);
  assert.match(lines.join('\n'), /src\/a\.ts/);
  assert.match(lines.join('\n'), /42 bytes/);
});

test('renderResult for error renders the error text', () => {
  const result: import('../src/types.js').ToolCallResult = {
    content: [{ type: 'text' as const, text: 'permission denied' }],
    isError: true,
  };
  const renderer = fileTool.renderResult!(result, {}, theme);
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.ok(lines.join('\n').includes('permission denied'));
});

test('renderResult for error with no content text retains the public tool identity', () => {
  const result: import('../src/types.js').ToolCallResult = { content: [], isError: true };
  const renderer = fileTool.renderResult!(result, {});
  const lines = (renderer as { render(width: number): string[] }).render(80);
  assert.ok(lines.join('\n').includes('file (Octocode)'));
});
