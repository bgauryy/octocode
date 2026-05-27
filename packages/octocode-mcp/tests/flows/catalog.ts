export const FLOW_CATALOG = {
  localWhereIsXDefined: {
    id: 'local.where-is-x-defined',
    description:
      'Search for a symbol, resolve its definition, and inspect the implementation.',
    docUrl:
      'https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md#L466-L478',
  },
  localImpactAnalysis: {
    id: 'local.impact-analysis',
    description:
      'Find symbol references, inspect callers, and read one impacted call site.',
    docUrl:
      'https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md#L592-L608',
  },
  remoteSearchToFetchContent: {
    id: 'remote.search-to-fetch-content',
    description:
      'Search remote code through the active provider and fetch one matched file.',
    docUrl:
      'https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_TOOLS_REFERENCE.md',
  },
} as const;

export type FlowId = (typeof FLOW_CATALOG)[keyof typeof FLOW_CATALOG]['id'];
