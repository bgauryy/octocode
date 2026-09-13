import { expect, it } from 'vitest';
import { ToolResultError } from '../src/tools/tool-result-error.js';

it('preserves complete failure evidence including unsupported blocks in the host error channel', () => {
  const text = 'failure evidence '.repeat(5_000);
  const image = { type: 'image' as const, data: 'aGVsbG8=', mimeType: 'image/png' };
  const result = { isError: true, content: [{ type: 'text' as const, text }, image] };
  const error = new ToolResultError(result, 'MCPTool');
  expect(error.message).toContain(text);
  expect(error.message).toContain(JSON.stringify(image));
  expect(error.result).toBe(result);
});
