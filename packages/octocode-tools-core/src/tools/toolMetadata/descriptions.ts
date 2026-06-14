import { completeMetadata } from '@octocodeai/octocode-core';

export const DESCRIPTIONS = new Proxy({} as Record<string, string>, {
  get(_target, prop: string) {
    return completeMetadata.tools[prop]?.description ?? '';
  },
});
