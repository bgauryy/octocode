import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { registerMediaTool, runMediaOperation } from '../src/tools/create-media-tool.js';
import { registerLocalServerTool } from '../src/tools/local-server-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { executeQueryBatch } from '../src/tools/query-envelope.js';
import { QueryBatchError } from '../src/tools/query-batch-error.js';
import { getLocalServerBaseUrl, serveDirectory, stopLocalServer } from '../src/tools/local-server.js';
import type { ToolDefinition } from '../src/types.js';

const SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2"/></svg>';
const roots: string[] = [];
function workspace(): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'tool-preflight-'));
  roots.push(root);
  return root;
}
function capture(register: typeof registerMediaTool): ToolDefinition {
  let tool: ToolDefinition | undefined;
  register({ registerTool: value => { tool = value; } }, new Set(), (pi, _names, value) => pi.registerTool?.(value));
  return tool!;
}
afterEach(() => {
  stopLocalServer();
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

describe('whole-batch preflight and partial receipts', () => {
  it.each(['sequential', 'parallel'] as const)('preserves first-failure evidence in a %s batch with no successful queries', async queryRunType => {
    let registered: ToolDefinition | undefined;
    const evidence = `diagnostic-header\n${'evidence-'.repeat(8_000)}\nfailed`;
    registerUniqueTool({ registerTool: value => { registered = value; } }, new Set(), {
      name: 'batchFixture', label: 'Batch fixture', description: 'fixture', parameters: {},
      execute: async (toolCallId, raw) => executeQueryBatch({
        toolCallId, raw, allowParallel: true,
        execute: async () => ({ isError: true, content: [{ type: 'text', text: evidence }] }),
      }),
    });
    const error = await registered!.execute('first-failure', {
      queries: [{ reasoning: 'read evidence' }], queryRunType,
    }).catch(error => error);
    expect(error).toBeInstanceOf(QueryBatchError);
    expect(error.completedCount).toBe(0);
    expect(error.message.includes(evidence)).toBe(true);
  });

  it('rejects a malformed later media operation before writing the first artifact', async () => {
    const cwd = workspace();
    const tool = capture(registerMediaTool);
    await expect(tool.execute('media-preflight', { queries: [
      { reasoning: 'render first', type: 'image', svg: SVG, dest: 'first.png' },
      { reasoning: 'invalid PDF', type: 'pdf', dest: 'nested/invalid.pdf' },
    ] }, undefined, undefined, { cwd })).rejects.toThrow(/queries\[1\].*preflight/);
    expect(fs.existsSync(path.join(cwd, 'first.png'))).toBe(false);
    expect(fs.existsSync(path.join(cwd, 'nested'))).toBe(false);
  });

  it('protects an existing image unless overwrite is explicit', async () => {
    const cwd = workspace();
    const dest = path.join(cwd, 'existing.png');
    fs.writeFileSync(dest, 'original');
    await expect(runMediaOperation({ type: 'image', svg: SVG, dest }, cwd)).rejects.toThrow(/exists|overwrite/);
    expect(fs.readFileSync(dest, 'utf8')).toBe('original');
    await expect(runMediaOperation({ type: 'image', svg: SVG, dest, overwrite: true }, cwd)).resolves.toMatchObject({ ok: true });
    expect(fs.readFileSync(dest).subarray(1, 4).toString()).toBe('PNG');
  });

  it('validates later server operations before stopping an existing mount', async () => {
    const cwd = workspace();
    await serveDirectory('fixture', cwd);
    const original = getLocalServerBaseUrl();
    expect(original).toBeTruthy();
    const tool = capture(registerLocalServerTool);
    await expect(tool.execute('server-preflight', { queries: [
      { reasoning: 'stop server', action: 'stop' },
      { reasoning: 'missing mount', action: 'serve', dir: cwd },
    ] }, undefined, undefined, { cwd })).rejects.toThrow(/queries\[1\].*preflight/);
    expect(getLocalServerBaseUrl()).toBe(original);
  });

  it('retains completed child evidence in the thrown host error when a later query fails', async () => {
    let registered: ToolDefinition | undefined;
    registerUniqueTool({ registerTool: value => { registered = value; } }, new Set(), {
      name: 'batchFixture', label: 'Batch fixture', description: 'fixture', parameters: {},
      execute: async (toolCallId, raw, signal, onUpdate, ctx) => executeQueryBatch({
        toolCallId, raw, signal, onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: import('../src/types.js').ToolCallResult) => void : undefined, ctx,
        execute: async (_query, index) => {
          if (index === 1) throw new Error('second failed');
          return { content: [{ type: 'text', text: 'header\nimportant complete evidence' }] };
        },
      }),
    });
    const result = await registered!.execute('partial-receipt', { queries: [
      { reasoning: 'read first' }, { reasoning: 'read second' }, { reasoning: 'never read third' },
    ] }).catch(error => error);
    expect(result).toBeInstanceOf(QueryBatchError);
    expect(result.message).toContain('header\nimportant complete evidence');
    expect(result.rows).toMatchObject([
      { index: 0, status: 'success' }, { index: 1, status: 'failed' }, { index: 2, status: 'not-run' },
    ]);
  });

  it('preserves complete evidence and images in host errors', () => {
    const evidence = `complete-${'x'.repeat(40_000)}-evidence`;
    const error = new QueryBatchError(1, 1, new Error('later failure'), [{
      index: 0, reasoning: 'first', status: 'success', summary: 'completed', content: [
        { type: 'text', text: evidence },
        { type: 'image', mimeType: 'image/png', data: Buffer.from('image evidence').toString('base64') },
      ],
    }, { index: 1, reasoning: 'second', status: 'failed', summary: 'later failure' }]);
    expect(error.withHostReceipt()).toBe(error);
    const message = error.message;
    expect(message).toContain(evidence);
    expect(message).toContain(Buffer.from('image evidence').toString('base64'));
    expect(error.withHostReceipt().message).toBe(message);
  });
});
