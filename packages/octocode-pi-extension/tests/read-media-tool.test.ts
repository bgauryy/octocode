import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { Type } from 'typebox';
import { readMediaImageFile, registerReadMediaTool } from '../src/tools/read-media-tool.js';
import { setCapabilityCheckForTests, setImageVisibilityCheckForTests } from '../src/tools/image-render.js';
import type { ImageContentPart, ToolDefinition } from '../src/types.js';

let dir: string;
const PNG_1x1 = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M8AAAMBAQDJ/pLvAAAAAElFTkSuQmCC',
  'base64',
);

beforeEach(async () => {
  dir = await mkdtemp(path.join(os.tmpdir(), 'read-image-'));
});
afterEach(async () => {
  setCapabilityCheckForTests(undefined);
  setImageVisibilityCheckForTests(undefined);
  await rm(dir, { recursive: true, force: true });
});

function getTool(): ToolDefinition {
  let def: ToolDefinition | undefined;
  registerReadMediaTool(
    { registerTool: (d: ToolDefinition) => { def = d; } },
    Type,
    new Set<string>(),
    (pi, _n, d) => pi.registerTool?.(d),
  );
  if (!def) throw new Error('readMedia not registered');
  return def;
}

// ── readMedia image unit tests ────────────────────────────────────────────────
describe('readMediaImageFile', () => {
  it('loads a valid png and reports mime + bytes', async () => {
    const f = path.join(dir, 'shot.png');
    await writeFile(f, PNG_1x1);
    const res = readMediaImageFile(f, dir);
    expect(res.ok).toBe(true);
    expect(res.mimeType).toBe('image/png');
    expect(res.base64).toBe(PNG_1x1.toString('base64'));
    expect(res.bytes).toBeGreaterThan(0);
  });

  it('rejects a non-image file with a reason', async () => {
    const f = path.join(dir, 'notes.txt');
    await writeFile(f, 'hello world', 'utf8');
    const res = readMediaImageFile(f, dir);
    expect(res.ok).toBe(false);
    expect(res.message).toMatch(/not a supported image|missing/);
  });

  it('rejects a missing file', () => {
    expect(readMediaImageFile(path.join(dir, 'nope.png'), dir).ok).toBe(false);
  });
});

