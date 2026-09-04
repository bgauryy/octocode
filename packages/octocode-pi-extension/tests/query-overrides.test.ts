import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { Type } from 'typebox';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
import { registerBashTool } from '../src/tools/bash-tool.js';
import { registerWriteTool } from '../src/tools/write-tool.js';
import type { ToolCallResult, ToolDefinition } from '../src/types.js';

const typeBuilder = Type as unknown as (typeof import('typebox'))['Type'];

let restoreProcessGuard: () => void;
beforeAll(() => {
  restoreProcessGuard = allowLocalFixtureProcesses();
});
afterAll(() => restoreProcessGuard());

function register(registerTool: typeof registerBashTool): ToolDefinition {
  let captured: ToolDefinition | undefined;
  const pi = { registerTool: (def: ToolDefinition) => { captured = def; } };
  const registerFn = (
    target: { registerTool?: (def: ToolDefinition) => void },
    _names: Set<string>,
    def: ToolDefinition,
  ): void => target.registerTool?.(def);
  registerTool(pi, typeBuilder, new Set<string>(), registerFn);
  return captured!;
}

function expectEnvelope(tool: ToolDefinition): void {
  const schema = tool.parameters as {
    properties?: { queries?: { minItems?: number; items?: { properties?: Record<string, unknown>; required?: string[] } } };
    required?: string[];
  };
      expect(Object.keys(schema.properties ?? {})).toEqual(['queries', 'queryRunType']);
  expect(schema.required).toContain('queries');
  expect(schema.properties?.queries?.minItems).toBe(1);
  expect(schema.properties?.queries?.items?.properties).toHaveProperty('reasoning');
  expect(schema.properties?.queries?.items?.required).toContain('reasoning');
}

describe('built-in override query envelopes', () => {
  const dirs: string[] = [];
  afterEach(() => {
    for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true });
  });

  it('bash exposes only queries and executes ordered commands', async () => {
    const tool = register(registerBashTool);
    expectEnvelope(tool);

    const result = await tool.execute('bash-batch', {
      queries: [
        { reasoning: 'print first marker', command: "printf 'one'" },
        { reasoning: 'print second marker', command: "printf 'two'" },
      ],
    });
    expect(result.isError).toBeFalsy();
    expect((result.content[0] as { text: string }).text).toMatch(/2 queries succeeded/);
    expect((result.details as { results: unknown[] }).results).toHaveLength(2);
  });

  it('bash preserves the original detail shape for one query', async () => {
    const tool = register(registerBashTool);
    const result = await tool.execute('bash-one', {
      queries: [{ reasoning: 'print one marker', command: "printf 'one'" }],
    });
    expect((result.content[0] as { text: string }).text).toBe('one');
    expect(result.details).toMatchObject({ code: 0, totalChars: 3, stdoutChars: 3, stderrChars: 0 });
    expect((result.details as { stdout?: string }).stdout).toBeUndefined();
  });

  it('write exposes only queries, preflights, and writes multiple files in order', async () => {
    const tool = register(registerWriteTool);
    expectEnvelope(tool);
    const cwd = mkdtempSync(join(tmpdir(), 'query-write-'));
    dirs.push(cwd);

    const result = await tool.execute(
      'write-batch',
      {
        queries: [
          { reasoning: 'create first fixture', path: 'a.txt', content: 'a' },
          { reasoning: 'create second fixture', path: 'b.txt', content: 'b' },
        ],
      },
      undefined,
      undefined,
      { cwd },
    );

    expect((result.content[0] as { text: string }).text).toMatch(/2 queries succeeded/);
    expect(readFileSync(join(cwd, 'a.txt'), 'utf8')).toBe('a');
    expect(readFileSync(join(cwd, 'b.txt'), 'utf8')).toBe('b');
  });

  it('write preflight prevents earlier writes when a later path is invalid', async () => {
    const tool = register(registerWriteTool);
    const cwd = mkdtempSync(join(tmpdir(), 'query-write-preflight-'));
    dirs.push(cwd);

    await expect(tool.execute(
      'write-invalid',
      {
        queries: [
          { reasoning: 'would create fixture', path: 'safe.txt', content: 'safe' },
          { reasoning: 'invalid outside write', path: '/private/forbidden.txt', content: 'bad' },
        ],
      },
      undefined,
      undefined,
      { cwd },
    )).rejects.toThrow(/queries\[1\] failed preflight|outside allowed/i);

    expect(() => readFileSync(join(cwd, 'safe.txt'), 'utf8')).toThrow();
  });

  it('write preserves the original detail shape for one query', async () => {
    const tool = register(registerWriteTool);
    const cwd = mkdtempSync(join(tmpdir(), 'query-write-one-'));
    dirs.push(cwd);
    const result: ToolCallResult = await tool.execute(
      'write-one',
      { queries: [{ reasoning: 'create one fixture', path: 'one.txt', content: 'one' }] },
      undefined,
      undefined,
      { cwd },
    );
    expect(result.details).toMatchObject({ path: 'one.txt', bytes: 3 });
  });
});
