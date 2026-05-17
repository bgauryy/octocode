import { describe, expect, it } from 'vitest';
import {
  parseRipgrepJsonOutput,
  extractFunctionBody,
} from '../../src/tools/lsp_call_hierarchy/callHierarchyPatterns.js';

describe('lsp_call_hierarchy/callHierarchyPatterns', () => {
  it('parses valid ripgrep JSON lines and skips invalid lines', () => {
    const input = [
      JSON.stringify({
        type: 'match',
        data: {
          path: { text: '/workspace/src/a.ts' },
          line_number: 12,
          lines: { text: 'foo(bar)\n' },
          submatches: [{ start: 4, end: 7, match: { text: 'bar' } }],
        },
      }),
      '{invalid-json',
      JSON.stringify({ type: 'stats', data: {} }),
    ].join('\n');

    const results = parseRipgrepJsonOutput(input);
    expect(results).toHaveLength(1);
    expect(results[0]).toEqual({
      filePath: '/workspace/src/a.ts',
      lineNumber: 12,
      column: 4,
      lineContent: 'foo(bar)\n',
    });
  });

  it('extracts function body and handles nested braces', () => {
    const lines = [
      'export function sample() {',
      '  if (ok) {',
      '    doWork();',
      '  }',
      '  return true;',
      '}',
    ];

    const extracted = extractFunctionBody(lines, 0);
    expect(extracted).not.toBeNull();
    expect(extracted?.startLine).toBe(0);
    expect(extracted?.lines.join('\n')).toContain('doWork();');
    expect(extracted?.lines.join('\n')).toContain('return true;');
  });

  it('returns null when no opening brace is found near start line', () => {
    const lines = ['const value = 1;', 'const next = 2;', 'const last = 3;'];
    expect(extractFunctionBody(lines, 0)).toBeNull();
  });
});
