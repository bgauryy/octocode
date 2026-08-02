import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

let _cached: CompleteMetadata | null = null;

// Provenance contract: tool descriptions, schemas, and the system prompt come
// from @octocodeai/octocode-core (src/resources) VERBATIM — no interface- or
// brain-level patching. Fix stale prose in octocode-core, never here.
// (A temporary descriptionOverrides patch layer lived here until 2026-08;
// guarded against regression by localFindFilesDescription.test.ts.)
function ensureLoaded(): CompleteMetadata {
  if (!_cached) {
    _cached = completeMetadata;
  }
  return _cached;
}

export async function loadToolContent(): Promise<CompleteMetadata> {
  return ensureLoaded();
}

export async function initializeToolMetadata(): Promise<void> {
  ensureLoaded();
}

export function getMetadataOrNull(): CompleteMetadata | null {
  return _cached;
}

export function _resetMetadataState(): void {
  _cached = null;
}
