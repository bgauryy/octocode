import { describe, expect, it } from 'vitest';
import { aggregatePeerEvidence } from '../../../src/utils/response/bulk.js';

// A concise query keeps its pagination cursor (display rows) even though the
// count is the complete answer. The aggregator must not let that display
// pagination flip the aggregate to incomplete when the whole bulk is concise.
function conciseQueryResult() {
  return {
    id: 'q1',
    data: {
      // per-query builder already produced a concise-correct evidence block
      evidence: { kind: 'metadata', answerReady: true, complete: true },
      pagination: { hasMore: true, totalFiles: 227 },
    },
  } as unknown as Parameters<typeof aggregatePeerEvidence>[0][number];
}

describe('aggregatePeerEvidence — concise probe gate (#3)', () => {
  it('keeps complete=true and adds no pagination reason when allConcise', () => {
    const out = aggregatePeerEvidence([conciseQueryResult()], true);
    expect(out?.answerReady).toBe(true);
    expect(out?.complete).toBe(true);
    expect(out?.reason).toBeUndefined();
  });

  it('flags incomplete via result pagination when not allConcise', () => {
    const out = aggregatePeerEvidence([conciseQueryResult()], false);
    expect(out?.complete).toBe(false);
    expect(out?.reason).toContain('Result pagination has more results.');
  });
});
