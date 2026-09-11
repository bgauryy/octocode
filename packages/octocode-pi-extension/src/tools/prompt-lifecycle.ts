import type { ContextSegmentV1 } from '@octocodeai/octocode-awareness/host';

export type PromptPlacement = 'frozen-system' | 'versioned-system' | 'turn-context' | 'transcript';
export interface PromptLifecycleRuleV1 {
  version: 1;
  placement: PromptPlacement;
  mutable: boolean;
  delivery: 'once' | 'on-change' | 'on-trigger';
  restoration: 'reload-owner' | 'attributed-turn-context' | 'transcript-owned';
  defaultTokenBudget: number;
  reason: string;
}

export const PROMPT_LIFECYCLE_MATRIX: Readonly<Record<ContextSegmentV1['kind'], PromptLifecycleRuleV1>> = Object.freeze({
  'product-policy': { version: 1, placement: 'frozen-system', mutable: false, delivery: 'once', restoration: 'reload-owner', defaultTokenBudget: 20_000, reason: 'cache-stable harness authority' },
  'project-instruction': { version: 1, placement: 'versioned-system', mutable: true, delivery: 'on-change', restoration: 'reload-owner', defaultTokenBudget: 20_000, reason: 'applicable host repository instructions' },
  'tool-contract': { version: 1, placement: 'versioned-system', mutable: true, delivery: 'on-change', restoration: 'reload-owner', defaultTokenBudget: 30_000, reason: 'effective capability revision at the turn boundary' },
  skill: { version: 1, placement: 'versioned-system', mutable: true, delivery: 'on-trigger', restoration: 'attributed-turn-context', defaultTokenBudget: 20_000, reason: 'inventory resolves by revision; selected bodies reload from their owner' },
  plan: { version: 1, placement: 'turn-context', mutable: true, delivery: 'on-change', restoration: 'attributed-turn-context', defaultTokenBudget: 15_000, reason: 'durable domain state can evolve' },
  'memory-lead': { version: 1, placement: 'turn-context', mutable: true, delivery: 'on-trigger', restoration: 'attributed-turn-context', defaultTokenBudget: 4_000, reason: 'retrieval is attributed evidence' },
  'tool-result': { version: 1, placement: 'transcript', mutable: false, delivery: 'on-trigger', restoration: 'attributed-turn-context', defaultTokenBudget: 12_000, reason: 'selected results reload as attributed call data' },
  'peer-event': { version: 1, placement: 'turn-context', mutable: true, delivery: 'on-trigger', restoration: 'attributed-turn-context', defaultTokenBudget: 4_000, reason: 'inbound data is policy-filtered before delivery' },
  'user-request': { version: 1, placement: 'transcript', mutable: false, delivery: 'on-trigger', restoration: 'attributed-turn-context', defaultTokenBudget: 8_000, reason: 'operator input remains attributable' },
});

export function promptLifecycleFor(kind: ContextSegmentV1['kind']): PromptLifecycleRuleV1 {
  return PROMPT_LIFECYCLE_MATRIX[kind];
}

export function canReprojectRehydratedSegment(segment: ContextSegmentV1): boolean {
  return segment.rehydrate === 'always'
    && segment.visibility !== 'hidden-policy'
    && (promptLifecycleFor(segment.kind).restoration === 'attributed-turn-context'
      || (segment.kind === 'tool-contract' && segment.authority !== 'product' && segment.scope !== 'session'));
}
