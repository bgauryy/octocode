import type { ContextSegmentV1 } from '../runtime/continuity-contracts.js';

export interface CurrentRehydrationSource {
  segment: ContextSegmentV1;
  content: string;
}
