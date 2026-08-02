import { describe, expect, it } from 'vitest';

import { referencesEnvelope } from '../../../src/tools/lsp/semantic_content/semanticEnvelopes/locationEnvelopes.js';
import type {
  SymbolAnchoredSemanticQuery,
  SymbolAnchor,
} from '../../../src/tools/lsp/shared/semanticTypes.js';

const URI = '/repo/src/toolNames.ts';

function makeQuery(): SymbolAnchoredSemanticQuery {
  return {
    id: 'q1',
    uri: URI,
    type: 'references',
    symbolName: 'isLocalTool',
    lineHint: 37,
  } as SymbolAnchoredSemanticQuery;
}

function makeAnchor(): SymbolAnchor {
  return {
    uri: URI,
    resolvedSymbol: {
      name: 'isLocalTool',
      foundAtLine: 37,
      position: { line: 36, character: 16 },
    },
  } as unknown as SymbolAnchor;
}

function location(line: number, character: number, uri = URI) {
  return {
    uri,
    range: {
      start: { line, character },
      end: { line, character: character + 11 },
    },
    snippet: 'export function isLocalTool(',
  } as never;
}

describe('references definition-only typed signal', () => {
  it('marks payload.definitionOnly when the only reference is the definition itself', () => {
    // A def-only result is NOT proof of absence (index scope may be narrow, or
    // the symbol may be public API). Warnings are stripped from responses, so
    // the honesty signal must be a typed payload field.
    const envelope = referencesEnvelope(makeQuery(), makeAnchor(), [
      location(36, 16),
    ]);
    const payload = envelope.payload as Record<string, unknown>;
    expect(payload.totalReferences).toBe(1);
    expect(payload.definitionOnly).toBe(true);
  });

  it('omits definitionOnly when real external references exist', () => {
    const envelope = referencesEnvelope(makeQuery(), makeAnchor(), [
      location(36, 16),
      location(12, 4, '/repo/src/other.ts'),
    ]);
    const payload = envelope.payload as Record<string, unknown>;
    expect(payload.definitionOnly).toBeUndefined();
  });
});
