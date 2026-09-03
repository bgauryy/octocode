import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  budgetToolResult,
  MODEL_VISIBLE_TOOL_RESULT_MAX_CHARS,
  MODEL_VISIBLE_TOOL_RESULT_MAX_IMAGES,
} from '../src/tools/tool-result-budget.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach((root) => fs.rmSync(root, { recursive: true, force: true })));

describe('model-visible tool result budget', () => {
  it('preserves small results byte-for-byte', () => {
    const result = { content: [{ type: 'text' as const, text: 'small' }], details: { exact: true } };
    expect(budgetToolResult(result, { toolCallId: 'small', toolName: 'demo' })).toBe(result);
  });

  it('caps large text and preserves the full body in a private session artifact', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'tool-result-budget-'));
    roots.push(workspace);
    const ctx = { cwd: workspace, sessionManager: { getSessionId: () => 'budget-session' } };
    const original = `head-${'x'.repeat(80_000)}-tail`;
    const result = budgetToolResult({
      content: [{ type: 'text', text: original }],
      details: { renderer: 'unchanged' },
    }, { ctx, toolCallId: 'call/unsafe', toolName: 'MCPTool' });
    const visible = result.content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('');
    expect(visible.length).toBeLessThanOrEqual(MODEL_VISIBLE_TOOL_RESULT_MAX_CHARS);
    expect(visible).toMatch(/tool output truncated/i);
    const spillPath = visible.match(/full text=([^;\]]+)/)?.[1];
    expect(spillPath).toBeTruthy();
    expect(fs.readFileSync(spillPath!, 'utf8')).toBe(original);
    expect(result.details).toEqual({ renderer: 'unchanged' });
  });

  it('keeps the image cap lossless by spilling every omitted image and a manifest', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'tool-result-images-'));
    roots.push(workspace);
    const ctx = { cwd: workspace, sessionManager: { getSessionId: () => 'image-session' } };
    const images = Array.from({ length: MODEL_VISIBLE_TOOL_RESULT_MAX_IMAGES + 2 }, (_, index) => ({
      type: 'image' as const,
      mimeType: index % 2 === 0 ? 'image/png' : 'image/jpeg',
      data: Buffer.from(`image-${index}`).toString('base64'),
    }));
    const result = budgetToolResult({ content: images }, {
      ctx,
      toolCallId: 'many-images',
      toolName: 'MCPTool',
    });
    expect(result.content.filter((part) => part.type === 'image')).toHaveLength(MODEL_VISIBLE_TOOL_RESULT_MAX_IMAGES);
    const notice = result.content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('');
    const manifestPath = notice.match(/image manifest=([^;\]]+)/)?.[1];
    expect(manifestPath).toBeTruthy();
    const manifest = JSON.parse(fs.readFileSync(manifestPath!, 'utf8')) as {
      images: Array<{ path: string; mimeType: string; bytes: number }>;
    };
    expect(manifest.images).toHaveLength(2);
    expect(manifest.images.map((entry) => fs.readFileSync(entry.path, 'utf8'))).toEqual(['image-4', 'image-5']);
  });

  it('reuses existing image artifacts instead of duplicating omitted media bytes', () => {
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'tool-result-existing-images-'));
    roots.push(workspace);
    const ctx = { cwd: workspace, sessionManager: { getSessionId: () => 'existing-image-session' } };
    const paths = Array.from({ length: 6 }, (_, index) => {
      const file = path.join(workspace, `existing-${index}.png`);
      fs.writeFileSync(file, `existing-image-${index}`);
      return file;
    });
    const result = budgetToolResult({
      content: paths.map((_, index) => ({
        type: 'image' as const,
        mimeType: 'image/png',
        data: Buffer.from(`existing-image-${index}`).toString('base64'),
      })),
      details: {
        queryRunType: 'parallel',
        results: paths.map((imagePath) => ({ result: { imagePath } })),
      },
    }, { ctx, toolCallId: 'reuse-images', toolName: 'media' });
    const notice = result.content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('');
    const manifestPath = notice.match(/image manifest=([^;\]]+)/)?.[1];
    const manifest = JSON.parse(fs.readFileSync(manifestPath!, 'utf8')) as { images: Array<{ path: string }> };
    expect(manifest.images.map((entry) => entry.path)).toEqual(paths.slice(4).map((file) => fs.realpathSync.native(file)));
    expect(fs.readdirSync(path.dirname(manifestPath!)).filter((name) => /-image-/.test(name))).toEqual([]);
  });
});
