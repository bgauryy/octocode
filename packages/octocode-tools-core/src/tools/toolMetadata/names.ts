import { STATIC_TOOL_NAMES } from '../toolNames.js';

export const TOOL_NAMES = {
  ...STATIC_TOOL_NAMES,
  LOCAL_SEARCH: 'localSearch',
  // Internal engines retained behind the unified public contracts.
  GITHUB_SEARCH_CODE: 'github.code',
  GITHUB_VIEW_REPO_STRUCTURE: 'github.tree',
  GITHUB_SEARCH_REPOSITORIES: 'github.repositories',
  LOCAL_RIPGREP: 'local.text',
  LOCAL_VIEW_STRUCTURE: 'local.tree',
  LOCAL_FIND_FILES: 'local.files',
} as const;
