import { describe, expect, it } from 'vitest';

import { withSemanticNext } from '../../../src/tools/lsp/semantic_content/semanticPresentation.js';
import { failedAnchorEnvelope } from '../../../src/tools/lsp/semantic_content/semanticEnvelopes.js';
import type {
  LspGetSemanticsQuery,
  LspSemanticEnvelope,
} from '../../../src/tools/lsp/shared/semanticTypes.js';

/**
 * The tool description promises: "Empty/incomplete: re-anchor or fall back to
 * localSearchCode." withSemanticNext must emit that structured fallback for
 * empty results that carry a symbolName.
 */
describe('withSemanticNext — empty-state fallback', () => {
  const symbolNotFound = (
    type: 'definition' | 'references'
  ): { query: LspGetSemanticsQuery; result: LspSemanticEnvelope } => {
    const query = {
      type,
      uri: 'src/foo.ts',
      symbolName: 'doThing',
      lineHint: 10,
    } as LspGetSemanticsQuery;
    const result = failedAnchorEnvelope(
      query,
      'Could not find symbol "doThing"'
    );
    expect(result.payload).toMatchObject({
      kind: 'empty',
      category: 'symbolNotFound',
    });
    return { query, result };
  };

  it('points a symbolNotFound definition to localSearchCode with a STRING keyword', () => {
    const { query, result } = symbolNotFound('definition');
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;

    const textSearch = withNext.next?.textSearch;
    expect(textSearch?.tool).toBe('localSearchCode');
    expect(textSearch?.query.keywords).toBe('doThing');
    // Regression guard: keywords must be a string, not an array.
    expect(Array.isArray(textSearch?.query.keywords)).toBe(false);
    expect(typeof textSearch?.query.keywords).toBe('string');
  });

  it('adds a re-anchor hint for symbolNotFound (references)', () => {
    const { query, result } = symbolNotFound('references');
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;

    expect(withNext.next?.textSearch?.query.keywords).toBe('doThing');
    const reAnchor = withNext.next?.reAnchor;
    expect(reAnchor?.tool).toBe('lspGetSemantics');
    expect(reAnchor?.query).toMatchObject({
      type: 'documentSymbols',
      uri: 'src/foo.ts',
    });
  });

  it('emits no fallback when there is no symbolName to search for', () => {
    const query = {
      type: 'documentSymbols',
      uri: 'src/foo.ts',
    } as LspGetSemanticsQuery;
    const result = failedAnchorEnvelope(query, 'anchor failed');
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    expect(withNext.next).toBeUndefined();
  });

  it('still emits readSite (not the fallback) when a location is present', () => {
    const query = {
      type: 'definition',
      uri: 'src/foo.ts',
      symbolName: 'doThing',
      lineHint: 10,
    } as LspGetSemanticsQuery;
    const result: LspSemanticEnvelope = {
      type: 'definition',
      uri: 'src/foo.ts',
      lsp: { serverAvailable: true },
      payload: {
        kind: 'definition',
        locations: [
          {
            uri: 'file:///abs/foo.ts',
            displayRange: { startLine: 5, endLine: 5 },
          },
        ],
      },
    };
    const withNext = withSemanticNext(query, result) as LspSemanticEnvelope;
    expect(withNext.next?.readSite?.tool).toBe('localGetFileContent');
    expect(withNext.next?.textSearch).toBeUndefined();
  });
});
