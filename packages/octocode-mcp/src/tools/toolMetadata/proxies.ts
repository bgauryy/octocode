export { TOOL_NAMES } from './names.js';
export { DESCRIPTIONS } from './descriptions.js';
export { isToolInMetadata } from './metadataPresence.js';
export { BASE_SCHEMA } from './baseSchema.js';

import { completeMetadata } from '@octocodeai/octocode-core';

// ── TOOL_HINTS ────────────────────────────────────────────────────────────────

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

// ── GENERIC_ERROR_HINTS ───────────────────────────────────────────────────────

export const GENERIC_ERROR_HINTS = new Proxy([] as readonly string[], {
  get(_target, prop: PropertyKey) {
    // readonly string[] does not carry an index signature — cast via unknown
    // to allow Proxy-style PropertyKey access to array indices and methods.
    const hints = completeMetadata.genericErrorHints as unknown as Record<
      PropertyKey,
      unknown
    >;
    return hints[prop];
  },
});

// ── Helper functions ──────────────────────────────────────────────────────────

/**
 * Returns hints for the given tool and result status (`"hasResults"`, `"empty"`, …).
 * Returns `[]` when the tool or status key is absent.
 */
export function getToolHintsSync(
  toolName: string,
  status: string
): readonly string[] {
  const hints = completeMetadata.tools[toolName]?.hints as unknown as
    | HintsMap
    | undefined;
  return hints?.[status] ?? [];
}

/**
 * Returns dynamic hints for the given tool and key from `hints.dynamic`.
 * Returns `[]` when absent.
 */
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

/**
 * Returns the generic error hints array.
 */
export function getGenericErrorHintsSync(): readonly string[] {
  return completeMetadata.genericErrorHints;
}
