/**
 * Tests for the Octocode branded tool renderer decorator and helpers.
 *
 * Uses stub theme and stub components; no real Pi host required.
 */

import { describe, it, expect, vi } from 'vitest';
import { Type } from 'typebox';
import { withOctocodeRender } from '../src/branding/renderers.js';
import { buildOctocodeRenderCall, buildToolCallSummary } from '../src/tools/render-helpers.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { registerBashTool } from '../src/tools/bash-tool.js';
import { registerFileTool } from '../src/tools/file-tool.js';
import { registerAskUserTool } from '../src/tools/ask-user-tool.js';
import { registerPlanTool } from '../src/tools/plan-tool.js';
import { registerWebTool } from '../src/tools/web-tool.js';
import { registerReadMediaTool } from '../src/tools/read-media-tool.js';
import { registerMediaTool } from '../src/tools/create-media-tool.js';
import { registerUnifiedAgentTool } from '../src/tools/unified-agent-tool.js';
import type { ToolDefinition, PiTheme, ToolCallResult, RenderResultOptions } from '../src/types.js';
// ─── Stub theme ───────────────────────────────────────────────────────────────

const stubTheme: PiTheme = {
  fg: (color: string, text: string) => `[${color}:${text}]`,
  bold: (text: string) => `**${text}**`,
} as unknown as PiTheme;

// ─── Helpers ─────────────────────────────────────────────────────────────────

function render(component: { render(w: number): string[] }, width = 80): string[] {
  return component.render(width);
}

function makeDef(overrides: Partial<ToolDefinition> = {}): ToolDefinition {
  return {
    name: 'testTool',
    label: 'Test Tool',
    description: 'A test tool',
    parameters: {} as never,
    async execute() {
      return { content: [{ type: 'text', text: 'ok' }], details: undefined };
    },
    ...overrides,
  };
}

function makeResult(overrides: Partial<ToolCallResult> = {}): ToolCallResult {
  return {
    content: [{ type: 'text', text: 'result output' }],
    details: { results: [] },
    isError: false,
    ...overrides,
  };
}

// ─── Shared registration helper ───────────────────────────────────────────────

describe('registerUniqueTool with builtin overrides', () => {
  it('wraps file/bash renderers and rejects duplicate names through the shared helper', () => {
    const tools = new Map<string, ToolDefinition>();
    const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
    const names = new Set<string>();

    registerFileTool(pi, Type, names, registerUniqueTool);
    registerBashTool(pi, Type, names, registerUniqueTool);

    expect([...tools.keys()]).toEqual(['file', 'bash']);
    for (const name of ['file', 'bash']) {
      expect(tools.get(name)?.renderCall).toBeTypeOf('function');
      expect(tools.get(name)?.renderResult).toBeTypeOf('function');
    }
    expect(() => registerFileTool(pi, Type, names, registerUniqueTool)).toThrow(/tool name collision: file/);
  });

  it('renders Bash, file, plan, web, media, and agent queries as operation/reason pairs', () => {
    const tools = new Map<string, ToolDefinition>();
    const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
    const names = new Set<string>();
    registerBashTool(pi, Type, names, registerUniqueTool);
    registerFileTool(pi, Type, names, registerUniqueTool);
    registerPlanTool(pi, Type, names, registerUniqueTool);
    registerWebTool(pi, Type, names, registerUniqueTool);
    registerReadMediaTool(pi, Type, names, registerUniqueTool);
    registerMediaTool(pi, Type, names, registerUniqueTool);
    registerUnifiedAgentTool(pi, Type, names, registerUniqueTool);

    const cases: Array<[string, Array<Record<string, unknown>>]> = [
      ['bash', [{ reasoning: 'run alpha', command: 'echo alpha' }, { reasoning: 'run beta', command: 'echo beta' }]],
      ['file', [{ reasoning: 'write alpha', type: 'write', path: '/a.ts' }, { reasoning: 'delete beta', type: 'delete', path: '/b.ts' }]],
      ['plan', [{ reasoning: 'show plan', action: 'show' }, { reasoning: 'clear plan', action: 'clear' }]],
      ['web', [{ reasoning: 'search alpha', query: 'alpha' }, { reasoning: 'fetch beta', url: 'https://example.com/beta' }]],
      ['readMedia', [{ reasoning: 'inspect alpha', type: 'image', path: '/a.png', view: 'metadata' }, { reasoning: 'inspect beta', type: 'image', path: '/b.png', view: 'metadata' }]],
      ['media', [{ reasoning: 'render alpha', type: 'image', dest: '/a.png' }, { reasoning: 'render beta', type: 'image', dest: '/b.png' }]],
      ['agent', [{ reasoning: 'inspect alpha', type: 'inspect', agentId: 'alpha' }, { reasoning: 'inspect beta', type: 'inspect', agentId: 'beta' }]],
    ];

    for (const [toolName, queries] of cases) {
      const lines = render(tools.get(toolName)!.renderCall!({ queries }, stubTheme), 160);
      expect(lines, toolName).toHaveLength(5);
      expect(lines[0], toolName).toMatch(/2 queries.*sequential/);
      expect(lines[2], toolName).toContain(String(queries[0]!['reasoning']));
      expect(lines[4], toolName).toContain(String(queries[1]!['reasoning']));
      expect(lines.join('\n'), toolName).not.toMatch(/\+1|why:|reasoning:/i);
    }
  });
});

