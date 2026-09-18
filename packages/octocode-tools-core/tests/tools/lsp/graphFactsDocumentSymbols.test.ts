import { afterEach, describe, expect, it, vi } from 'vitest';
import { contextUtils } from '../../../src/utils/contextUtils.js';
import { graphFactsDocumentSymbols } from '../../../src/tools/lsp/semantic_content/semanticFileOps/anchor.js';

afterEach(() => vi.restoreAllMocks());

describe('graphFactsDocumentSymbols contract decoding', () => {
  it('rejects unsupported graph-fact schemas', () => {
    vi.spyOn(contextUtils, 'extractGraphFacts').mockReturnValue(
      JSON.stringify({ schemaVersion: 2, declarations: [] })
    );

    expect(graphFactsDocumentSymbols('/tmp/example.js', '')).toBeNull();
  });

  it('preserves valid v1 declaration fallbacks', () => {
    vi.spyOn(contextUtils, 'extractGraphFacts').mockReturnValue(
      JSON.stringify({
        schemaVersion: 1,
        declarations: [
          {
            name: 'answer',
            kind: 'function',
            range: {
              start: { line: 0, character: 0 },
              end: { line: 0, character: 6 },
            },
          },
        ],
      })
    );

    expect(graphFactsDocumentSymbols('/tmp/example.js', '')).toEqual({
      diagnostics: [],
      symbols: [
        {
          name: 'answer',
          kind: 'function',
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 6 },
          },
        },
      ],
    });
  });
});
