import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';
import { getPatchedToolMetadata } from './descriptionOverrides.js';

let _cached: CompleteMetadata | null = null;

function ensureLoaded(): CompleteMetadata {
  if (!_cached) {
    _cached = getPatchedToolMetadata(completeMetadata);
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