// ── tool execute/schema tests ─────────────────────────────────────────────────
describe('readMedia tool', () => {
  it('schema exposes a typed read-only media contract', () => {
    const tool = getTool();
    const schema = tool.parameters as {
      properties?: Record<string, unknown>;
      required?: string[];
    };
    expect(Object.keys(schema.properties ?? {})).toEqual(['queries', 'queryRunType']);
    expect((schema.properties?.['queryRunType'] as { enum?: string[] })?.enum).toEqual(['sequential', 'parallel']);
    expect(schema.required ?? []).toContain('queries');
    const q = schema.properties?.['queries'] as {
      items?: { properties?: Record<string, unknown>; required?: string[] };
    };
    expect(q?.items?.properties?.['reasoning']).toBeDefined();
    expect(q?.items?.properties?.['type']).toBeDefined();
    expect(q?.items?.properties?.['path']).toBeDefined();
    expect(q?.items?.required ?? []).toContain('reasoning');
    expect(q?.items?.required ?? []).toContain('type');
  });

  it('returns an image content block for the vision model', async () => {
    const f = path.join(dir, 'shot.png');
    await writeFile(f, PNG_1x1);
    const tool = getTool();
    const result = await tool.execute!(
      't1',
      { queries: [{ reasoning: 'inspect screenshot', type: 'image', path: f }] },
      undefined, undefined, { cwd: dir },
    );
    expect(result.isError).toBeFalsy();
    const img = result.content.find((c) => c.type === 'image') as ImageContentPart | undefined;
    expect(img).toBeDefined();
    expect(img!.mimeType).toBe('image/png');
    expect(img!.data).toBe(PNG_1x1.toString('base64'));
    expect(result.content.some((c) => c.type === 'text')).toBe(true);
  });

  it('reports effective inline capability from TUI mode, protocol, and image settings', async () => {
    const f = path.join(dir, 'shot.png');
    await writeFile(f, PNG_1x1);
    const tool = getTool();
    setCapabilityCheckForTests(() => true);
    setImageVisibilityCheckForTests(() => false);
    const hidden = await tool.execute!(
      'tsettings',
      { queries: [{ reasoning: 'check hidden', type: 'image', path: f }] },
      undefined, undefined, { cwd: dir, hasUI: true, mode: 'tui' },
    );
    const hiddenDetails = hidden.details as { terminalSupportsImages?: boolean; effectiveInlineImages?: boolean };
    expect(hiddenDetails.terminalSupportsImages).toBe(true);
    expect(hiddenDetails.effectiveInlineImages).toBe(false);
    expect((hidden.content.find((c) => c.type === 'text') as { text: string }).text).toMatch(/browser.*ask the user first/i);

    setImageVisibilityCheckForTests(() => true);
    const visible = await tool.execute!(
      'tvisible',
      { queries: [{ reasoning: 'check visible', type: 'image', path: f }] },
      undefined, undefined, { cwd: dir, hasUI: true, mode: 'tui' },
    );
    expect((visible.details as { effectiveInlineImages?: boolean }).effectiveInlineImages).toBe(true);
    expect((visible.content.find((c) => c.type === 'text') as { text: string }).text).not.toMatch(/browser/i);
  });

  it('rejects a non-image with a thrown error (no image block)', async () => {
    const f = path.join(dir, 'x.txt');
    await writeFile(f, 'nope', 'utf8');
    const tool = getTool();
    await expect(
      tool.execute!('t2', { queries: [{ reasoning: 'try image', type: 'image', path: f }] }, undefined, undefined, { cwd: dir }),
    ).rejects.toThrow();
  });

  it('throws when path is missing from the query', async () => {
    const tool = getTool();
    await expect(
      tool.execute!('t3', { queries: [{ reasoning: 'missing path', type: 'image' }] }, undefined, undefined, { cwd: dir }),
    ).rejects.toThrow(/path/);
  });

  it('one-query passthrough: detail shape is preserved', async () => {
    const f = path.join(dir, 'shot.png');
    await writeFile(f, PNG_1x1);
    const tool = getTool();
    const result = await tool.execute!(
      'tpass',
      { queries: [{ reasoning: 'one query', type: 'image', path: f }] },
      undefined, undefined, { cwd: dir },
    );
    // passthroughSingle: details are the per-query result details (not batch wrapper)
    expect((result.details as { ok?: boolean }).ok).toBe(true);
    expect((result.details as { mimeType?: string }).mimeType).toBe('image/png');
  });

  it('multi-query: reads the same image twice and returns batch summary', async () => {
    const f = path.join(dir, 'shot.png');
    await writeFile(f, PNG_1x1);
    const tool = getTool();
    const result = await tool.execute!(
      'tmulti',
      {
        queries: [
          { reasoning: 'first read', type: 'image', path: f },
          { reasoning: 'second read', type: 'image', path: f },
        ],
      },
      undefined, undefined, { cwd: dir },
    );
    expect(result.isError).toBeFalsy();
    const text = (result.content[0] as { text: string }).text;
    expect(text).toMatch(/2 quer/);
    const details = result.details as { results: unknown[] };
    expect(details.results).toHaveLength(2);
  });

  it('renderCall shows the path; renderResult shows the note', async () => {
    const f = path.join(dir, 'shot.png');
    await writeFile(f, PNG_1x1);
    const tool = getTool();
    const call = tool.renderCall!({ queries: [{ reasoning: 'r', type: 'image', path: 'shot.png' }] }).render(120).join('');
    expect(call).toContain('shot.png');

    const result = await tool.execute!(
      't4',
      { queries: [{ reasoning: 'render test', type: 'image', path: f }] },
      undefined, undefined, { cwd: dir },
    );
    const line = tool.renderResult!(result, { expanded: false }).render(120).join('');
    expect(line).toContain('readMedia');
    expect(line).toMatch(/image\/png/);
  });

  it('renderResult emits Ghostty/Kitty image rows when expanded and a browser fallback placeholder otherwise', async () => {
    const f = path.join(dir, 'shot.png');
    await writeFile(f, PNG_1x1);
    const tool = getTool();
    const result = await tool.execute!(
      't5',
      { queries: [{ reasoning: 'render check', type: 'image', path: f }] },
      undefined, undefined, { cwd: dir },
    );
    expect(result.content.some((c) => c.type === 'image')).toBe(true);
    setCapabilityCheckForTests(() => true);
    const renderContext = { showImages: true, state: {}, invalidate: () => {} };
    const inline = tool.renderResult!(result, { expanded: true }, undefined, renderContext).render(120);
    expect(inline.length).toBeGreaterThan(1);

    setCapabilityCheckForTests(() => false);
    const fallback = tool.renderResult!(result, { expanded: true }, undefined, renderContext).render(120);
    expect(fallback.join('\n')).toMatch(/image: shot\.png/i);
  });
});
