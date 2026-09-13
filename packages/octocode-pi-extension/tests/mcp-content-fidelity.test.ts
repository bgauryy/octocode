import { expect, test } from 'vitest';
import { resolveMcpCallContent, resolveMcpCallTable } from '../src/tools/mcp/sanitize.js';

test('equivalent JSON text and structured content are delivered once', () => {
  const content = [{ type: 'text' as const, text: '{"b":2,"a":1}' }];
  expect(resolveMcpCallContent({ content, structuredContent: { a: 1, b: 2 } })).toEqual(content);
});

test('ordinary MCP text does not hide structured evidence or continuations', () => {
  const structuredContent = { evidence: 'x'.repeat(80_000), next: { tool: 'search', arguments: { cursor: 'page-2' } } };
  const content = resolveMcpCallContent({ content: [{ type: 'text', text: 'Summary' }], structuredContent });
  expect(content[0]).toEqual({ type: 'text', text: 'Summary' });
  expect(JSON.parse((content[1] as { text: string }).text)).toEqual(structuredContent);
});

test('table view adds a summary without replacing complete MCP content', () => {
  const payload = {
    content: [{ type: 'image', data: 'image-data', mimeType: 'image/png' }],
    structuredContent: { results: [{ data: { path: 'a.ts', body: 'x'.repeat(80_000), next: { cursor: 'page-2' } } }] },
  };
  expect(resolveMcpCallTable(payload)?.slice(1)).toEqual(resolveMcpCallContent(payload));
});
