import { completeMetadata } from '@octocodeai/octocode-core';
import { getMetadataOrNull } from './state.js';

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
