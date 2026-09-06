import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';

import {
  sniffImageMime,
  loadImageForRender,
  buildImageLines,
  buildImageLinesFromData,
  appendImageLines,
  effectiveInlineImages,
  renderRuntimeCapabilitiesAddendum,
  setCapabilityCheckForTests,
  setImageVisibilityCheckForTests,
} from '../src/tools/image-render.js';
import { makeComponentRenderer } from '../src/tools/render-helpers.js';
import type { PiTheme, RenderContext } from '../src/types.js';

const theme = { fg: (c: string, t: string) => '<' + c + '>' + t + '</' + c + '>', bold: (t: string) => t } as unknown as PiTheme;

const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

function tmpDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-image-render-'));
}

function writePng(dir: string, name = 'shot.png', extraBytes = 64): string {
  const p = path.join(dir, name);
  fs.writeFileSync(p, Buffer.concat([PNG_MAGIC, Buffer.alloc(extraBytes)]));
  return p;
}

// ─── sniffImageMime ───────────────────────────────────────────────────────────

test('sniffImageMime recognizes png magic bytes', () => {
  assert.equal(sniffImageMime(Buffer.concat([PNG_MAGIC, Buffer.alloc(4)])), 'image/png');
});

test('sniffImageMime recognizes jpeg magic bytes', () => {
  assert.equal(sniffImageMime(Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0x00])), 'image/jpeg');
});

test('sniffImageMime recognizes gif87a and gif89a', () => {
  assert.equal(sniffImageMime(Buffer.from('GIF87a\x00\x00', 'latin1')), 'image/gif');
  assert.equal(sniffImageMime(Buffer.from('GIF89a\x00\x00', 'latin1')), 'image/gif');
});

test('sniffImageMime recognizes webp (RIFF....WEBP)', () => {
  const webp = Buffer.concat([
    Buffer.from('RIFF', 'latin1'),
    Buffer.from([0x10, 0x00, 0x00, 0x00]),
    Buffer.from('WEBP', 'latin1'),
    Buffer.alloc(4),
  ]);
  assert.equal(sniffImageMime(webp), 'image/webp');
});

test('sniffImageMime returns undefined for unknown or short buffers', () => {
  assert.equal(sniffImageMime(Buffer.from('not an image at all')), undefined);
  assert.equal(sniffImageMime(Buffer.from([0x89, 0x50])), undefined); // truncated png magic
  assert.equal(sniffImageMime(Buffer.alloc(0)), undefined);
  // RIFF but not WEBP (e.g. WAV)
  const wav = Buffer.concat([Buffer.from('RIFF', 'latin1'), Buffer.alloc(4), Buffer.from('WAVE', 'latin1')]);
  assert.equal(sniffImageMime(wav), undefined);
});

// ─── loadImageForRender ───────────────────────────────────────────────────────

test('loadImageForRender loads a real png file as base64 + mime', () => {
  const dir = tmpDir();
  const p = writePng(dir);
  const loaded = loadImageForRender(p);
  assert.ok(loaded);
  assert.equal(loaded.mimeType, 'image/png');
  assert.equal(Buffer.from(loaded.base64, 'base64').length, fs.statSync(p).size);
});

test('loadImageForRender rejects files larger than 4MB', () => {
  const dir = tmpDir();
  const p = path.join(dir, 'huge.png');
  fs.writeFileSync(p, Buffer.concat([PNG_MAGIC, Buffer.alloc(4 * 1024 * 1024)])); // 4MB + 8 bytes
  assert.equal(loadImageForRender(p), undefined);
});

test('loadImageForRender rejects unknown mime types', () => {
  const dir = tmpDir();
  const p = path.join(dir, 'notes.txt');
  fs.writeFileSync(p, 'plain text, definitely not an image');
  assert.equal(loadImageForRender(p), undefined);
});

test('loadImageForRender rejects missing / unreadable / empty paths', () => {
  const dir = tmpDir();
  assert.equal(loadImageForRender(path.join(dir, 'does-not-exist.png')), undefined);
  assert.equal(loadImageForRender(dir), undefined); // directory, not a file
  const empty = path.join(dir, 'empty.png');
  fs.writeFileSync(empty, Buffer.alloc(0));
  assert.equal(loadImageForRender(empty), undefined);
});

// ─── buildImageLines: placeholder fallback ────────────────────────────────────

