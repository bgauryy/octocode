import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

let METADATA_JSON: CompleteMetadata | null = null;

export async function getMetadata(): Promise<CompleteMetadata> {
  return completeMetadata;
}

export function getMetadataOrThrow(): CompleteMetadata {
  if (!METADATA_JSON) {
    throw new Error(
      'Tool metadata not initialized. Call and await initializeToolMetadata() before using tool metadata.'
    );
  }
  return METADATA_JSON;
}

export function getMetadataOrNull(): CompleteMetadata | null {
  return METADATA_JSON;
}

export async function initializeToolMetadata(): Promise<void> {
  if (METADATA_JSON) return;
  METADATA_JSON = completeMetadata;
}

export async function loadToolContent(): Promise<CompleteMetadata> {
  if (!METADATA_JSON) await initializeToolMetadata();
  return getMetadataOrThrow();
}

export function _resetMetadataState(): void {
  METADATA_JSON = null;
}
