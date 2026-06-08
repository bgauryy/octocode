import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import type { HintContext, HintStatus, ToolHintGenerators } from './types.js';

import { hints as localRipgrepHints } from '../tools/local_ripgrep/hints.js';
import { hints as localFetchContentHints } from '../tools/local_fetch_content/hints.js';
import { hints as localViewStructureHints } from '../tools/local_view_structure/hints.js';
import { hints as localFindFilesHints } from '../tools/local_find_files/hints.js';
import { hints as githubSearchCodeHints } from '../tools/github_search_code/hints.js';
import { hints as githubFetchContentHints } from '../tools/github_fetch_content/hints.js';
import { hints as githubViewRepoStructureHints } from '../tools/github_view_repo_structure/hints.js';
import { hints as githubSearchPullRequestsHints } from '../tools/github_search_pull_requests/hints.js';
import { hints as githubSearchReposHints } from '../tools/github_search_repos/hints.js';
import { hints as githubCloneRepoHints } from '../tools/github_clone_repo/hints.js';
import { hints as packageSearchHints } from '../tools/package_search/hints.js';
import { hints as semanticContentHints } from '../tools/lsp/semantic_content/hints.js';
import { hints as diagnosticsHints } from '../tools/lsp/diagnostics/hints.js';
import {
  LSP_GET_DIAGNOSTICS_TOOL_NAME,
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
} from '../tools/lsp/shared/semanticTypes.js';

export const HINTS: Record<string, ToolHintGenerators> = {
  [STATIC_TOOL_NAMES.LOCAL_RIPGREP]: localRipgrepHints,
  [STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT]: localFetchContentHints,
  [STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE]: localViewStructureHints,
  [STATIC_TOOL_NAMES.LOCAL_FIND_FILES]: localFindFilesHints,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]: githubSearchCodeHints,
  [STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT]: githubFetchContentHints,
  [STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE]: githubViewRepoStructureHints,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS]:
    githubSearchPullRequestsHints,
  [STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES]: githubSearchReposHints,
  [STATIC_TOOL_NAMES.GITHUB_CLONE_REPO]: githubCloneRepoHints,
  [STATIC_TOOL_NAMES.PACKAGE_SEARCH]: packageSearchHints,
  [LSP_GET_SEMANTIC_CONTENT_TOOL_NAME]: semanticContentHints,
  [LSP_GET_DIAGNOSTICS_TOOL_NAME]: diagnosticsHints,
};

type DynamicToolName = keyof typeof HINTS;

export function hasDynamicHints(toolName: string): toolName is DynamicToolName {
  return toolName in HINTS;
}

export function getDynamicHints(
  toolName: string,
  status: HintStatus,
  context?: HintContext
): string[] {
  const hintGenerator = HINTS[toolName]?.[status];
  if (!hintGenerator) return [];

  const rawHints = hintGenerator(context || {});

  return rawHints.filter((h): h is string => typeof h === 'string');
}
