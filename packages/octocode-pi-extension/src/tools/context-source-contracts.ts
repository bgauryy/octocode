import type { ContextSegmentV1 } from '@octocodeai/octocode-awareness/host';

export interface CurrentRehydrationSource {
  segment: ContextSegmentV1;
  content: string;
}
