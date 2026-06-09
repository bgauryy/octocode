import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

export const TOOL_NAMES = new Proxy({} as CompleteMetadata['toolNames'], {
  get(_target, prop: string) {
    return completeMetadata.toolNames[
      prop as keyof CompleteMetadata['toolNames']
    ];
  },
  ownKeys() {
    return Object.keys(completeMetadata.toolNames);
  },
  getOwnPropertyDescriptor(_target, prop) {
    const source = completeMetadata.toolNames;
    if (prop in source) {
      return {
        enumerable: true,
        configurable: true,
        value: source[prop as keyof typeof source],
      };
    }
    return undefined;
  },
}) as CompleteMetadata['toolNames'];
