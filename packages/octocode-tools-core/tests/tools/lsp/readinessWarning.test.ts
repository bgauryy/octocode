import { describe, expect, it } from 'vitest';

import {
  attachReadinessWarning,
  zeroResultEmptyCategory,
} from '../../../src/tools/lsp/shared/readiness.js';
import type { LspSemanticEnvelope } from '../../../src/tools/lsp/shared/semanticTypes.js';

function emptyReferencesEnvelope(): LspSemanticEnvelope {
  return {
    type: 'references',
    uri: 'src/foo.ts',
    lsp: { serverAvailable: true, provider: 'referencesProvider', source: 'lsp' },
    payload: {
      kind: 'references',
      locations: [],
      totalReferences: 0,
      totalFiles: 0,
      empty: {
        category: 'noReferences',
        reason: 'referencesProvider returned no references',
      },
    },
  };
}

function emptyDefinitionEnvelope(): LspSemanticEnvelope {
  return {
    type: 'definition',
    uri: 'src/foo.ts',
    lsp: { serverAvailable: true, provider: 'definitionProvider' },
    payload: {
      kind: 'empty',
      category: 'noLocations',
      reason: 'definitionProvider returned no locations',
    },
  };
}

function nonEmptyDefinitionEnvelope(): LspSemanticEnvelope {
  return {
    type: 'definition',
    uri: 'src/foo.ts',
    lsp: { serverAvailable: true, provider: 'definitionProvider' },
    payload: {
      kind: 'definition',
      locations: ['src/foo.ts:10-12 def'],
    },
  };
}

function unsupportedEnvelope(): LspSemanticEnvelope {
  return {
    type: 'references',
    uri: 'src/foo.ts',
    lsp: { serverAvailable: true, provider: 'referencesProvider' },
    payload: {
      kind: 'empty',
      category: 'unsupportedOperation',
      reason: 'referencesProvider unsupported',
    },
  };
}

describe('zeroResultEmptyCategory', () => {
  it('reads a nested empty on an otherwise-shaped payload (references)', () => {
    expect(zeroResultEmptyCategory(emptyReferencesEnvelope())).toBe(
      'noReferences'
    );
  });

  it('reads the category of an empty-kind payload (definition)', () => {
    expect(zeroResultEmptyCategory(emptyDefinitionEnvelope())).toBe(
      'noLocations'
    );
  });

  it('returns undefined when the envelope carries real results', () => {
    expect(zeroResultEmptyCategory(nonEmptyDefinitionEnvelope())).toBeUndefined();
  });
});

describe('attachReadinessWarning', () => {
  it('adds a caveat to an empty relation query when readiness is timeout', () => {
    const result = attachReadinessWarning(emptyReferencesEnvelope(), 'timeout');
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings?.[0]).toContain('readiness: timeout');
    expect(result.warnings?.[0]).toContain('not yet indexed');
  });

  it('adds a caveat when readiness is settledFallback', () => {
    const result = attachReadinessWarning(
      emptyDefinitionEnvelope(),
      'settledFallback'
    );
    expect(result.warnings?.[0]).toContain('readiness: settledFallback');
  });

  it('leaves the envelope untouched when readiness is progressIdle', () => {
    const envelope = emptyReferencesEnvelope();
    const result = attachReadinessWarning(envelope, 'progressIdle');
    expect(result).toBe(envelope);
    expect(result.warnings).toBeUndefined();
  });

  it('leaves the envelope untouched when readiness is undefined (wait skipped)', () => {
    const envelope = emptyReferencesEnvelope();
    const result = attachReadinessWarning(envelope, undefined);
    expect(result).toBe(envelope);
    expect(result.warnings).toBeUndefined();
  });

  it('does not caveat a non-empty result even when readiness is timeout', () => {
    const envelope = nonEmptyDefinitionEnvelope();
    const result = attachReadinessWarning(envelope, 'timeout');
    expect(result).toBe(envelope);
    expect(result.warnings).toBeUndefined();
  });

  it('does not caveat an unsupportedOperation empty (not indexing-related)', () => {
    const envelope = unsupportedEnvelope();
    const result = attachReadinessWarning(envelope, 'timeout');
    expect(result).toBe(envelope);
    expect(result.warnings).toBeUndefined();
  });

  it('preserves any pre-existing warnings', () => {
    const envelope = emptyReferencesEnvelope();
    envelope.warnings = ['existing warning'];
    const result = attachReadinessWarning(envelope, 'timeout');
    expect(result.warnings).toHaveLength(2);
    expect(result.warnings?.[0]).toBe('existing warning');
  });
});
