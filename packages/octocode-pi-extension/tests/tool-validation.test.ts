import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { Type } from 'typebox';
import { validateToolArguments } from '@earendil-works/pi-ai';
import type { PiContext, ToolDefinition } from '../src/types.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { registerFileTool } from '../src/tools/file-tool.js';
import { registerPlanTool } from '../src/tools/plan-tool.js';

function capture(register: typeof registerFileTool | typeof registerPlanTool): ToolDefinition {
  let tool: ToolDefinition | undefined;
  const pi = { registerTool: (definition: ToolDefinition) => { tool = definition; } };
  register(pi, Type, new Set<string>(), registerUniqueTool);
  if (!tool) throw new Error('tool was not registered');
  return tool;
}

function prepareAndValidate(tool: ToolDefinition, argumentsValue: Record<string, unknown>): Record<string, unknown> {
  const prepared = tool.prepareArguments?.(argumentsValue) ?? argumentsValue;
  return validateToolArguments(tool as never, {
    id: 'validation-test', name: tool.name, arguments: prepared,
  } as never) as Record<string, unknown>;
}

const temporaryDirectories: string[] = [];
afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) fs.rmSync(directory, { recursive: true, force: true });
});

test('plan rejects flat calls and accepts queries[] when reasoning is omitted', () => {
  const plan = capture(registerPlanTool);
  assert.throws(() => prepareAndValidate(plan, { action: 'show' }), /queries/i);
  const envelope = prepareAndValidate(plan, { queries: [{ action: 'show' }] });
  assert.equal((envelope.queries as Array<Record<string, unknown>>)[0]?.reasoning, 'plan operation');
});

test('every plan action branch passes Pi validation through the shared query boundary', () => {
  const plan = capture(registerPlanTool);
  const actions: Array<Record<string, unknown>> = [
    { action: 'set', steps: ['inspect'] },
    { action: 'propose', steps: ['inspect'] },
    { action: 'clarify', questions: [{ prompt: 'Which scope?' }] },
    { action: 'add', text: 'verify' },
    { action: 'start' },
    { action: 'complete' },
    { action: 'remove' },
    { action: 'clear' },
    { action: 'show' },
  ];
  for (const query of actions) {
    assert.doesNotThrow(() => prepareAndValidate(plan, { queries: [query] }), String(query.action));
  }
});

test('file rejects flat calls and accepts queries[] when reasoning is omitted', () => {
  const file = capture(registerFileTool);
  assert.throws(
    () => prepareAndValidate(file, { type: 'write', path: 'note.txt', content: 'hello' }),
    /queries/i,
  );
  const envelope = prepareAndValidate(file, { queries: [{ type: 'delete', path: 'note.txt' }] });
  assert.equal((envelope.queries as Array<Record<string, unknown>>)[0]?.reasoning, 'file operation');
});

test('shared registration preserves explicit reasoning for every query', () => {
  const file = capture(registerFileTool);
  const prepared = prepareAndValidate(file, {
    queries: [
      { reasoning: 'create first file', type: 'write', path: 'a.txt', content: 'a' },
      { reasoning: 'create second file', type: 'write', path: 'b.txt', content: 'b' },
    ],
  });
  assert.deepEqual(
    (prepared.queries as Array<Record<string, unknown>>).map((query) => query.reasoning),
    ['create first file', 'create second file'],
  );
});

test('registered query tools execute every prepared file query in order', async () => {
  const file = capture(registerFileTool);
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-tool-validation-'));
  temporaryDirectories.push(cwd);
  const prepared = prepareAndValidate(file, {
    queries: [
      { type: 'write', path: 'a.txt', content: 'a' },
      { type: 'write', path: 'b.txt', content: 'b' },
    ],
  });
  const result = await file.execute('batch', prepared, undefined, undefined, { cwd } as PiContext);
  assert.match((result.content[0] as { text: string }).text, /2 queries succeeded/);
  assert.equal(fs.readFileSync(path.join(cwd, 'a.txt'), 'utf8'), 'a');
  assert.equal(fs.readFileSync(path.join(cwd, 'b.txt'), 'utf8'), 'b');
});
