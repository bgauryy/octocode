import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { Type } from 'typebox';
import { registerAwarenessCoordinationTools } from '../src/tools/awareness-coordination-tools.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import type { ToolDefinition, ToolCallResult } from '../src/types.js';

function register(): Map<string, ToolDefinition> {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
  const registerFn = (
    p: { registerTool?: (d: ToolDefinition) => void },
    names: Set<string>,
    def: ToolDefinition,
  ): void => { registerUniqueTool(p, names, def); };
  registerAwarenessCoordinationTools(
    pi,
    Type as unknown as (typeof import('typebox'))['Type'],
    new Set<string>(),
    registerFn as never,
  );
  return tools;
}

describe('first-class Awareness coordination tools', () => {
  let ws: string;
  let tools: Map<string, ToolDefinition>;
  const prevAgent = process.env['OCTOCODE_AGENT_ID'];

  const run = async (name: string, args: Record<string, unknown>): Promise<{ result: ToolCallResult; text: string; json: unknown }> => {
    const tool = tools.get(name)!;
    expect(tool, `tool ${name} registered`).toBeTruthy();
    const result = await (tool.execute as (id: string, raw: Record<string, unknown>, s: unknown, u: unknown, ctx: unknown) => Promise<ToolCallResult>)(
      't', args, undefined, undefined, { cwd: ws },
    );
    const text = (result.content?.[0] as { text?: string })?.text ?? '';
    const details = (result as unknown as { details?: { result?: unknown } }).details;
    return { result, text, json: details?.result };
  };

  const one = (query: Record<string, unknown>): Record<string, unknown> => ({
    queries: [{ reasoning: 'test coordination operation', ...query }],
  });

  beforeEach(() => {
    ws = mkdtempSync(join(tmpdir(), 'aw-coord-'));
    process.env['OCTOCODE_AGENT_ID'] = 'pi:test-a';
    tools = register();
  });
  afterEach(() => {
    if (prevAgent === undefined) delete process.env['OCTOCODE_AGENT_ID'];
    else process.env['OCTOCODE_AGENT_ID'] = prevAgent;
    rmSync(ws, { recursive: true, force: true });
  });

  it('registers only actionable lock and message coordination', () => {
    expect([...tools.keys()].sort()).toEqual(['lock', 'message']);

    for (const tool of tools.values()) {
      const schema = tool.parameters as {
        properties?: { queries?: { minItems?: number; items?: { properties?: Record<string, unknown>; required?: string[] } } };
        required?: string[];
      };
        expect(Object.keys(schema.properties ?? {})).toEqual(['queries', 'queryRunType']);
      expect(schema.required).toContain('queries');
      expect(schema.properties?.queries?.minItems).toBe(1);
      expect(schema.properties?.queries?.items?.properties).toHaveProperty('reasoning');
      expect(schema.properties?.queries?.items?.properties).not.toHaveProperty('reason');
      expect(schema.properties?.queries?.items?.required).toContain('reasoning');
    }

    const lockSchema = tools.get('lock')!.parameters as {
      properties?: { queries?: { items?: { oneOf?: Array<{ title?: string }> } } };
    };
    expect(lockSchema.properties?.queries?.items?.oneOf?.map((variant) => variant.title)).toEqual([
      'acquire',
      'release',
      'wait',
    ]);
  });

  it('lock executes acquire, wait, and release', async () => {
    writeFileSync(join(ws, 'locked.ts'), 'x');

    const acquired = await run('lock', one({ action: 'acquire', file: 'locked.ts', ttlSeconds: 60 }));
    expect(acquired.result.isError).toBeFalsy();
    expect(acquired.text).toMatch(/Locked locked\.ts/);

    process.env['OCTOCODE_AGENT_ID'] = 'pi:peer';
    const waited = await run('lock', one({ action: 'wait', file: 'locked.ts', waitMs: 10 }));
    expect(waited.result.isError).toBeFalsy();
    expect(waited.text).toMatch(/still held by pi:test-a/);

    process.env['OCTOCODE_AGENT_ID'] = 'pi:test-a';
    const released = await run('lock', one({ action: 'release', file: 'locked.ts' }));
    expect(released.result.isError).toBeFalsy();
    expect(released.text).toMatch(/Released lock/);
  });


  it('message: broadcast/send → peer inbox', async () => {
    await run('message', one({ action: 'send', to: 'pi:peer', text: 'watch a.ts' }));
    process.env['OCTOCODE_AGENT_ID'] = 'pi:peer';
    const inbox = await run('message', one({ action: 'read' }));
    expect(inbox.text).toMatch(/1 message/);    // 'read' lists + marks messages as read
  });

  it('message batch sequences send and read queries correctly', async () => {
    const message = tools.get('message')!;
    const batch = {
      queries: [
        { reasoning: 'send first', action: 'send', to: 'pi:peer', text: 'first' },
        { reasoning: 'send second', action: 'send', to: 'pi:peer', text: 'second' },
        { reasoning: 'read peers inbox', action: 'read' },
      ],
    };
    const result = await (message.execute as (
      id: string,
      raw: Record<string, unknown>,
      signal: unknown,
      update: unknown,
      ctx: unknown,
    ) => Promise<ToolCallResult>)('batch', batch, undefined, undefined, { cwd: ws });
    const text = (result.content ?? []).map((c: unknown) => (c as { text?: string }).text ?? '').join('\n');
    expect(result.isError).toBeFalsy();
    expect(text).toContain('Sent message');
    expect(text).toContain('message(s) from peers');
  });

  it('keeps full model results while rendering compact tool-specific views', async () => {
    const largeText = `start-${'x'.repeat(25_000)}-end`;
    const sent = await run('message', one({ action: 'send', to: 'pi:peer', text: largeText }));
    expect(sent.text).toContain(largeText);
    expect(sent.text.length).toBeGreaterThan(25_000);

    const message = tools.get('message')!;
    const call = message.renderCall?.({
      queries: [
        { reasoning: 'notify', action: 'send', to: 'pi:peer', text: 'one' },
        { reasoning: 'inspect', action: 'read' },
      ],
    }, undefined)?.render(100).join('\n') ?? '';
    expect(call).toMatch(/message/i);
    expect(call).toContain('send');
    expect(call).toContain('read');

    const rendered = message.renderResult?.(sent.result, {}, undefined)?.render(80).join('\n') ?? '';
    expect(rendered).toMatch(/message/i);
    expect(rendered.length).toBeLessThan(sent.text.length);
  });

  it('renders a held lock as a tool-specific warning', async () => {
    writeFileSync(join(ws, 'warning.ts'), 'x');
    await run('lock', one({ action: 'acquire', file: 'warning.ts' }));
    process.env['OCTOCODE_AGENT_ID'] = 'pi:peer';
    const waited = await run('lock', one({ action: 'wait', file: 'warning.ts', waitMs: 0 }));
    const rendered = tools.get('lock')!.renderResult?.(waited.result, {}, undefined)?.render(100).join('\n') ?? '';
    expect(rendered).toMatch(/lock/i);
    expect(rendered).toContain('!');
    expect(rendered).toMatch(/held by pi:test-a/i);
  });
});
