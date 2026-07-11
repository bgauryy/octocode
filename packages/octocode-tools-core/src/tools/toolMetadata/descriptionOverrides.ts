import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

/**
 * Core (`@octocodeai/octocode-core`) still ships stale tool-level prose for a
 * few tools. Field-level overrides in this package already match runtime
 * behavior; rewrite the composed `description` here so CLI `--scheme` and MCP
 * registration stay consistent until the next core release.
 */
const LOCAL_FIND_FILES_STALE =
  /Default excludeDir skips common generated\/vendor dirs; pass \[\] to search all\./;

const LOCAL_FIND_FILES_TRUTH =
  'Nothing is excluded by default — pass excludeDir (e.g. ["node_modules","dist","coverage"]) to prune build/vendor dirs.';

let patched: CompleteMetadata | null = null;

export function getPatchedToolMetadata(
  source: CompleteMetadata = completeMetadata
): CompleteMetadata {
  if (patched && source === completeMetadata) {
    return patched;
  }

  const tool = source.tools?.localFindFiles;
  if (!tool?.description || !LOCAL_FIND_FILES_STALE.test(tool.description)) {
    if (source === completeMetadata) patched = source;
    return source;
  }

  const next: CompleteMetadata = {
    ...source,
    tools: {
      ...source.tools,
      localFindFiles: {
        ...tool,
        description: tool.description.replace(
          LOCAL_FIND_FILES_STALE,
          LOCAL_FIND_FILES_TRUTH
        ),
      },
    },
  };
  if (source === completeMetadata) patched = next;
  return next;
}

/** Test helper — clear memoization between cases. */
export function _resetDescriptionOverrideCache(): void {
  patched = null;
}
