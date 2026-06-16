export { TOOL_NAMES } from './names.js';
export { DESCRIPTIONS } from './descriptions.js';
export { isToolInMetadata } from './metadataPresence.js';
export { BASE_SCHEMA } from './baseSchema.js';

import { completeMetadata } from '@octocodeai/octocode-core';

type HintsMap = Record<string, readonly string[]>;

function resolveToolHints(toolName: string): HintsMap {
  const hints = completeMetadata.tools[toolName]?.hints as unknown as
    | HintsMap
    | undefined;
  return hints ?? { hasResults: [], empty: [] };
}

export const TOOL_HINTS = new Proxy({} as Record<string, HintsMap>, {
  get(_target, prop: PropertyKey) {
    if (typeof prop !== 'string') return undefined;
    if (prop === 'base')
      return completeMetadata.baseHints as unknown as HintsMap;
    return resolveToolHints(prop);
  },
  ownKeys() {
    return ['base', ...Object.keys(completeMetadata.tools)];
  },
  getOwnPropertyDescriptor(_target, prop: PropertyKey) {
    if (typeof prop !== 'string') return undefined;
    if (prop === 'base' || prop in completeMetadata.tools) {
      return {
        enumerable: true,
        configurable: true,
        value:
          prop === 'base'
            ? (completeMetadata.baseHints as unknown as HintsMap)
            : resolveToolHints(prop),
      };
    }
    return undefined;
  },
});

export const GENERIC_ERROR_HINTS = new Proxy([] as readonly string[], {
  get(_target, prop: PropertyKey) {
    const hints = completeMetadata.genericErrorHints as unknown as Record<
      PropertyKey,
      unknown
    >;
    return hints[prop];
  },
});

export function getToolHintsSync(
  toolName: string,
  status: string
): readonly string[] {
  const hints = completeMetadata.tools[toolName]?.hints as unknown as
    | HintsMap
    | undefined;
  return hints?.[status] ?? [];
}

export function getDynamicHints(
  toolName: string,
  key: string
): readonly string[] {
  const hints = completeMetadata.tools[toolName]?.hints as unknown as
    | Record<string, unknown>
    | undefined;
  const dynamic = hints?.['dynamic'] as HintsMap | undefined;
  return dynamic?.[key] ?? [];
}

export function getGenericErrorHintsSync(): readonly string[] {
  return completeMetadata.genericErrorHints;
}
