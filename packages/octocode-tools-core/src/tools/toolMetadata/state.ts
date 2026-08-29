import { localCompleteMetadata } from '../../toolContract/metadata.js';
import type { LocalCompleteMetadata } from '../../toolContract/types.js';

let _cached: LocalCompleteMetadata | null = null;

function ensureLoaded(): LocalCompleteMetadata {
  if (!_cached) {
    _cached = localCompleteMetadata;
  }
  return _cached;
}

export async function loadToolContent(): Promise<LocalCompleteMetadata> {
  return ensureLoaded();
}

export async function initializeToolMetadata(): Promise<void> {
  ensureLoaded();
}

export function getMetadataOrNull(): LocalCompleteMetadata | null {
  return _cached;
}

export function _resetMetadataState(): void {
  _cached = null;
}
