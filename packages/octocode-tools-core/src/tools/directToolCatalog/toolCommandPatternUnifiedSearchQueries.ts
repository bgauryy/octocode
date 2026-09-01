import { GITHUB_SEARCH_TOOL_NAME } from '../toolNames.js';

type CommandPattern = {
  label: string;
  query: Record<string, unknown>;
};

export function buildUnifiedSearchCommandPatternQueries(
  toolName: string
): CommandPattern[] {
  if (toolName !== GITHUB_SEARCH_TOOL_NAME) return [];
  return [
    {
      label: 'code search',
      query: {
        operation: 'code',
        keywords: ['localSearch'],
        owner: 'bgauryy',
        repo: 'octocode',
        pageSize: 5,
      },
    },
    {
      label: 'repository search',
      query: {
        operation: 'repositories',
        keywords: ['code research'],
        language: 'TypeScript',
        pageSize: 5,
      },
    },
    {
      label: 'repository tree',
      query: {
        operation: 'tree',
        owner: 'bgauryy',
        repo: 'octocode',
        path: 'packages',
        maxDepth: 2,
      },
    },
  ];
}
