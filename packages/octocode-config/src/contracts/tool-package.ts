import { TOOL_NAMES } from '@octocodeai/octocode-core/schema';

/** The CLI and MCP distribution that owns this tool's public entry points. */
export function toolPackage(name: string): 'octocode' | 'octocode-graph' {
  return name === TOOL_NAMES.GRAPH_BUILD || name === TOOL_NAMES.GRAPH_QUERY
    ? 'octocode-graph'
    : 'octocode';
}
