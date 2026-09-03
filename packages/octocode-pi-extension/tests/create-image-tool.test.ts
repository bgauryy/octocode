import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mkdtemp, rm, readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Type } from 'typebox';
import {
  cleanupImplicitImageArtifacts,
  createImageFromSvg,
  createImageFromHtml,
  registerCreateImageTool,
} from '../src/tools/create-image-tool.js';
import { setCapabilityCheckForTests, setImageVisibilityCheckForTests } from '../src/tools/image-render.js';
import type { ImageContentPart, RenderContext, ToolDefinition } from '../src/types.js';

let dir: string;
const SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#0d1117"/><circle cx="20" cy="10" r="6" fill="#58a6ff"/></svg>';

beforeEach(async () => { dir = await mkdtemp(path.join(os.tmpdir(), 'create-image-')); });
afterEach(async () => {
  cleanupImplicitImageArtifacts();
  setCapabilityCheckForTests(undefined);
  setImageVisibilityCheckForTests(undefined);
  await rm(dir, { recursive: true, force: true });
});

function getTool(): ToolDefinition {
  let def: ToolDefinition | undefined;
  registerCreateImageTool(
    { registerTool: (d: ToolDefinition) => { def = d; } },
    Type,
    new Set<string>(),
    (pi, _n, d) => pi.registerTool?.(d),
  );
  if (!def) throw new Error('createImage not registered');
  return def;
}

function q(reasoning: string, fields: Record<string, unknown>) {
  return { queries: [{ reasoning, ...fields }] };
}

// ── createImageFromSvg ─────────────────────────────────────────────────────────
describe('createImageFromSvg', () => {
  it('rasterizes valid SVG to a PNG base64 (png magic header)', () => {
    const res = createImageFromSvg(SVG, dir, { name: 'dot.png' });
    expect(res.ok).toBe(true);
    expect(res.bytes).toBeGreaterThan(0);
    expect(Buffer.from(res.base64!, 'base64').subarray(0, 4).toString('latin1')).toBe('\x89PNG');
  });

  it('honors an explicit render width', () => {
    const small = createImageFromSvg(SVG, dir, { width: 40 });
    const big = createImageFromSvg(SVG, dir, { width: 400 });
    expect(big.bytes!).toBeGreaterThan(small.bytes!);
  });

  it('rejects input without an <svg> element', () => {
    const res = createImageFromSvg('not svg at all', dir);
    expect(res.ok).toBe(false);
    expect(res.message).toMatch(/<svg>/);
  });

  it('reports a render failure for malformed SVG', () => {
    expect(createImageFromSvg('<svg><rect width="oops"', dir).ok).toBe(false);
  });

  it('saves the PNG to disk when saveTo is given', async () => {
    const out = path.join(dir, 'nested', 'diagram.png');
    const res = createImageFromSvg(SVG, dir, { saveTo: out });
    expect(res.ok).toBe(true);
    expect(res.savedPath).toBe(out);
    expect(existsSync(out)).toBe(true);
    expect((await readFile(out)).length).toBe(res.bytes);
  });
});

// ── schema ───────────────────────────────────────────────────────────────────────────
describe('createImage schema', () => {
  it('only exposes queries at the top level with per-query reasoning', () => {
    const tool = getTool();
    const schema = tool.parameters as {
      properties?: Record<string, unknown>;
      required?: string[];
    };
      expect(Object.keys(schema.properties ?? {})).toEqual(['queries', 'queryRunType']);
    expect(schema.required ?? []).toContain('queries');
    const q = schema.properties?.['queries'] as {
      items?: { properties?: Record<string, unknown>; required?: string[] };
    };
    expect(q?.items?.properties?.['reasoning']).toBeDefined();
    expect(q?.items?.properties?.['svg']).toBeDefined();
    expect(q?.items?.properties?.['html']).toBeDefined();
    expect(q?.items?.required ?? []).toContain('reasoning');
  });
});

