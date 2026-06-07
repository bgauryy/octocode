import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';
import { getMetadataOrNull } from './state.js';

function getBaseSchemaSource(): Record<PropertyKey, unknown> {
  const metadata = getMetadataOrNull();
  return (metadata ?? completeMetadata).baseSchema as unknown as Record<
    PropertyKey,
    unknown
  >;
}

export const BASE_SCHEMA = new Proxy({} as CompleteMetadata['baseSchema'], {
  get(_target, prop: PropertyKey) {
    const source = getBaseSchemaSource();
    return source[prop];
  },
  ownKeys() {
    return Array.from(new Set([...Reflect.ownKeys(getBaseSchemaSource())]));
  },
  getOwnPropertyDescriptor(_target, prop: PropertyKey) {
    const source = getBaseSchemaSource();
    if (prop in source) {
      return {
        enumerable: true,
        configurable: true,
        value: source[prop],
      };
    }
    return undefined;
  },
}) as CompleteMetadata['baseSchema'];
