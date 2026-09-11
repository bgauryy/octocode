export type EvidenceAuthority = 'live' | 'demoted' | 'retired';

export interface EvidenceAuthorityInput {
  observedAt: number;
  now: number;
  staleAfterMs?: number;
  sourceExists?: boolean;
  observedGeneration?: number | string;
  currentGeneration?: number | string;
  observedFingerprint?: string;
  currentFingerprint?: string;
}

export interface EvidenceAuthorityDecision {
  authority: EvidenceAuthority;
  ambient: boolean;
  preserveCoordinates: boolean;
  mayBlock: boolean;
  recheckRequired: boolean;
  reason?: string;
}

const LIVE: EvidenceAuthorityDecision = {
  authority: 'live',
  ambient: true,
  preserveCoordinates: true,
  mayBlock: true,
  recheckRequired: false,
};

function demoted(reason: string): EvidenceAuthorityDecision {
  return {
    authority: 'demoted',
    ambient: true,
    preserveCoordinates: false,
    mayBlock: false,
    recheckRequired: true,
    reason,
  };
}

export function classifyEvidenceAuthority(input: EvidenceAuthorityInput): EvidenceAuthorityDecision {
  if (input.sourceExists === false) {
    return {
      authority: 'retired',
      ambient: false,
      preserveCoordinates: false,
      mayBlock: false,
      recheckRequired: false,
      reason: 'evidence target is no longer available',
    };
  }
  if (
    input.observedGeneration !== undefined
    && input.currentGeneration !== undefined
    && input.observedGeneration !== input.currentGeneration
  ) return demoted('evidence generation changed');
  if (
    input.observedFingerprint !== undefined
    && input.currentFingerprint !== undefined
    && input.observedFingerprint !== input.currentFingerprint
  ) return demoted('evidence fingerprint changed');
  if (
    input.staleAfterMs !== undefined
    && Math.max(0, input.now - input.observedAt) > input.staleAfterMs
  ) return demoted('evidence observation expired');
  return { ...LIVE };
}
