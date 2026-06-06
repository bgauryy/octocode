import { describe, expect, it } from 'vitest';
import { aggregatePeerEvidence } from '../../../src/utils/response/bulk.js';

function queryResultWithPagination() {
  return {
    id: 'q1',
    data: {
      evidence: { kind: 'metadata', answerReady: true, complete: true },
      pagination: { hasMore: true, totalFiles: 227 },
    },
  } as unknown as Parameters<typeof aggregatePeerEvidence>[0][number];
}

function queryResultComplete() {
  return {
    id: 'q1',
    data: {
      evidence: { kind: 'metadata', answerReady: true, complete: true },
    },
  } as unknown as Parameters<typeof aggregatePeerEvidence>[0][number];
}

describe('aggregatePeerEvidence — pagination completeness gate', () => {
  it('keeps complete=true when no pagination hasMore', () => {
    const out = aggregatePeerEvidence([queryResultComplete()]);
    expect(out?.answerReady).toBe(true);
    expect(out?.complete).toBe(true);
    expect(out?.reason).toBeUndefined();
  });

  it('flags incomplete via result pagination when pagination hasMore=true', () => {
    const out = aggregatePeerEvidence([queryResultWithPagination()]);
    expect(out?.complete).toBe(false);
    expect(out?.reason).toContain('Result pagination has more results.');
  });
});
