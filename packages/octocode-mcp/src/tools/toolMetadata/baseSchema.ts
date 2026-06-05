import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';
import { getMetadataOrNull } from './state.js';

const VERBOSE_SCHEMA_DESCRIPTION =
  'Boolean detail switch shared by every tool query. false returns efficient research data; true includes extended metadata.';

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
    if (prop === 'verbose' && source[prop] === undefined) {
      return VERBOSE_SCHEMA_DESCRIPTION;
    }
    return source[prop];
  },
  ownKeys() {
    return Array.from(
      new Set([...Reflect.ownKeys(getBaseSchemaSource()), 'verbose'])
    );
  },
  getOwnPropertyDescriptor(_target, prop: PropertyKey) {
    const source = getBaseSchemaSource();
    if (prop === 'verbose' && source[prop] === undefined) {
      return {
        enumerable: true,
        configurable: true,
        value: VERBOSE_SCHEMA_DESCRIPTION,
      };
    }
    if (prop in source) {
      return {
        enumerable: true,
        configurable: true,
        value: source[prop],
      };
    }
    return undefined;
  },
}) as CompleteMetadata['baseSchema'] & { verbose: string };