test('buildImageLines emits a themed one-line placeholder when images unsupported', () => {
  setCapabilityCheckForTests(() => false);
  try {
    const dir = tmpDir();
    const p = writePng(dir, 'login-page.png');
    const ctx = { state: {}, invalidate() {} } as unknown as RenderContext;
    const lines = buildImageLines(ctx, p, 80, theme);
    assert.equal(lines.length, 1);
    assert.ok(lines[0].includes('🖼 image: login-page.png'));
    assert.match(lines[0], /\(\d+(\.\d+)? (B|KB|MB)\)/);
    assert.ok(lines[0].includes('<dim>')); // painted with the theme
    // No Image instance cached on the placeholder path
    assert.deepEqual(Object.keys((ctx as { state: Record<string, unknown> }).state), []);
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});

test('buildImageLines returns [] for unloadable files (no placeholder, no crash)', () => {
  setCapabilityCheckForTests(() => false);
  try {
    const dir = tmpDir();
    const ctx = { state: {}, invalidate() {} } as unknown as RenderContext;
    assert.deepEqual(buildImageLines(ctx, path.join(dir, 'missing.png'), 80, theme), []);
    assert.deepEqual(buildImageLines(undefined, path.join(dir, 'missing.png'), 80, theme), []);
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});

test('buildImageLines shows placeholder when ctx.showImages is false even if terminal supports images', () => {
  setCapabilityCheckForTests(() => true);
  try {
    const dir = tmpDir();
    const p = writePng(dir);
    const ctx = { state: {}, showImages: false, invalidate() {} } as unknown as RenderContext;
    const lines = buildImageLines(ctx, p, 80, theme);
    assert.equal(lines.length, 1);
    assert.ok(lines[0].includes('🖼 image:'));
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});

test('effectiveInlineImages requires TUI UI, protocol support, and enabled image settings', () => {
  setCapabilityCheckForTests(() => true);
  setImageVisibilityCheckForTests(() => true);
  try {
    assert.equal(effectiveInlineImages({ cwd: '/tmp', hasUI: true, mode: 'tui' }), true);
    assert.equal(effectiveInlineImages({ cwd: '/tmp', hasUI: true, mode: 'rpc' }), false);
    assert.equal(effectiveInlineImages({ cwd: '/tmp', hasUI: false, mode: 'tui' }), false);
    setImageVisibilityCheckForTests(() => false);
    assert.equal(effectiveInlineImages({ cwd: '/tmp', hasUI: true, mode: 'tui' }), false);
    setImageVisibilityCheckForTests(() => true);
    setCapabilityCheckForTests(() => false);
    assert.equal(effectiveInlineImages({ cwd: '/tmp', hasUI: true, mode: 'tui' }), false);
  } finally {
    setCapabilityCheckForTests(undefined);
    setImageVisibilityCheckForTests(undefined);
  }
});

test('runtime capability addendum exposes the same effective boolean used by tools', () => {
  setCapabilityCheckForTests(() => true);
  setImageVisibilityCheckForTests(() => false);
  try {
    const addendum = renderRuntimeCapabilitiesAddendum({ cwd: '/tmp', hasUI: true, mode: 'tui' });
    assert.match(addendum, /^<runtime_capabilities>/);
    assert.match(addendum, /effective_inline_images: false/);
    assert.match(addendum, /terminal_image_protocol_supported: true/);
    assert.match(addendum, /<\/runtime_capabilities>$/);
  } finally {
    setCapabilityCheckForTests(undefined);
    setImageVisibilityCheckForTests(undefined);
  }
});

// ─── buildImageLines: cache reuse ─────────────────────────────────────────────

test('buildImageLines caches the Image instance in ctx.state and reuses it', () => {
  setCapabilityCheckForTests(() => true);
  try {
    const dir = tmpDir();
    const p = writePng(dir);
    const state: Record<string, unknown> = {};
    const ctx = { state, invalidate() {} } as unknown as RenderContext;

    const first = buildImageLines(ctx, p, 80, theme);
    assert.ok(first.length >= 1);
    const key = `octocode-image:${p}`;
    const cached = state[key];
    assert.ok(cached, 'Image instance stored under octocode-image:<path>');

    buildImageLines(ctx, p, 80, theme);
    assert.equal(state[key], cached, 'second render reuses the same Image instance');
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});

test('buildImageLines works without ctx.state (no cache, no throw)', () => {
  setCapabilityCheckForTests(() => true);
  try {
    const dir = tmpDir();
    const p = writePng(dir);
    const lines = buildImageLines(undefined, p, 80, theme);
    assert.ok(lines.length >= 1);
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});

// ─── buildImageLinesFromData: base64 in, no disk read ──────────────────────

test('buildImageLinesFromData renders raw image lines from in-memory base64', () => {
  setCapabilityCheckForTests(() => true);
  try {
    const base64 = Buffer.concat([PNG_MAGIC, Buffer.alloc(64)]).toString('base64');
    const state: Record<string, unknown> = {};
    const ctx = { state, invalidate() {} } as unknown as RenderContext;
    const lines = buildImageLinesFromData(ctx, 'shot.png', base64, 'image/png', 80, { theme, name: 'shot.png' });
    assert.ok(lines.length >= 1);
    assert.ok(state['octocode-image:shot.png'], 'cached under octocode-image:<cacheKey>');
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});

test('buildImageLinesFromData emits a placeholder with name + bytes when unsupported', () => {
  setCapabilityCheckForTests(() => false);
  try {
    const base64 = Buffer.concat([PNG_MAGIC, Buffer.alloc(64)]).toString('base64');
    const lines = buildImageLinesFromData(undefined, 'k', base64, 'image/png', 80, { theme, name: 'diagram.png', bytes: 2048 });
    assert.equal(lines.length, 1);
    assert.ok(lines[0].includes('\ud83d\uddbc image: diagram.png'));
    assert.match(lines[0], /2\.0 KB/);
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});

test('buildImageLinesFromData returns [] for empty base64', () => {
  assert.deepEqual(buildImageLinesFromData(undefined, 'k', '', 'image/png', 80), []);
});

// ─── appendImageLines: image lines bypass width truncation ───────────────────

test('appendImageLines appends image lines after the base renderer lines, untruncated', () => {
  setCapabilityCheckForTests(() => false); // deterministic placeholder tail
  try {
    const dir = tmpDir();
    const p = writePng(dir, 'a-very-long-screenshot-file-name.png');
    const longLine = 'x'.repeat(200);
    const base = makeComponentRenderer((_props, _context) => [longLine], undefined);
    const ctx = { state: {}, invalidate() {} } as unknown as RenderContext;

    const combined = appendImageLines(base, ctx, p, theme);
    const lines = combined.render(40);
    assert.equal(lines.length, 2);
    assert.ok(lines[0].length < longLine.length, 'base line went through truncation');
    assert.ok(lines[1].includes('🖼 image:'), 'placeholder appended after base lines');
    combined.invalidate(); // must not throw; delegates to base
  } finally {
    setCapabilityCheckForTests(undefined);
  }
});