// ── createImage tool (execute) ─────────────────────────────────────────────────
describe('createImage tool', () => {
  it('returns a text note and keeps the image out of model content by default', async () => {
    const tool = getTool();
    const result = await tool.execute!('c1', q('test render', { svg: SVG }), undefined, undefined, { cwd: dir });
    expect(result.isError).toBeFalsy();
    expect(result.content.some((c) => c.type === 'image')).toBe(false);
    expect(result.content.some((c) => c.type === 'text')).toBe(true);
    const details = result.details as { base64?: string; imagePath?: string };
    expect(details.base64).toBeUndefined();
    expect(details.imagePath).toBeTruthy();
    expect(existsSync(details.imagePath!)).toBe(true);
  });

  it('includes a vision image block when showToModel is true', async () => {
    setCapabilityCheckForTests(() => true);
    setImageVisibilityCheckForTests(() => true);
    const tool = getTool();
    const result = await tool.execute!('c2', q('show to model', { svg: SVG, showToModel: true }), undefined, undefined, { cwd: dir, hasUI: true, mode: 'tui' });
    const img = result.content.find((c) => c.type === 'image') as ImageContentPart | undefined;
    expect(img).toBeDefined();
    expect(img!.mimeType).toBe('image/png');
    expect((result.details as { base64?: string; imagePath?: string }).base64).toBeUndefined();
    expect((result.details as { imagePath?: string }).imagePath).toBeUndefined();
  });

  it('throws for invalid svg (no image block returned)', async () => {
    const tool = getTool();
    await expect(
      tool.execute!('c3', q('bad svg', { svg: 'nope' }), undefined, undefined, { cwd: dir }),
    ).rejects.toThrow();
  });

  it('throws when neither svg nor html is given', async () => {
    const tool = getTool();
    await expect(
      tool.execute!('c4', q('no source', {}), undefined, undefined, { cwd: dir }),
    ).rejects.toThrow(/svg.*html|html.*svg/);
  });

  it('throws when both svg and html are given', async () => {
    const tool = getTool();
    await expect(
      tool.execute!('c4b', q('both', { svg: SVG, html: '<div>x</div>' }), undefined, undefined, { cwd: dir }),
    ).rejects.toThrow(/only one/);
  });

  it('one-query passthrough: detail shape preserved', async () => {
    const tool = getTool();
    const result = await tool.execute!('cpass', q('passthrough check', { svg: SVG, name: 'chart.png' }), undefined, undefined, { cwd: dir });
    expect((result.details as { ok?: boolean }).ok).toBe(true);
    expect((result.details as { mimeType?: string }).mimeType).toBe('image/png');
    const details = result.details as { base64?: string; imagePath?: string };
    expect(details.base64).toBeUndefined();
    expect(existsSync(details.imagePath!)).toBe(true);
  });

  it('multi-query: renders two SVGs and returns batch summary', async () => {
    const tool = getTool();
    const result = await tool.execute!(
      'cmulti',
      {
        queries: [
          { reasoning: 'render first', svg: SVG, name: 'a.png' },
          { reasoning: 'render second', svg: SVG, name: 'b.png' },
        ],
      },
      undefined, undefined, { cwd: dir },
    );
    expect(result.isError).toBeFalsy();
    const text = (result.content[0] as { text: string }).text;
    expect(text).toMatch(/2 quer/);
    const details = result.details as { results: unknown[] };
    expect(details.results).toHaveLength(2);
    expect(JSON.stringify(details)).not.toContain('base64');
  });

  it('renderResult inlines the rendered image below the status line (placeholder when unsupported)', async () => {
    setCapabilityCheckForTests(() => false);
    try {
      const tool = getTool();
      const result = await tool.execute!('c5', q('render test', { svg: SVG, name: 'chart.png' }), undefined, undefined, { cwd: dir });
      const ctx = ({ state: {}, invalidate() {} } as unknown) as RenderContext;
      const lines = tool.renderResult!(result, { expanded: true }, undefined, ctx).render(120);
      expect(lines.length).toBeGreaterThan(1);
      expect(lines.some((l) => l.includes('\uD83D\uDDBC image: chart.png'))).toBe(true);
    } finally {
      setCapabilityCheckForTests(undefined);
    }
  });

  it('on an image-incapable terminal: saves the PNG and suggests opening in a browser', async () => {
    setCapabilityCheckForTests(() => false);
    try {
      const tool = getTool();
      const result = await tool.execute!('cff', q('fallback test', { svg: SVG, name: 'chart.png' }), undefined, undefined, { cwd: dir });
      const text = (result.content.find((c) => c.type === 'text') as { text: string }).text;
      expect(text).toMatch(/no inline-image support/i);
      expect(text).toMatch(/ask the user first/i);
      const details = result.details as { savedPath?: string; terminalSupportsImages?: boolean };
      expect(details.terminalSupportsImages).toBe(false);
      expect(details.savedPath).toBeTruthy();
      expect(existsSync(details.savedPath!)).toBe(true);
    } finally {
      setCapabilityCheckForTests(undefined);
    }
  });

  it('on an image-capable terminal: no browser suggestion, no fallback save', async () => {
    setCapabilityCheckForTests(() => true);
    try {
      const tool = getTool();
      const result = await tool.execute!('cok', q('capable terminal', { svg: SVG }), undefined, undefined, { cwd: dir, hasUI: true, mode: 'tui' });
      const text = (result.content.find((c) => c.type === 'text') as { text: string }).text;
      expect(text).not.toMatch(/browser/i);
      const details = result.details as { savedPath?: string; imagePath?: string; terminalSupportsImages?: boolean };
      expect(details.terminalSupportsImages).toBe(true);
      expect(details.savedPath).toBeFalsy();
      expect(existsSync(details.imagePath!)).toBe(true);
    } finally {
      setCapabilityCheckForTests(undefined);
    }
  });

  it('uses effective capability when image display is disabled despite protocol support', async () => {
    setCapabilityCheckForTests(() => true);
    setImageVisibilityCheckForTests(() => false);
    const tool = getTool();
    const result = await tool.execute!('csettings', q('settings test', { svg: SVG, name: 'hidden.png' }), undefined, undefined, { cwd: dir, hasUI: true, mode: 'tui' });
    const text = (result.content.find((c) => c.type === 'text') as { text: string }).text;
    const details = result.details as { savedPath?: string; terminalSupportsImages?: boolean; effectiveInlineImages?: boolean; temporaryArtifact?: boolean };
    expect(details.terminalSupportsImages).toBe(true);
    expect(details.effectiveInlineImages).toBe(false);
    expect(details.temporaryArtifact).toBe(true);
    expect(details.savedPath).toBeTruthy();
    expect(text).toMatch(/ask the user first/i);
  });

  it('cleans implicit fallback artifacts but preserves explicit saveTo output', async () => {
    setCapabilityCheckForTests(() => false);
    setImageVisibilityCheckForTests(() => true);
    const tool = getTool();
    const explicitPath = path.join(dir, 'durable.png');
    const implicit = await tool.execute!('ctmp', q('implicit artifact', { svg: SVG, name: 'temporary.png' }), undefined, undefined, { cwd: dir, hasUI: true, mode: 'tui' });
    const explicit = await tool.execute!('cdurable', q('explicit save', { svg: SVG, saveTo: explicitPath }), undefined, undefined, { cwd: dir, hasUI: true, mode: 'tui' });
    const implicitPath = (implicit.details as { savedPath?: string }).savedPath!;
    expect(existsSync(implicitPath)).toBe(true);
    expect((implicit.details as { temporaryArtifact?: boolean }).temporaryArtifact).toBe(true);
    expect((explicit.details as { temporaryArtifact?: boolean }).temporaryArtifact).toBe(false);
    cleanupImplicitImageArtifacts();
    expect(existsSync(implicitPath)).toBe(false);
    expect(existsSync(explicitPath)).toBe(true);
  });

  it('rejects empty html markup', async () => {
    const res = await createImageFromHtml('   ', dir);
    expect(res.ok).toBe(false);
    expect(res.message).toMatch(/html/);
  });

  it('renders HTML through an injected browser renderer', async () => {
    const png = Buffer.concat([Buffer.from('\x89PNG\r\n\x1a\n', 'latin1'), Buffer.from('mock-image')]);
    const renderHtml = vi.fn(async () => png);
    const res = await createImageFromHtml(
      '<div style="width:120px;height:60px;background:#58a6ff;color:#fff;font:16px sans-serif;display:flex;align-items:center;justify-content:center">hi</div>',
      dir,
      { name: 'card.png' },
      { renderHtml },
    );
    expect(res.ok).toBe(true);
    expect(res.bytes).toBeGreaterThan(0);
    expect(Buffer.from(res.base64!, 'base64').subarray(0, 4).toString('latin1')).toBe('\x89PNG');
    expect(renderHtml).toHaveBeenCalledOnce();
  });

  it('renderResult does NOT self-render when showToModel put an image in content (pi renders it)', async () => {
    setCapabilityCheckForTests(() => false);
    try {
      const tool = getTool();
      const result = await tool.execute!('c6', q('show to model', { svg: SVG, name: 'chart.png', showToModel: true }), undefined, undefined, { cwd: dir });
      expect(result.content.some((c) => c.type === 'image')).toBe(true);
      const ctx = ({ state: {}, invalidate() {} } as unknown) as RenderContext;
      const lines = tool.renderResult!(result, { expanded: true }, undefined, ctx).render(120);
      expect(lines.length).toBe(1);
      expect(lines.some((l) => l.includes('\uD83D\uDDBC image:'))).toBe(false);
    } finally {
      setCapabilityCheckForTests(undefined);
    }
  });
});
