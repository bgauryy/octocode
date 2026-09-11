import { describe, expect, it } from 'vitest';
import { summarizeMcpBatchResult } from '../src/tools/mcp/presentation.js';
import type { ToolCallResult } from '../src/types.js';

function textResult(text: string, details?: unknown, isError = false): ToolCallResult {
  return { content: [{ type: 'text', text }], details, isError };
}

describe('MCP presentation', () => {
  it('prefers a structured summary over transport text', () => {
    expect(summarizeMcpBatchResult(
      textResult('transport wrapper', { summary: '22 matches · 3 files' }),
    )).toBe('22 matches · 3 files');
  });

  it('summarizes result counts without exposing structural wrapper keys', () => {
    expect(summarizeMcpBatchResult(textResult([
      'results:',
      'stats:',
      'totalOccurrences: 22',
      'filesMatched: 3',
    ].join('\n')))).toBe('22 matches · 3 files');
  });

  it('uses the final diagnostic line for errors', () => {
    expect(summarizeMcpBatchResult(
      textResult('[MCP_ERROR]\nSchema unavailable', undefined, true),
    )).toBe('Schema unavailable');
  });
});
