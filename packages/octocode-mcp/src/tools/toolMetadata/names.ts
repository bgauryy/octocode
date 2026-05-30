import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';
import { getMetadataOrNull } from './state.js';

export const TOOL_NAMES = new Proxy({} as CompleteMetadata['toolNames'], {
  get(_target, prop: string) {
    const metadata = getMetadataOrNull();
    if (metadata) {
      const value =
        metadata.toolNames[prop as keyof CompleteMetadata['toolNames']];
      if (value !== undefined) return value;
    }
    return completeMetadata.toolNames[
      prop as keyof CompleteMetadata['toolNames']
    ];
  },
  ownKeys() {
    const metadata = getMetadataOrNull();
    return Object.keys((metadata ?? completeMetadata).toolNames);
  },
  getOwnPropertyDescriptor(_target, prop) {
    const metadata = getMetadataOrNull();
    const source = (metadata ?? completeMetadata).toolNames;
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
