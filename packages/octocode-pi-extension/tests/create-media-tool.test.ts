import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import { mkdtemp, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Type } from 'typebox';
import {
  pdfDocumentFromHtml,
  pdfDocumentFromMarkdown,
  pdfDocumentFromImages,
  runMediaOperation,
  registerMediaTool,
} from '../src/tools/create-media-tool.js';
import type { ToolDefinition } from '../src/types.js';
import { cleanupImplicitImageArtifacts } from '../src/tools/create-image-tool.js';

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#0d1117"/><circle cx="20" cy="10" r="6" fill="#58a6ff"/></svg>';

// ── pure PDF composition helpers ────────────────────────────────────────────
describe('pdf document builders', () => {
  it('wraps fragment html but passes full docs through', () => {
    expect(pdfDocumentFromHtml('<h1>hi</h1>')).toMatch(/<!doctype html>.*<h1>hi<\/h1>/is);
    const full = '<!doctype html><html><body>x</body></html>';
    expect(pdfDocumentFromHtml(full)).toBe(full);
  });
  it('renders markdown to html', () => {
    const doc = pdfDocumentFromMarkdown('# Title\n\n- a\n- b');
    expect(doc).toMatch(/<h1>Title<\/h1>/);
    expect(doc).toMatch(/<li>a<\/li>/);
  });
  it('makes one page per image with page-breaks', () => {
    const doc = pdfDocumentFromImages(['data:image/png;base64,AAA', 'data:image/png;base64,BBB']);
    expect(doc.match(/<img/g)?.length).toBe(2);
    expect(doc).toMatch(/page-break-after:always/);
  });
});

// ── registration ────────────────────────────────────────────────────────────
describe('registerMediaTool', () => {
  it('registers the single artifact-producing `media` tool', () => {
    let def: ToolDefinition | undefined;
    registerMediaTool({ registerTool: (d: ToolDefinition) => { def = d; } }, Type, new Set(), (pi, _n, d) => pi.registerTool?.(d));
    expect(def?.name).toBe('media');
  });

  it('keeps preview pixels out of result details and persists one render artifact', async () => {
    const dir = await mkdtemp(path.join(os.tmpdir(), 'create-media-details-'));
    try {
      let def: ToolDefinition | undefined;
      registerMediaTool({ registerTool: (d: ToolDefinition) => { def = d; } }, Type, new Set(), (pi, _n, d) => pi.registerTool?.(d));
      const result = await def!.execute!(
        'media-details',
        { queries: [{ reasoning: 'render preview', type: 'image', svg: SVG }] },
        undefined,
        undefined,
        { cwd: dir },
      );
      const details = result.details as { base64?: string; imagePath?: string };
      expect(result.content.some((part) => part.type === 'image')).toBe(false);
      expect(details.base64).toBeUndefined();
      expect(details.imagePath).toBeTruthy();
      expect(existsSync(details.imagePath!)).toBe(true);
    } finally {
      cleanupImplicitImageArtifacts();
      await rm(dir, { recursive: true, force: true });
    }
  });
});

// ── image authoring (browser-free svg path — always runs) ───────────────────
describe('runMediaOperation image (svg)', () => {
  let dir: string;
  beforeAll(async () => { dir = await mkdtemp(path.join(os.tmpdir(), 'create-media-')); });
  afterAll(async () => { await rm(dir, { recursive: true, force: true }); });

  it('rasterizes svg to an inline PNG', async () => {
    const r = await runMediaOperation({ type: 'image', svg: SVG }, dir);
    expect(r.ok).toBe(true);
    expect(r.mimeType).toBe('image/png');
    expect(Buffer.from(r.base64!, 'base64').subarray(0, 4).toString('latin1')).toBe('\x89PNG');
  });
  it('rejects image with neither svg nor html', async () => {
    await expect(runMediaOperation({ type: 'image' }, dir)).rejects.toThrow(/svg.*html/);
  });
  it('rejects an unknown output', async () => {
    await expect(runMediaOperation({ type: 'hologram' }, dir)).rejects.toThrow(/type/);
  });
  it('pdf requires exactly one source', async () => {
    await expect(runMediaOperation({ type: 'pdf', dest: path.join(dir, 'x.pdf'), html: '<p>a</p>', markdown: '# b' }, dir))
      .rejects.toThrow(/only one PDF source/);
  });
});

// ── external renderer/binary delegation (always mocked) ────────────────────
describe('runMediaOperation external delegation', () => {
  let dir: string;
  beforeAll(async () => {
    dir = await mkdtemp(path.join(os.tmpdir(), 'create-media-ff-'));
  });
  afterAll(async () => { await rm(dir, { recursive: true, force: true }); });

  it('delegates ffmpeg operations without executing a binary', async () => {
    const sample = path.join(dir, 'sample.mp4');
    const out = path.join(dir, 'out.gif');
    const runMedia = vi.fn(async () => ({
      ok: true,
      mode: 'gif' as const,
      message: 'mock gif',
      savedPath: out,
      bytes: 42,
      ffprobe: { streams: ['unbounded internal metadata'] },
    }));
    const r = await runMediaOperation(
      { type: 'gif', source: sample, dest: out, fps: 8, width: 120 },
      dir,
      undefined,
      { runMedia },
    );
    expect(r.ok).toBe(true);
    expect(r.savedPath).toBe(out);
    expect('ffprobe' in r).toBe(false);
    expect(runMedia).toHaveBeenCalledWith(expect.objectContaining({
      mode: 'gif', input: sample, output: out, fps: 8, width: 120,
    }), dir, undefined);
  });

  it('delegates PDF rendering without launching Chrome', async () => {
    const out = path.join(dir, 'mock.pdf');
    const renderPdf = vi.fn(async () => Buffer.from('%PDF-mock'));
    const r = await runMediaOperation(
      { type: 'pdf', dest: out, html: '<h1>mock</h1>' },
      dir,
      undefined,
      { renderPdf },
    );
    expect(r.ok).toBe(true);
    expect(renderPdf).toHaveBeenCalledOnce();
  });
});
