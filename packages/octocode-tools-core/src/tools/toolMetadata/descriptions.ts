import { completeMetadata } from '@octocodeai/octocode-core';

// Descriptions come from @octocodeai/octocode-core verbatim — single source
// of truth (src/resources/tools). Never patch prose here; fix it in core.
export const DESCRIPTIONS = new Proxy({} as Record<string, string>, {
  get(_target, prop: string) {
    return completeMetadata.tools[prop]?.description ?? '';
  },
});
