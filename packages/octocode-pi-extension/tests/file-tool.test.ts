import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, test } from 'vitest';
import { Type } from 'typebox';
import { registerFileTool } from '../src/tools/file-tool.js';

type Tool = {
  name: string;
  parameters: unknown;
  execute: (id: string, params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: unknown, ctx?: { cwd?: string }) => Promise<{ isError?: boolean; content: Array<{ type: string; text?: string }>; details?: unknown }>;
};

let cwd: string;
let tool: Tool;

beforeEach(() => {
  cwd = mkdtempSync(join(tmpdir(), 'octocode-file-tool-'));
  let captured: Tool | undefined;
  registerFileTool(
    {},
    Type,
    new Set(),
    (_pi, names, definition) => {
      names.add(definition.name);
      captured = definition as Tool;
    },
  );
  if (!captured) throw new Error('file tool was not registered');
  tool = captured;
});

afterEach(() => rmSync(cwd, { recursive: true, force: true }));

const call = (...queries: Array<Record<string, unknown>>) => tool.execute('file-test', { queries }, undefined, undefined, { cwd });

test('registers one discriminated file mutation contract', () => {
  assert.equal(tool.name, 'file');
  const schema = tool.parameters as {
    properties: { queries: { items: { properties: Record<string, unknown>; oneOf?: Array<{ title?: string }> } } };
  };
    assert.deepEqual(Object.keys(schema.properties), ['queries', 'queryRunType']);
  assert.ok(schema.properties.queries.items.properties['type']);
  assert.deepEqual(schema.properties.queries.items.oneOf?.map((item) => item.title), ['edit', 'write', 'delete']);
});

test('writes, edits, and deletes through one tool', async () => {
  const written = await call({ type: 'write', reasoning: 'create fixture', path: 'note.txt', content: 'hello world\n' });
  assert.equal(written.isError, undefined);
  assert.equal(readFileSync(join(cwd, 'note.txt'), 'utf8'), 'hello world\n');

  const edited = await call({
    type: 'edit',
    reasoning: 'rename greeting',
    path: 'note.txt',
    edits: [{ oldText: 'hello', newText: 'goodbye' }],
  });
  assert.equal(edited.isError, undefined);
  assert.equal(readFileSync(join(cwd, 'note.txt'), 'utf8'), 'goodbye world\n');

  const deleted = await call({ type: 'delete', reasoning: 'remove fixture', path: 'note.txt' });
  assert.equal(deleted.isError, undefined);
  assert.equal(existsSync(join(cwd, 'note.txt')), false);
});

test('preflights the complete mixed batch before any mutation', async () => {
  mkdirSync(join(cwd, 'directory'));
  await assert.rejects(
    call(
      { type: 'write', reasoning: 'would create first', path: 'first.txt', content: 'first' },
      { type: 'delete', reasoning: 'invalid directory delete', path: 'directory' },
    ),
    /delete supports files and symbolic links, not directories/i,
  );
  assert.equal(existsSync(join(cwd, 'first.txt')), false);
});

test('rejects operation-specific missing and extra fields', async () => {
  await assert.rejects(call({ type: 'write', reasoning: 'missing content', path: 'a.txt' }), /content/i);
  await assert.rejects(call({ type: 'delete', reasoning: 'no write payload', path: 'a.txt', content: 'x' }), /does not accept content/i);
  await assert.rejects(call({ type: 'edit', reasoning: 'missing edits', path: 'a.txt' }), /edits/i);
});

test('delete fails closed for missing paths and paths outside allowed roots', async () => {
  await assert.rejects(call({ type: 'delete', reasoning: 'must exist', path: 'missing.txt' }), /ENOENT|does not exist/i);
  await assert.rejects(call({ type: 'delete', reasoning: 'outside workspace', path: '/etc/hosts' }), /outside the allowed roots/i);
});

test('delete removes a symbolic link without following it', async () => {
  const target = join(cwd, 'link-target.txt');
  const link = join(cwd, 'my-link.txt');
  writeFileSync(target, 'target content', 'utf8');
  const { symlinkSync } = await import('node:fs');
  symlinkSync(target, link);
  assert.ok(existsSync(link), 'symlink must exist before delete');
  const result = await call({ type: 'delete', reasoning: 'remove link', path: 'my-link.txt' });
  assert.equal(result.isError, undefined);
  // Link gone, target still exists
  assert.equal(existsSync(link), false, 'symlink should be removed');
  assert.ok(existsSync(target), 'link target must survive link deletion');
});

test('two write queries targeting the same path in one batch are rejected at preflight', async () => {
  await assert.rejects(
    () => tool.execute('id', {
      queryRunType: 'sequential',
      queries: [
        { type: 'write', reasoning: 'first', path: 'dup.txt', content: 'a' },
        { type: 'write', reasoning: 'second', path: 'dup.txt', content: 'b' },
      ],
    }, undefined, undefined, { cwd }),
    /duplicate|same path|already|conflict/i,
    'duplicate path in same batch should be rejected before any write',
  );
});

test('edit uses one query-level reason for every replacement', async () => {
  writeFileSync(join(cwd, 'multi.txt'), 'a b\n');
  const result = await call({
    type: 'edit',
    reasoning: 'rename both tokens',
    path: 'multi.txt',
    edits: [
      { oldText: 'a', newText: 'x' },
      { oldText: 'b', newText: 'y' },
    ],
  });
  assert.equal(result.isError, undefined);
  assert.equal(readFileSync(join(cwd, 'multi.txt'), 'utf8'), 'x y\n');
  assert.match(result.content[0]?.text ?? '', /rename both tokens/);
});
