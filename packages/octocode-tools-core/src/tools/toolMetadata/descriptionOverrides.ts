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

function withLocalFindFilesTruth(description: string): string {
  if (description.includes('Nothing is excluded by default'))
    return description;
  if (LOCAL_FIND_FILES_STALE.test(description)) {
    return description.replace(LOCAL_FIND_FILES_STALE, LOCAL_FIND_FILES_TRUTH);
  }
  return `${description} ${LOCAL_FIND_FILES_TRUTH}`;
}

// Core only mentions type:"prs" and type:"commits"; the runtime also supports
// type:"issues" (search/read GitHub issues) and type:"releases". Patch the
// description so agents can discover issue-fetch mode and issueNumber.
const GH_HISTORY_RESEARCH_STALE =
  /type:"prs" searches PRs; add prNumber to read selected content\. type:"commits" reads owner\/repo\/path history\./;

const GH_HISTORY_RESEARCH_TRUTH =
  'type:"prs" searches PRs; add prNumber to read selected content. type:"commits" reads owner/repo/path history. type:"issues" searches or reads GitHub issues; add issueNumber to read a specific issue (body + comments). type:"releases" lists repo releases.';

const GH_HISTORY_RESEARCH_EXTRA =
  'type:"issues" searches or reads GitHub issues; add issueNumber to read a specific issue (body + comments). type:"releases" lists repo releases.';

function withGhHistoryResearchTruth(description: string): string {
  if (
    description.includes('type:"issues"') &&
    description.includes('type:"releases"')
  ) {
    return description;
  }
  if (GH_HISTORY_RESEARCH_STALE.test(description)) {
    return description.replace(
      GH_HISTORY_RESEARCH_STALE,
      GH_HISTORY_RESEARCH_TRUTH
    );
  }
  return `${description} ${GH_HISTORY_RESEARCH_EXTRA}`;
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
  if (
    findFilesTool?.description &&
    !findFilesTool.description.includes('Nothing is excluded by default')
  ) {
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

  // Patch ghHistoryResearch — add type:"issues" and type:"releases" modes
  const historyTool = next.tools?.ghHistoryResearch;
  if (
    historyTool?.description &&
    (!historyTool.description.includes('type:"issues"') ||
      !historyTool.description.includes('type:"releases"'))
  ) {
    next = {
      ...next,
      tools: {
        ...next.tools,
        ghHistoryResearch: {
          ...historyTool,
          description: withGhHistoryResearchTruth(historyTool.description),
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
