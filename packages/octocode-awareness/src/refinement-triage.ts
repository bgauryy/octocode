import type { RefinementRecord } from './types/identity-memory.js';

export type RefinementDestinationTriage =
  | { destination: 'message'; reason: 'HANDOFF_CONTINUATION' }
  | { destination: 'work'; reason: 'ACTIONABLE_FILE_WORK' }
  | { destination: 'memory'; reason: 'VERIFIED_REUSABLE_LESSON' }
  | { destination: 'blocked'; reason:
      | 'INSTRUCTIONS_REQUIRE_AUTHORITY_DESTINATION'
      | 'MIXED_ACTION_AND_MEMORY_PAYLOAD'
      | 'INSUFFICIENT_DESTINATION_EVIDENCE' };

/**
 * Lossless triage for copy-on-write migration. Refuse to guess when a legacy
 * refinement combines responsibilities or lacks evidence for one destination.
 */
export function triageRefinementDestination(
  refinement: Pick<RefinementRecord, 'quality' | 'state' | 'files' | 'remember'>,
): RefinementDestinationTriage {
  const hasFiles = refinement.files.length > 0;
  const hasMemory = refinement.remember.trim().length > 0;
  if (refinement.quality === 'handoff') return { destination: 'message', reason: 'HANDOFF_CONTINUATION' };
  if (refinement.quality === 'instructions') {
    return { destination: 'blocked', reason: 'INSTRUCTIONS_REQUIRE_AUTHORITY_DESTINATION' };
  }
  if (hasFiles && hasMemory) return { destination: 'blocked', reason: 'MIXED_ACTION_AND_MEMORY_PAYLOAD' };
  if (refinement.state === 'done' && hasMemory) {
    return { destination: 'memory', reason: 'VERIFIED_REUSABLE_LESSON' };
  }
  if (refinement.state !== 'done' && hasFiles) return { destination: 'work', reason: 'ACTIONABLE_FILE_WORK' };
  return { destination: 'blocked', reason: 'INSUFFICIENT_DESTINATION_EVIDENCE' };
}
