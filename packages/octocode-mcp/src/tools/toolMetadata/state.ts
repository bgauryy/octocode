import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

// Shared cache — populated on first access, resets on vi.resetModules() in tests.
let _cached: CompleteMetadata | null = null;

function ensureLoaded(): CompleteMetadata {
  if (!_cached) _cached = completeMetadata;
  return _cached;
}

/**
 * Tool metadata is sourced synchronously from @octocodeai/octocode-core.
 * This async accessor is kept as the stable public API used by octocode-cli
 * and the server startup sequence.
 */
export async function loadToolContent(): Promise<CompleteMetadata> {
  return ensureLoaded();
}

/**
 * Compatibility wrapper for callers that expect an async init step.
 * Reads completeMetadata exactly once; subsequent calls are no-ops.
 */
export async function initializeToolMetadata(): Promise<void> {
  ensureLoaded();
}

/**
 * Returns the cached metadata snapshot, or null if not yet initialized.
 */
export function getMetadataOrNull(): CompleteMetadata | null {
  return _cached;
}

/**
 * Resets the internal cache. Used in tests with vi.resetModules() to ensure
 * a clean state between test runs.
 * @internal
 */
export function _resetMetadataState(): void {
  _cached = null;
}
