import { getPatchedToolMetadata } from './descriptionOverrides.js';

export const DESCRIPTIONS = new Proxy({} as Record<string, string>, {
  get(_target, prop: string) {
    return getPatchedToolMetadata().tools[prop]?.description ?? '';
  },
});