// ─── withOctocodeRender — basic decoration ────────────────────────────────────

describe('withOctocodeRender', () => {
  it('adds renderCall when missing', () => {
    const def = makeDef();
    expect(def.renderCall).toBeUndefined();
    withOctocodeRender(def);
    expect(typeof def.renderCall).toBe('function');
  });

  it('adds renderResult when missing', () => {
    const def = makeDef();
    withOctocodeRender(def);
    expect(typeof def.renderResult).toBe('function');
  });

  it('wraps an existing renderCall into ordered operation/reasoning blocks', () => {
    const customRenderCall = vi.fn((args: unknown) => {
      const envelope = args as { queries?: Array<Record<string, unknown>> };
      const query = envelope.queries?.[0] ?? {};
      return { render: () => [`custom ${String(query['value'] ?? '?')}`], invalidate: () => {} };
    });
    const def = makeDef({ renderCall: customRenderCall });
    withOctocodeRender(def);

    expect(def.renderCall).not.toBe(customRenderCall);
    const lines = render(def.renderCall!({
      queries: [
        { value: 'alpha', reasoning: 'first reason' },
        { value: 'beta', reasoning: 'second reason' },
      ],
    }, stubTheme), 120);

    expect(lines).toHaveLength(5);
    expect(lines[0]).toMatch(/2 queries.*sequential/);
    expect(lines[1]).toContain('custom alpha');
    expect(lines[2]).toContain('first reason');
    expect(lines[3]).toContain('custom beta');
    expect(lines[4]).toContain('second reason');
    expect(lines.join('\n')).not.toMatch(/why:|reasoning:/i);
    expect(customRenderCall).toHaveBeenCalledTimes(2);
    for (const [callArgs] of customRenderCall.mock.calls) {
      expect(JSON.stringify(callArgs)).not.toContain('reasoning');
    }
  });

  it('delegates single-query results and renders multi-query results one row per query', () => {
    const customRenderResult = vi.fn().mockReturnValue({ render: () => ['custom'], invalidate: () => {} });
    const def = makeDef({ renderResult: customRenderResult });
    withOctocodeRender(def);

    const single = def.renderResult!(makeResult(), {} as RenderResultOptions, stubTheme);
    expect(customRenderResult).toHaveBeenCalledOnce();
    expect(single.render(80)).toEqual(['custom']);

    customRenderResult.mockClear();
    const batch = makeResult({
      details: {
        results: [
          { index: 0, reasoning: 'first', status: 'success', summary: 'alpha ok', result: {} },
          { index: 1, reasoning: 'second', status: 'failed', summary: 'beta failed', result: {} },
          { index: 2, reasoning: 'third', status: 'not-run', summary: 'not run', result: undefined },
        ],
      },
    });
    const output = def.renderResult!(
      batch,
      {} as RenderResultOptions,
      stubTheme,
      { args: { queries: [{ reasoning: 'first' }, { reasoning: 'second' }, { reasoning: 'third' }] } } as never,
    );
    const lines = output.render(120);
    expect(customRenderResult).not.toHaveBeenCalled();
    expect(lines).toHaveLength(3);
    expect(lines[0]).toMatch(/✓.*\[0\].*alpha ok/);
    expect(lines[1]).toMatch(/✗.*\[1\].*beta failed/);
    expect(lines[2]).toMatch(/[–-].*\[2\].*not run/);
  });

  it('overrides an existing renderResult on a system error (context.isError)', () => {
    const customRenderResult = vi.fn().mockReturnValue({ render: () => ['custom'], invalidate: () => {} });
    const def = makeDef({ name: 'boomTool', renderResult: customRenderResult });
    withOctocodeRender(def);
    // Pi sets context.isError when execute() threw / the call was rejected; the
    // tool's own renderer (keyed off result.isError) must NOT paint a success row.
    const out = def.renderResult!(
      makeResult({ isError: false, content: [{ type: 'text', text: 'arguments: must be object' }] }),
      { isPartial: false } as RenderResultOptions,
      stubTheme,
      { isError: true, invalidate() {} } as never,
    );
    expect(customRenderResult).not.toHaveBeenCalled();
    const line = out.render(200).join('');
    expect(line).toContain('boomTool');
    expect(line).toContain('arguments: must be object');
  });

  it('preserves execute and parameters', async () => {
    const def = makeDef();
    const originalExecute = def.execute;
    const originalParameters = def.parameters;
    withOctocodeRender(def);
    expect(def.execute).toBe(originalExecute);
    expect(def.parameters).toBe(originalParameters);
    // execute still works
    const result = await def.execute({} as never, {} as never);
    expect(result.content[0]).toMatchObject({ type: 'text', text: 'ok' });
  });

  it('returns the same def reference', () => {
    const def = makeDef();
    const returned = withOctocodeRender(def);
    expect(returned).toBe(def);
  });

  it('uses displayName opt in branded title', () => {
    const def = makeDef({ name: 'internalName' });
    withOctocodeRender(def, { displayName: 'BrandedName' });
    const component = def.renderCall!({}, stubTheme);
    const lines = render(component);
    expect(lines[0]).toContain('BrandedName');
    expect(lines[0]).not.toContain('internalName');
  });
});

