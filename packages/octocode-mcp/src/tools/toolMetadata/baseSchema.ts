import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

function getBaseSchemaSource(): Record<PropertyKey, unknown> {
  // BaseSchema has no index signature; cast via unknown to allow Proxy-style
  // PropertyKey access without losing the strongly-typed Proxy facade.
  return completeMetadata.baseSchema as unknown as Record<PropertyKey, unknown>;
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
