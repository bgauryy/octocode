import { completeMetadata } from '@octocodeai/octocode-core';
import type { CompleteMetadata } from '@octocodeai/octocode-core/types';

/**
 * Core (`@octocodeai/octocode-core`) still ships stale tool-level prose for a
 * few tools. Field-level overrides in this package already match runtime
 * behavior; rewrite the composed `description` here so CLI `--scheme` and MCP
 * registration stay consistent until the next core release.
 */
const LOCAL_FIND_FILES_STALE_PATTERNS = [
  /Default excludeDir skips common generated\/vendor dirs; pass \[\] to search all\./,
  /node_modules\/\.git\/dist\/build\/out\/coverage\/target\/\.next\/\.cache are pruned by default — pass excludeDir explicitly \(including excludeDir: \[\] to prune nothing\) to inspect installed deps or build output\./,
  /Nothing is excluded by default — pass excludeDir \(e\.g\. \["node_modules","dist","coverage"\]\) to prune build\/vendor dirs\./,
];

const LOCAL_FIND_FILES_TRUTH =
  'localFindFiles prunes common generated/vendor dirs by default (node_modules, .git, dist, build, out, coverage, target, .next, .cache); pass excludeDir: [] to prune nothing, or pass excludeDir explicitly to choose pruned dirs.';

function withLocalFindFilesTruth(description: string): string {
  let next = description;
  for (const pattern of LOCAL_FIND_FILES_STALE_PATTERNS) {
    next = next
      .replace(pattern, '')
      .replace(/\s{2,}/g, ' ')
      .trim();
  }
  if (next.includes('prunes common generated/vendor dirs by default'))
    return next;
  return `${next} ${LOCAL_FIND_FILES_TRUTH}`.replace(/\s{2,}/g, ' ').trim();
}

let patched: CompleteMetadata | null = null;

export function getPatchedToolMetadata(
  source: CompleteMetadata = completeMetadata
): CompleteMetadata {
  if (patched && source === completeMetadata) {
    return patched;
  }

  let next = source;

  // Patch localFindFiles
  const findFilesTool = next.tools?.localFindFiles;
  if (findFilesTool?.description) {
    next = {
      ...next,
      tools: {
        ...next.tools,
        localFindFiles: {
          ...findFilesTool,
          description: withLocalFindFilesTruth(findFilesTool.description),
        },
      },
    };
  }

  if (source === completeMetadata) patched = next;
  return next;
}

/** Test helper — clear memoization between cases. */
export function _resetDescriptionOverrideCache(): void {
  patched = null;
}
