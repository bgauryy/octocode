import { describe, expect, it } from 'vitest';
import { attachLspEvidence as attachLspEvidenceImpl } from '../../src/lsp/evidence.js';

type AttachedEvidence = {
  kind: 'calls' | 'references';
  answerReady: boolean;
  complete: boolean;
  confidence: 'high';
  reason?: string;
};

const attachLspEvidence = (
  result: Record<string, unknown>,
  opts: {
    kind: 'calls' | 'references';
    paginationKey: 'pagination' | 'outputPagination';
  }
) =>
  attachLspEvidenceImpl<
    Record<string, unknown> & { evidence?: AttachedEvidence }
  >(result, opts);

describe('attachLspEvidence', () => {
  it('marks semantic success complete when pagination is exhausted', () => {
    const result = attachLspEvidence(
      {
        incomingCalls: [],
        outputPagination: { hasMore: false },
      },
      {
        kind: 'calls',
        paginationKey: 'outputPagination',
      }
    );

    expect(result.evidence).toEqual({
      kind: 'calls',
      answerReady: true,
      complete: true,
      confidence: 'high',
    });
  });

  it('adds a reason when semantic output pagination has more data', () => {
    const result = attachLspEvidence(
      {
        incomingCalls: [],
        outputPagination: { hasMore: true },
      },
      {
        kind: 'calls',
        paginationKey: 'outputPagination',
      }
    );

    expect(result.evidence).toEqual({
      kind: 'calls',
      answerReady: true,
      complete: false,
      confidence: 'high',
      reason: 'LSP output pagination has more data.',
    });
  });

  it('adds only the pagination reason for references (always high confidence)', () => {
    const result = attachLspEvidence(
      {
        locations: [],
        pagination: { hasMore: true },
      },
      {
        kind: 'references',
        paginationKey: 'pagination',
      }
    );

    expect(result.evidence).toEqual({
      kind: 'references',
      answerReady: true,
      complete: false,
      confidence: 'high',
      reason: 'LSP result pagination has more results.',
    });
  });

  it('adds a reason when the LSP result is empty', () => {
    const result = attachLspEvidence(
      {
        status: 'empty' as const,
        data: { errorCode: 'SYMBOL_NOT_FOUND' },
      },
      {
        kind: 'references',
        paginationKey: 'pagination',
      }
    );

    expect(result.evidence).toEqual({
      kind: 'references',
      answerReady: false,
      complete: false,
      confidence: 'high',
      reason:
        'No references were resolved for the supplied symbol and line hint.',
    });
  });

  it('produces calls-specific empty reason when kind is calls', () => {
    const result = attachLspEvidence(
      { status: 'empty' as const, data: {} },
      { kind: 'calls', paginationKey: 'pagination' }
    );
    expect(result.evidence?.reason).toBe(
      'No calls were resolved for the supplied symbol and line hint.'
    );
  });
});
