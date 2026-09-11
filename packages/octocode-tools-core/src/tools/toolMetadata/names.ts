import { STATIC_TOOL_NAMES } from '@octocodeai/octocode-core/schema';

export const TOOL_NAMES = {
  ...STATIC_TOOL_NAMES,
  // Internal processors share the one public GitHub discovery identity.
  GITHUB_SEARCH_CODE: STATIC_TOOL_NAMES.GITHUB_SEARCH,
  GITHUB_VIEW_REPO_STRUCTURE: STATIC_TOOL_NAMES.GITHUB_SEARCH,
  GITHUB_SEARCH_REPOSITORIES: STATIC_TOOL_NAMES.GITHUB_SEARCH,
  LOCAL_RIPGREP: 'local.text',
} as const;
