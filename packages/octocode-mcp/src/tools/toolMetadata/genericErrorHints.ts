import { completeMetadata } from '@octocodeai/octocode-core';
import { getMetadataOrNull } from './state.js';

/**
 * Proxy seed — empty mutable array typed as `readonly string[]` so the
 * Proxy<T> generic infers `readonly string[]` and no type cast is needed.
 *
 * The seed's own properties are never read: the `get` trap forwards every
 * lookup to the live metadata. An empty plain array satisfies Proxy's
 * non-configurable property invariants (no fixed indices, `length` is
 * writable on plain arrays).
 */
const PROXY_SEED: readonly string[] = [];

const liveHintsHandler: ProxyHandler<readonly string[]> = {
  get(_target, prop, receiver) {
    const metadata = getMetadataOrNull() ?? completeMetadata;
    return Reflect.get(metadata.genericErrorHints, prop, receiver);
  },
};

export const GENERIC_ERROR_HINTS: readonly string[] = new Proxy(
  PROXY_SEED,
  liveHintsHandler
);
