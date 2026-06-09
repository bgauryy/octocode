import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

/**
 * Tool metadata is sourced synchronously from @octocodeai/octocode-core.
 * This async accessor is kept as the stable public API used by octocode-cli
 * and the server startup sequence.
 */
export async function loadToolContent(): Promise<CompleteMetadata> {
  return completeMetadata;
}
