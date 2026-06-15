import { describe, expect, it } from 'vitest';

import { attachLspEvidence } from '../src/evidence.js';

describe('attachLspEvidence', () => {
  it('marks successful result objects as answer-ready and complete', () => {
    const result = attachLspEvidence(
      { items: [1], pagination: { hasMore: false } } as {
        items: number[];
        pagination: { hasMore: boolean };
        evidence?: unknown;
      },
      { kind: 'references', paginationKey: 'pagination' }
    );

    expect(result.evidence).toEqual({
      kind: 'references',
      answerReady: true,
      complete: true,
      confidence: 'high',
    });
  });

  it('adds reasons for empty and paginated results', () => {
    const result = attachLspEvidence(
      { status: 'empty', outputPagination: { hasMore: true } } as {
        status: string;
        outputPagination: { hasMore: boolean };
        evidence?: { reason?: string };
      },
      { kind: 'calls', paginationKey: 'outputPagination' }
    );

    expect(result.evidence).toMatchObject({
      kind: 'calls',
      answerReady: false,
      complete: false,
      confidence: 'high',
    });
    expect(result.evidence?.reason).toContain('No calls were resolved');
    expect(result.evidence?.reason).toContain(
      'LSP output pagination has more data'
    );
  });

  it('covers reference-empty and result-pagination reasons', () => {
    const emptyReferences = attachLspEvidence(
      { status: 'empty', pagination: { hasMore: false } } as {
        status: string;
        pagination: { hasMore: boolean };
        evidence?: { reason?: string };
      },
      { kind: 'references', paginationKey: 'pagination' }
    );
    expect(emptyReferences.evidence?.reason).toContain(
      'No references were resolved'
    );

    const paginatedReferences = attachLspEvidence(
      { items: [1], pagination: { hasMore: true } } as {
        items: number[];
        pagination: { hasMore: boolean };
        evidence?: { reason?: string; complete?: boolean };
      },
      { kind: 'references', paginationKey: 'pagination' }
    );
    expect(paginatedReferences.evidence?.complete).toBe(false);
    expect(paginatedReferences.evidence?.reason).toContain(
      'LSP result pagination has more results'
    );
  });

  it('does not modify non-empty status envelopes', () => {
    const result = { status: 'error', message: 'failed' };
    expect(
      attachLspEvidence(result, {
        kind: 'references',
        paginationKey: 'pagination',
      })
    ).toBe(result);
    expect('evidence' in result).toBe(false);
  });
});