// ─── Branded renderCall output ────────────────────────────────────────────────

describe('branded renderCall', () => {
  it('includes the tool name in the output line', () => {
    const def = makeDef({ name: 'myOctoTool' });
    withOctocodeRender(def);
    const component = def.renderCall!({}, stubTheme);
    const lines = render(component);
    expect(lines.length).toBeGreaterThanOrEqual(1);
    expect(lines[0]).toContain('myOctoTool');
  });

  it('includes args summary when present', () => {
    const def = makeDef({ name: 'localGetFileContent' });
    withOctocodeRender(def);
    const args = { queries: [{ path: '/src/foo.ts', startLine: 10 }] };
    const component = def.renderCall!(args, stubTheme);
    const lines = render(component);
    expect(lines[0]).toContain('foo.ts');
  });

  it('truncates to requested width', () => {
    const def = makeDef({ name: 'toolWithLongName' });
    withOctocodeRender(def);
    const args = { queries: [{ path: '/very/long/path/that/exceeds/screen/width/definitely.ts' }] };
    const component = def.renderCall!(args, stubTheme);
    // render at narrow width — should not exceed it
    const lines = render(component, 40);
    for (const line of lines) {
      // strip ANSI codes to count visible chars
      const stripped = line.replace(/\x1b\[[0-9;]*m/g, '');
      expect(stripped.length).toBeLessThanOrEqual(42); // some tolerance for ellipsis
    }
  });

  it('works without a theme (graceful degradation)', () => {
    const def = makeDef({ name: 'noThemeTool' });
    withOctocodeRender(def);
    const component = def.renderCall!({});
    const lines = render(component);
    expect(lines[0]).toContain('noThemeTool');
  });
});

// ─── Branded renderResult output ─────────────────────────────────────────────

describe('branded renderResult', () => {
  const opts: RenderResultOptions = { expanded: false, isPartial: false };

  it('shows success icon on success', () => {
    const def = makeDef();
    withOctocodeRender(def);
    const result = makeResult({ isError: false });
    const component = def.renderResult!(result, opts, stubTheme);
    const lines = render(component);
    expect(lines[0]).toContain('✓');
  });

  it('shows error icon on error', () => {
    const def = makeDef();
    withOctocodeRender(def);
    const result = makeResult({ isError: true });
    const component = def.renderResult!(result, opts, stubTheme);
    const lines = render(component);
    expect(lines[0]).toContain('✗');
  });

  it('shows running indicator when isPartial', () => {
    const def = makeDef({ name: 'streamingTool' });
    withOctocodeRender(def);
    const result = makeResult();
    const partialOpts: RenderResultOptions = { expanded: false, isPartial: true };
    const component = def.renderResult!(result, partialOpts, stubTheme);
    const lines = render(component);
    // should contain the tool name and a running indicator
    expect(lines[0]).toContain('streamingTool');
    expect(lines[0]).toMatch(/running|…/);
  });

  it('shows expanded content when expanded:true', () => {
    const def = makeDef();
    withOctocodeRender(def);
    const text = 'line one\nline two\nline three';
    const result = makeResult({ content: [{ type: 'text', text }] });
    const expandedOpts: RenderResultOptions = { expanded: true, isPartial: false };
    const component = def.renderResult!(result, expandedOpts, stubTheme);
    const lines = render(component);
    expect(lines.length).toBeGreaterThan(1);
    const allText = lines.join('\n');
    expect(allText).toContain('line one');
  });

  it('truncates long result lines to width', () => {
    const def = makeDef();
    withOctocodeRender(def);
    const result = makeResult();
    const component = def.renderResult!(result, { expanded: false, isPartial: false }, stubTheme);
    const lines = render(component, 30);
    for (const line of lines) {
      const stripped = line.replace(/\x1b\[[0-9;]*m/g, '');
      expect(stripped.length).toBeLessThanOrEqual(32);
    }
  });

  it('works without theme', () => {
    const def = makeDef();
    withOctocodeRender(def);
    const result = makeResult();
    const component = def.renderResult!(result, opts);
    const lines = render(component);
    expect(lines.length).toBeGreaterThanOrEqual(1);
  });
});

// ─── buildOctocodeRenderCall direct tests ─────────────────────────────────────

describe('buildOctocodeRenderCall', () => {
  it('produces branded title with theme colors', () => {
    const c = buildOctocodeRenderCall('ghSearch', { queries: [{ operation: 'code', keywords: ['useState'] }] }, stubTheme);
    const lines = render(c);
    expect(lines[0]).toContain('ghSearch');
    expect(lines[0]).toContain('useState');
  });

  it('handles empty queries gracefully', () => {
    const c = buildOctocodeRenderCall('localSearch', { queries: [] }, stubTheme);
    const lines = render(c);
    expect(lines[0]).toContain('localSearch');
  });

  it('renders every nested Octocode query with its unlabeled reason on the next line', () => {
    const c = buildOctocodeRenderCall('localGetFileContent', {
      queries: [
        { path: '/src/a.ts', reasoning: 'read alpha' },
        { path: '/src/b.ts', reasoning: 'read beta' },
      ],
    }, stubTheme);
    const lines = render(c, 120);
    expect(lines).toHaveLength(5);
    expect(lines[0]).toMatch(/2 queries.*sequential/);
    expect(lines[1]).toContain('a.ts');
    expect(lines[2]).toContain('read alpha');
    expect(lines[3]).toContain('b.ts');
    expect(lines[4]).toContain('read beta');
    expect(lines.join('\n')).not.toMatch(/\+1|why:|reasoning:/i);
  });
});

// ─── buildToolCallSummary spot checks ────────────────────────────────────────

describe('buildToolCallSummary', () => {
  it('ghSearch: formats keywords and repo', () => {
    const summary = buildToolCallSummary('ghSearch', {
      queries: [{ operation: 'code', keywords: ['renderCall'], owner: 'earendil', repo: 'pi' }],
    });
    expect(summary).toContain('renderCall');
    expect(summary).toContain('earendil/pi');
  });

  it('localGetFileContent: shows file basename and line range', () => {
    const summary = buildToolCallSummary('localGetFileContent', {
      queries: [{ path: '/src/tools/render-helpers.ts', startLine: 5, endLine: 20 }],
    });
    expect(summary).toContain('render-helpers.ts');
  });

  it('returns empty string for unknown tool with empty args', () => {
    const summary = buildToolCallSummary('unknownTool', { queries: [] });
    expect(typeof summary).toBe('string');
  });

  it('does not collapse multiple queries into a +N summary', () => {
    const summary = buildToolCallSummary('localGetFileContent', {
      queries: [
        { path: '/a.ts' },
        { path: '/b.ts' },
        { path: '/c.ts' },
      ],
    });
    expect(summary).not.toMatch(/\+2|3 queries/);
  });
});

// ─── Shared load helper ───────────────────────────────────────────────────────

function loadTool(
  registerFn: (pi: { registerTool?: (d: ToolDefinition) => void }, T: unknown, names: Set<string>, reg: unknown) => void,
  toolName: string,
): ToolDefinition {
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (d: ToolDefinition) => tools.set(d.name, d) };
  registerFn(pi, Type, new Set(), registerUniqueTool);
  return tools.get(toolName)!;
}

// ─── file-tool custom renderer ────────────────────────────────────────────────

describe('file-tool renderResult', () => {
  it('write + path: renders op \u00b7 path with no dangling separator', () => {
    const tool = loadTool(registerFileTool as never, 'file');
    const result: ToolCallResult = {
      content: [{ type: 'text', text: 'written' }],
      details: { operation: 'write', path: '/src/foo.ts', bytes: 42 },
      isError: false,
    };
    const lines = render(tool.renderResult!(result, { isPartial: false }, stubTheme), 160);
    const joined = lines.join('\n');
    expect(joined).toContain('write');
    expect(joined).toContain('/src/foo.ts');
    // The separator appears between op and path, not at the end.
    expect(lines[0]).not.toMatch(/\u00b7\s*$/);
  });

  it('write without path: no dangling \u00b7 separator', () => {
    const tool = loadTool(registerFileTool as never, 'file');
    const result: ToolCallResult = {
      content: [{ type: 'text', text: 'written' }],
      details: { operation: 'write' }, // deliberately no path key
      isError: false,
    };
    const lines = render(tool.renderResult!(result, { isPartial: false }, stubTheme));
    expect(lines[0]).toContain('write');
    // Must NOT produce a dangling «·» at the end of the line.
    expect(lines[0]).not.toMatch(/\u00b7\s*$/);
  });

  it('isPartial: shows spinner (not static \u2026 prefix)', () => {
    const tool = loadTool(registerFileTool as never, 'file');
    const result: ToolCallResult = { content: [], details: undefined, isError: false };
    const lines = render(tool.renderResult!(result, { isPartial: true }, stubTheme));
    expect(lines).toHaveLength(1);
    // The old output was the static string «… file (Octocode)» — now it uses the
    // spinner frame (a rotating character), so the line must NOT start with the
    // literal … glyph that the old code produced.
    const stripped = lines[0].replace(/\x1b\[[0-9;]*m/g, '');
    expect(stripped).not.toMatch(/^…/);
    // The tool name must still be present.
    expect(stripped).toContain('file (Octocode)');
  });

  it('renderCall: reasoning appears on a second line', () => {
    const tool = loadTool(registerFileTool as never, 'file');
    const lines = render(
      tool.renderCall!(
        { queries: [{ type: 'write', path: '/src/x.ts', reasoning: 'create fixture for test' }] },
        stubTheme,
      ),
    );
    expect(lines).toHaveLength(2);
    expect(lines[1]).toContain('create fixture for test');
  });

  it('renderCall: no second line when reasoning is absent', () => {
    const tool = loadTool(registerFileTool as never, 'file');
    const lines = render(
      tool.renderCall!(
        { queries: [{ type: 'write', path: '/src/x.ts' }] },
        stubTheme,
      ),
    );
    expect(lines).toHaveLength(1);
  });

  it('renderCall: renders every file query and its unlabeled reasoning one by one', () => {
    const tool = loadTool(registerFileTool as never, 'file');
    const lines = render(
      tool.renderCall!(
        {
          queries: [
            { type: 'write', path: '/src/a.ts', reasoning: 'create alpha' },
            { type: 'delete', path: '/src/b.ts', reasoning: 'remove beta' },
          ],
        },
        stubTheme,
      ),
      120,
    );
    expect(lines).toHaveLength(5);
    expect(lines[0]).toMatch(/2 queries.*sequential/);
    expect(lines[1]).toMatch(/write.*a\.ts/);
    expect(lines[2]).toContain('create alpha');
    expect(lines[3]).toMatch(/delete.*b\.ts/);
    expect(lines[4]).toContain('remove beta');
    expect(lines.join('\n')).not.toMatch(/\+1|why:|reasoning:/i);
  });
});

// ─── askUser custom renderer ──────────────────────────────────────────────────

describe('askUser renderCall + renderResult', () => {
  it('renderCall: reasoning appears on second line when provided', () => {
    const tool = loadTool(registerAskUserTool as never, 'askUser');
    const lines = render(
      tool.renderCall!(
        {
          queries: [{
            question: 'Which approach?',
            options: [{ value: 'a', label: 'A' }, { value: 'b', label: 'B' }],
            reasoning: 'need user preference before committing',
          }],
        },
        stubTheme,
      ),
    );
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain('Which approach?');
    expect(lines[1]).toContain('need user preference before committing');
  });

  it('renderCall: only one line when reasoning is absent', () => {
    const tool = loadTool(registerAskUserTool as never, 'askUser');
    const lines = render(
      tool.renderCall!(
        { queries: [{ question: 'Confirm?', options: [{ value: 'y', label: 'Yes' }] }] },
        stubTheme,
      ),
    );
    expect(lines).toHaveLength(1);
  });

  it('renderResult isPartial: shows spinner not silent blank', () => {
    const tool = loadTool(registerAskUserTool as never, 'askUser');
    const result: ToolCallResult = { content: [], details: undefined, isError: false };
    const lines = render(tool.renderResult!(result, { isPartial: true }, stubTheme));
    expect(lines).toHaveLength(1);
    // Should show the tool name while waiting for input.
    const stripped = lines[0].replace(/\x1b\[[0-9;]*m/g, '');
    expect(stripped).toContain('askUser');
  });

  it('renderResult final: selected status shows \u2713 and label', () => {
    const tool = loadTool(registerAskUserTool as never, 'askUser');
    const result: ToolCallResult = {
      content: [{ type: 'text', text: 'selected' }],
      details: { status: 'selected', value: 'a', label: 'Option A' },
      isError: false,
    };
    const lines = render(tool.renderResult!(result, { isPartial: false }, stubTheme));
    const joined = lines.join('\n');
    expect(joined).toContain('\u2713'); // success glyph
    expect(joined).toContain('Option A');
  });

  it('renderResult final: cancelled shows cancel indicator', () => {
    const tool = loadTool(registerAskUserTool as never, 'askUser');
    const result: ToolCallResult = {
      content: [{ type: 'text', text: 'cancelled' }],
      details: { status: 'cancelled' },
      isError: false,
    };
    const lines = render(tool.renderResult!(result, { isPartial: false }, stubTheme));
    const stripped = lines.join('\n').replace(/\x1b\[[0-9;]*m/g, '');
    expect(stripped).toContain('cancelled');
  });
});

// ─── plan-tool custom renderer ────────────────────────────────────────────────

describe('plan-tool renderCall + renderResult', () => {
  it('renderCall: space between title and action parenthetical', () => {
    const tool = loadTool(registerPlanTool as never, 'plan');
    const lines = render(
      tool.renderCall!(
        { queries: [{ action: 'start', index: 2 }] },
        stubTheme,
      ),
    );
    // Must not produce 'plan(start)' without a space — should be 'plan (start #2)'
    expect(lines[0]).not.toMatch(/plan\(/);
    expect(lines[0]).toContain('start');
  });

  it('renderCall: reasoning on second line when provided', () => {
    const tool = loadTool(registerPlanTool as never, 'plan');
    const lines = render(
      tool.renderCall!(
        { queries: [{ action: 'complete', reasoning: 'all acceptance criteria met' }] },
        stubTheme,
      ),
    );
    expect(lines).toHaveLength(2);
    expect(lines[1]).toContain('all acceptance criteria met');
  });

  it('renderCall: only one line when reasoning is absent', () => {
    const tool = loadTool(registerPlanTool as never, 'plan');
    const lines = render(
      tool.renderCall!(
        { queries: [{ action: 'show' }] },
        stubTheme,
      ),
    );
    expect(lines).toHaveLength(1);
  });

  it('renderResult isPartial: shows spinner', () => {
    const tool = loadTool(registerPlanTool as never, 'plan');
    const result: ToolCallResult = { content: [], details: undefined, isError: false };
    const lines = render(tool.renderResult!(result, { isPartial: true }, stubTheme));
    expect(lines).toHaveLength(1);
    const stripped = lines[0].replace(/\x1b\[[0-9;]*m/g, '');
    expect(stripped).toContain('plan');
  });

  it('renderResult step summary: shows done/total counts', () => {
    const tool = loadTool(registerPlanTool as never, 'plan');
    const result: ToolCallResult = {
      content: [{ type: 'text', text: '\u25c6 plan 1/3' }],
      details: {
        action: 'complete',
        steps: [
          { id: '1', text: 'step one', status: 'done' },
          { id: '2', text: 'step two', status: 'todo' },
          { id: '3', text: 'step three', status: 'todo' },
        ],
      },
      isError: false,
    };
    const lines = render(tool.renderResult!(result, { isPartial: false }, stubTheme));
    const joined = lines.join('\n').replace(/\x1b\[[0-9;]*m/g, '');
    expect(joined).toContain('1/3');
  });
});
